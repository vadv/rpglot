//! Collector for pg_stat_user_tables (per-database view) with activity-only filtering.
//!
//! Collects from all databases via `db_clients` pool.

use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatUserTablesInfo;

use super::PgCollectError;
use super::PostgresCollector;
use super::queries::{build_stat_user_tables_query, build_statio_user_tables_query};

/// Cache entry for pg_stat_user_tables rows.
/// Stores original strings for re-interning on each call.
#[derive(Clone)]
pub(crate) struct PgStatUserTablesCacheEntry {
    pub info: PgStatUserTablesInfo,
    pub datname: String,
    pub schemaname: String,
    pub relname: String,
}

impl PostgresCollector {
    /// Collects pg_stat_user_tables statistics from all connected databases.
    ///
    /// Uses 30-second caching (same interval as pg_stat_statements).
    /// Returns cached data with re-interned strings if cache is fresh.
    pub fn collect_tables(
        &mut self,
        interner: &mut StringInterner,
    ) -> Result<Vec<PgStatUserTablesInfo>, PgCollectError> {
        // Return cached filtered data if fresh (re-intern strings for current interner state)
        if let Some(cache_time) = self.tables_cache_time
            && self.statements_collect_interval > std::time::Duration::ZERO
            && cache_time.elapsed() < self.statements_collect_interval
            && !self.tables_cache.is_empty()
        {
            return Ok(self.return_filtered_tables_cached(interner));
        }

        let collected_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let query = build_stat_user_tables_query();
        let statio_query = build_statio_user_tables_query();

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
                let Some(info) = parse_table_row(row, interner, collected_at, &datname) else {
                    continue;
                };

                cache.push(PgStatUserTablesCacheEntry {
                    info: info.0.clone(),
                    datname: datname.clone(),
                    schemaname: info.1,
                    relname: info.2,
                });
                results.push(info.0);
            }

            // Merge pg_statio_user_tables I/O counters by relid
            let statio_rows = db_client
                .client
                .query(statio_query, &[])
                .unwrap_or_default();
            let statio_map: HashMap<u32, StatioRow> = statio_rows
                .iter()
                .filter_map(|row| parse_statio_row(row).map(|s| (s.relid, s)))
                .collect();

            for info in &mut results {
                if let Some(s) = statio_map.get(&info.relid) {
                    s.apply(info);
                }
            }
            for entry in &mut cache {
                if let Some(s) = statio_map.get(&entry.info.relid) {
                    s.apply(&mut entry.info);
                }
            }

            all_results.extend(results);
            all_cache.extend(cache);
        }

        self.tables_cache = all_cache;
        self.tables_cache_time = Some(Instant::now());

        // Activity-only filtering: only keep rows where cumulative counters changed.
        let filtered = filter_tables(&all_results, &self.pgt_prev, self.pgt_first_collect);

        // Update prev with the full (unfiltered) snapshot.
        self.pgt_prev = all_results
            .iter()
            .map(|info| (info.relid, info.clone()))
            .collect();
        self.pgt_first_collect = false;
        self.pgt_filtered_cache = filtered.clone();

        Ok(filtered)
    }

    /// Returns the cached filtered result with re-interned strings.
    fn return_filtered_tables_cached(
        &self,
        interner: &mut StringInterner,
    ) -> Vec<PgStatUserTablesInfo> {
        let cache_map: HashMap<u32, &PgStatUserTablesCacheEntry> = self
            .tables_cache
            .iter()
            .map(|e| (e.info.relid, e))
            .collect();

        self.pgt_filtered_cache
            .iter()
            .filter_map(|info| {
                let entry = cache_map.get(&info.relid)?;
                let mut out = info.clone();
                out.datname_hash = interner.intern(&entry.datname);
                out.schemaname_hash = interner.intern(&entry.schemaname);
                out.relname_hash = interner.intern(&entry.relname);
                Some(out)
            })
            .collect()
    }
}

/// Parsed statio columns for merge (not stored separately).
struct StatioRow {
    relid: u32,
    heap_blks_read: i64,
    heap_blks_hit: i64,
    idx_blks_read: i64,
    idx_blks_hit: i64,
    toast_blks_read: i64,
    toast_blks_hit: i64,
    tidx_blks_read: i64,
    tidx_blks_hit: i64,
}

