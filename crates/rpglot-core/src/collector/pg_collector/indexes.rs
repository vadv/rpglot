//! Collector for pg_stat_user_indexes (per-database view) with activity-only filtering.
//!
//! Collects from all databases via `db_clients` pool.

use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatUserIndexesInfo;

use super::PgCollectError;
use super::PostgresCollector;
use super::queries::{build_stat_user_indexes_query, build_statio_user_indexes_query};

/// Cache entry for pg_stat_user_indexes rows.
/// Stores original strings for re-interning on each call.
#[derive(Clone)]
pub(crate) struct PgStatUserIndexesCacheEntry {
    pub info: PgStatUserIndexesInfo,
    pub datname: String,
    pub schemaname: String,
    pub relname: String,
    pub indexrelname: String,
    pub tablespace: String,
}

impl PostgresCollector {
    /// Collects pg_stat_user_indexes statistics from all connected databases.
    ///
    /// Uses 30-second caching (same interval as pg_stat_statements).
    /// Returns cached data with re-interned strings if cache is fresh.
    pub fn collect_indexes(
        &mut self,
        interner: &mut StringInterner,
    ) -> Result<Vec<PgStatUserIndexesInfo>, PgCollectError> {
        // Return cached filtered data if fresh (re-intern strings for current interner state)
        if let Some(cache_time) = self.indexes_cache_time
            && self.statements_collect_interval > std::time::Duration::ZERO
            && cache_time.elapsed() < self.statements_collect_interval
            && !self.indexes_cache.is_empty()
        {
            return Ok(self.return_filtered_indexes_cached(interner));
        }

        let collected_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let query = build_stat_user_indexes_query();
        let statio_query = build_statio_user_indexes_query();

        let mut all_results = Vec::new();
        let mut all_cache = Vec::new();

        for db_client in &mut self.db_clients {
            let datname = db_client.datname.clone();

            let rows = match db_client.client.query(query, &[]) {
                Ok(rows) => rows,
                Err(_) => continue, // skip this database on error
            };

            let mut results = Vec::with_capacity(rows.len());
            let mut cache = Vec::with_capacity(rows.len());

            for row in &rows {
                let Some(info) = parse_index_row(row, interner, collected_at, &datname) else {
                    continue;
                };

                cache.push(PgStatUserIndexesCacheEntry {
                    info: info.0.clone(),
                    datname: datname.clone(),
                    schemaname: info.1,
                    relname: info.2,
                    indexrelname: info.3,
                    tablespace: info.4,
                });
                results.push(info.0);
            }

            // Merge I/O counters from pg_statio_user_indexes (graceful on failure)
            let statio_rows = db_client
                .client
                .query(statio_query, &[])
                .unwrap_or_default();
            let statio_map: HashMap<u32, IndexStatioRow> = statio_rows
                .iter()
                .filter_map(|row| parse_index_statio_row(row).map(|s| (s.indexrelid, s)))
                .collect();
            for r in &mut results {
                if let Some(s) = statio_map.get(&r.indexrelid) {
                    s.apply(r);
                }
            }
            for c in &mut cache {
                if let Some(s) = statio_map.get(&c.info.indexrelid) {
                    s.apply(&mut c.info);
                }
            }

            all_results.extend(results);
            all_cache.extend(cache);
        }

        self.indexes_cache = all_cache;
        self.indexes_cache_time = Some(Instant::now());

        // Activity-only filtering: only keep rows where cumulative counters changed.
        let filtered = filter_indexes(&all_results, &self.pgi_prev, self.pgi_first_collect);

        // Update prev with the full (unfiltered) snapshot.
        self.pgi_prev = all_results
            .iter()
            .map(|info| (info.indexrelid, info.clone()))
            .collect();
        self.pgi_first_collect = false;
        self.pgi_filtered_cache = filtered.clone();

        Ok(filtered)
    }

    /// Returns the cached filtered result with re-interned strings.
    fn return_filtered_indexes_cached(
        &self,
        interner: &mut StringInterner,
    ) -> Vec<PgStatUserIndexesInfo> {
        let cache_map: HashMap<u32, &PgStatUserIndexesCacheEntry> = self
            .indexes_cache
            .iter()
            .map(|e| (e.info.indexrelid, e))
            .collect();

        self.pgi_filtered_cache
            .iter()
            .filter_map(|info| {
                let entry = cache_map.get(&info.indexrelid)?;
                let mut out = info.clone();
                out.datname_hash = interner.intern(&entry.datname);
                out.schemaname_hash = interner.intern(&entry.schemaname);
                out.relname_hash = interner.intern(&entry.relname);
                out.indexrelname_hash = interner.intern(&entry.indexrelname);
                out.tablespace_hash = interner.intern(&entry.tablespace);
                Some(out)
            })
            .collect()
    }
}