impl StatioRow {
    fn apply(&self, info: &mut PgStatUserTablesInfo) {
        info.heap_blks_read = self.heap_blks_read;
        info.heap_blks_hit = self.heap_blks_hit;
        info.idx_blks_read = self.idx_blks_read;
        info.idx_blks_hit = self.idx_blks_hit;
        info.toast_blks_read = self.toast_blks_read;
        info.toast_blks_hit = self.toast_blks_hit;
        info.tidx_blks_read = self.tidx_blks_read;
        info.tidx_blks_hit = self.tidx_blks_hit;
    }
}

/// Safely parses a single row from pg_statio_user_tables query.
fn parse_statio_row(row: &postgres::Row) -> Option<StatioRow> {
    Some(StatioRow {
        relid: row.try_get::<_, i64>(0).ok()? as u32,
        heap_blks_read: row.try_get(1).unwrap_or(0),
        heap_blks_hit: row.try_get(2).unwrap_or(0),
        idx_blks_read: row.try_get(3).unwrap_or(0),
        idx_blks_hit: row.try_get(4).unwrap_or(0),
        toast_blks_read: row.try_get(5).unwrap_or(0),
        toast_blks_hit: row.try_get(6).unwrap_or(0),
        tidx_blks_read: row.try_get(7).unwrap_or(0),
        tidx_blks_hit: row.try_get(8).unwrap_or(0),
    })
}

/// Safely parses a single row from pg_stat_user_tables query.
/// Returns None if any column fails to deserialize (instead of panicking).
fn parse_table_row(
    row: &postgres::Row,
    interner: &mut StringInterner,
    collected_at: i64,
    datname: &str,
) -> Option<(PgStatUserTablesInfo, String, String)> {
    let relid: u32 = row.try_get::<_, i64>(0).ok()? as u32;
    let schemaname: String = row.try_get(1).unwrap_or_default();
    let relname: String = row.try_get(2).unwrap_or_default();

    let info = PgStatUserTablesInfo {
        relid,
        datname_hash: interner.intern(datname),
        schemaname_hash: interner.intern(&schemaname),
        relname_hash: interner.intern(&relname),
        seq_scan: row.try_get(3).unwrap_or(0),
        seq_tup_read: row.try_get(4).unwrap_or(0),
        idx_scan: row.try_get(5).unwrap_or(0),
        idx_tup_fetch: row.try_get(6).unwrap_or(0),
        n_tup_ins: row.try_get(7).unwrap_or(0),
        n_tup_upd: row.try_get(8).unwrap_or(0),
        n_tup_del: row.try_get(9).unwrap_or(0),
        n_tup_hot_upd: row.try_get(10).unwrap_or(0),
        n_live_tup: row.try_get(11).unwrap_or(0),
        n_dead_tup: row.try_get(12).unwrap_or(0),
        vacuum_count: row.try_get(13).unwrap_or(0),
        autovacuum_count: row.try_get(14).unwrap_or(0),
        analyze_count: row.try_get(15).unwrap_or(0),
        autoanalyze_count: row.try_get(16).unwrap_or(0),
        last_vacuum: row.try_get(17).unwrap_or(0),
        last_autovacuum: row.try_get(18).unwrap_or(0),
        last_analyze: row.try_get(19).unwrap_or(0),
        last_autoanalyze: row.try_get(20).unwrap_or(0),
        size_bytes: row.try_get(21).unwrap_or(0),
        heap_blks_read: 0,
        heap_blks_hit: 0,
        idx_blks_read: 0,
        idx_blks_hit: 0,
        toast_blks_read: 0,
        toast_blks_hit: 0,
        tidx_blks_read: 0,
        tidx_blks_hit: 0,
        collected_at,
    };

    Some((info, schemaname, relname))
}

/// Filters tables to only include rows where any cumulative counter changed.
///
/// On first collect (`first_collect == true`), returns all rows (full snapshot).
fn filter_tables(
    rows: &[PgStatUserTablesInfo],
    prev: &HashMap<u32, PgStatUserTablesInfo>,
    first_collect: bool,
) -> Vec<PgStatUserTablesInfo> {
    if first_collect {
        return rows.to_vec();
    }
    rows.iter()
        .filter(|row| match prev.get(&row.relid) {
            Some(prev_row) => table_changed(row, prev_row),
            None => true,
        })
        .cloned()
        .collect()
}

/// Returns true if any cumulative counter changed between two snapshots of the same relid.
fn table_changed(curr: &PgStatUserTablesInfo, prev: &PgStatUserTablesInfo) -> bool {
    curr.seq_scan != prev.seq_scan
        || curr.seq_tup_read != prev.seq_tup_read
        || curr.idx_scan != prev.idx_scan
        || curr.idx_tup_fetch != prev.idx_tup_fetch
        || curr.n_tup_ins != prev.n_tup_ins
        || curr.n_tup_upd != prev.n_tup_upd
        || curr.n_tup_del != prev.n_tup_del
        || curr.n_tup_hot_upd != prev.n_tup_hot_upd
        || curr.vacuum_count != prev.vacuum_count
        || curr.autovacuum_count != prev.autovacuum_count
        || curr.analyze_count != prev.analyze_count
        || curr.autoanalyze_count != prev.autoanalyze_count
        || curr.heap_blks_read != prev.heap_blks_read
        || curr.heap_blks_hit != prev.heap_blks_hit
        || curr.idx_blks_read != prev.idx_blks_read
        || curr.idx_blks_hit != prev.idx_blks_hit
        || curr.toast_blks_read != prev.toast_blks_read
        || curr.toast_blks_hit != prev.toast_blks_hit
        || curr.tidx_blks_read != prev.tidx_blks_read
        || curr.tidx_blks_hit != prev.tidx_blks_hit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_tables_first_collect_returns_all() {
        let rows = vec![
            PgStatUserTablesInfo {
                relid: 1,
                seq_scan: 10,
                ..Default::default()
            },
            PgStatUserTablesInfo {
                relid: 2,
                seq_scan: 5,
                ..Default::default()
            },
        ];
        let prev = HashMap::new();
        let filtered = filter_tables(&rows, &prev, true);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_tables_removes_unchanged() {
        let unchanged = PgStatUserTablesInfo {
            relid: 1,
            seq_scan: 10,
            ..Default::default()
        };
        let changed = PgStatUserTablesInfo {
            relid: 2,
            seq_scan: 20,
            n_tup_ins: 5,
            ..Default::default()
        };

        let rows = vec![unchanged.clone(), changed.clone()];

        let mut prev = HashMap::new();
        prev.insert(1, unchanged.clone());
        prev.insert(
            2,
            PgStatUserTablesInfo {
                relid: 2,
                seq_scan: 19,
                n_tup_ins: 5,
                ..Default::default()
            },
        );

        let filtered = filter_tables(&rows, &prev, false);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].relid, 2);
    }

    #[test]
    fn table_changed_detects_all_cumulative_fields() {
        let base = PgStatUserTablesInfo::default();
        assert!(!table_changed(&base, &base));

        let fields: Vec<fn(&mut PgStatUserTablesInfo)> = vec![
            |t| t.seq_scan = 1,
            |t| t.seq_tup_read = 1,
            |t| t.idx_scan = 1,
            |t| t.idx_tup_fetch = 1,
            |t| t.n_tup_ins = 1,
            |t| t.n_tup_upd = 1,
            |t| t.n_tup_del = 1,
            |t| t.n_tup_hot_upd = 1,
            |t| t.vacuum_count = 1,
            |t| t.autovacuum_count = 1,
            |t| t.analyze_count = 1,
            |t| t.autoanalyze_count = 1,
            |t| t.heap_blks_read = 1,
            |t| t.heap_blks_hit = 1,
            |t| t.idx_blks_read = 1,
            |t| t.idx_blks_hit = 1,
            |t| t.toast_blks_read = 1,
            |t| t.toast_blks_hit = 1,
            |t| t.tidx_blks_read = 1,
            |t| t.tidx_blks_hit = 1,
        ];

        for mutator in fields {
            let mut m = base.clone();
            mutator(&mut m);
            assert!(table_changed(&m, &base));
        }
    }

    #[test]
    fn table_changed_ignores_gauge_fields() {
        let base = PgStatUserTablesInfo::default();

        // Gauge fields should NOT trigger change.
        let mut m = base.clone();
        m.n_live_tup = 1000;
        m.n_dead_tup = 500;
        m.size_bytes = 999999;
        m.last_vacuum = 12345;
        m.last_autovacuum = 12345;
        m.last_analyze = 12345;
        m.last_autoanalyze = 12345;
        assert!(!table_changed(&m, &base));
    }
}