/// Safely parses a single row from pg_stat_user_indexes query.
/// Returns None if any column fails to deserialize (instead of panicking).
fn parse_index_row(
    row: &postgres::Row,
    interner: &mut StringInterner,
    collected_at: i64,
    datname: &str,
) -> Option<(PgStatUserIndexesInfo, String, String, String, String)> {
    let indexrelid: u32 = row.try_get::<_, i64>(0).ok()? as u32;
    let relid: u32 = row.try_get::<_, i64>(1).ok()? as u32;
    let schemaname: String = row.try_get(2).unwrap_or_default();
    let relname: String = row.try_get(3).unwrap_or_default();
    let indexrelname: String = row.try_get(4).unwrap_or_default();
    let tablespace: String = row.try_get(5).unwrap_or_default();

    let info = PgStatUserIndexesInfo {
        indexrelid,
        relid,
        datname_hash: interner.intern(datname),
        schemaname_hash: interner.intern(&schemaname),
        relname_hash: interner.intern(&relname),
        indexrelname_hash: interner.intern(&indexrelname),
        tablespace_hash: interner.intern(&tablespace),
        idx_scan: row.try_get(6).unwrap_or(0),
        idx_tup_read: row.try_get(7).unwrap_or(0),
        idx_tup_fetch: row.try_get(8).unwrap_or(0),
        size_bytes: row.try_get(9).unwrap_or(0),
        idx_blks_read: 0,
        idx_blks_hit: 0,
        collected_at,
    };

    Some((info, schemaname, relname, indexrelname, tablespace))
}

struct IndexStatioRow {
    indexrelid: u32,
    idx_blks_read: i64,
    idx_blks_hit: i64,
}

impl IndexStatioRow {
    fn apply(&self, info: &mut PgStatUserIndexesInfo) {
        info.idx_blks_read = self.idx_blks_read;
        info.idx_blks_hit = self.idx_blks_hit;
    }
}

fn parse_index_statio_row(row: &postgres::Row) -> Option<IndexStatioRow> {
    Some(IndexStatioRow {
        indexrelid: row.try_get::<_, i64>(0).ok()? as u32,
        idx_blks_read: row.try_get(1).unwrap_or(0),
        idx_blks_hit: row.try_get(2).unwrap_or(0),
    })
}

/// Filters indexes to only include rows where any cumulative counter changed.
///
/// On first collect (`first_collect == true`), returns all rows (full snapshot).
fn filter_indexes(
    rows: &[PgStatUserIndexesInfo],
    prev: &HashMap<u32, PgStatUserIndexesInfo>,
    first_collect: bool,
) -> Vec<PgStatUserIndexesInfo> {
    if first_collect {
        return rows.to_vec();
    }
    rows.iter()
        .filter(|row| match prev.get(&row.indexrelid) {
            Some(prev_row) => index_changed(row, prev_row),
            None => true,
        })
        .cloned()
        .collect()
}

/// Returns true if any cumulative counter changed between two snapshots of the same indexrelid.
fn index_changed(curr: &PgStatUserIndexesInfo, prev: &PgStatUserIndexesInfo) -> bool {
    curr.idx_scan != prev.idx_scan
        || curr.idx_tup_read != prev.idx_tup_read
        || curr.idx_tup_fetch != prev.idx_tup_fetch
        || curr.idx_blks_read != prev.idx_blks_read
        || curr.idx_blks_hit != prev.idx_blks_hit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_indexes_first_collect_returns_all() {
        let rows = vec![
            PgStatUserIndexesInfo {
                indexrelid: 1,
                idx_scan: 10,
                ..Default::default()
            },
            PgStatUserIndexesInfo {
                indexrelid: 2,
                idx_scan: 5,
                ..Default::default()
            },
        ];
        let prev = HashMap::new();
        let filtered = filter_indexes(&rows, &prev, true);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_indexes_removes_unchanged() {
        let unchanged = PgStatUserIndexesInfo {
            indexrelid: 1,
            idx_scan: 10,
            ..Default::default()
        };
        let changed = PgStatUserIndexesInfo {
            indexrelid: 2,
            idx_scan: 20,
            ..Default::default()
        };
        let new_row = PgStatUserIndexesInfo {
            indexrelid: 3,
            idx_scan: 1,
            ..Default::default()
        };

        let rows = vec![unchanged.clone(), changed.clone(), new_row.clone()];

        let mut prev = HashMap::new();
        prev.insert(1, unchanged.clone());
        prev.insert(
            2,
            PgStatUserIndexesInfo {
                indexrelid: 2,
                idx_scan: 19,
                ..Default::default()
            },
        );

        let filtered = filter_indexes(&rows, &prev, false);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].indexrelid, 2);
        assert_eq!(filtered[1].indexrelid, 3);
    }

    #[test]
    fn index_changed_detects_all_cumulative_fields() {
        let base = PgStatUserIndexesInfo::default();
        assert!(!index_changed(&base, &base));

        let fields: Vec<fn(&mut PgStatUserIndexesInfo)> = vec![
            |i| i.idx_scan = 1,
            |i| i.idx_tup_read = 1,
            |i| i.idx_tup_fetch = 1,
            |i| i.idx_blks_read = 1,
            |i| i.idx_blks_hit = 1,
        ];

        for mutator in fields {
            let mut m = base.clone();
            mutator(&mut m);
            assert!(index_changed(&m, &base));
        }
    }

    #[test]
    fn index_changed_ignores_non_cumulative_fields() {
        let base = PgStatUserIndexesInfo::default();

        let mut m = base.clone();
        m.size_bytes = 999999;
        assert!(!index_changed(&m, &base));
    }
}
