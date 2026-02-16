//! Collector for pg_stat_user_indexes (per-database view).
//!
//! Collects from all databases via `db_clients` pool.

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatUserIndexesInfo;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use std::collections::HashMap;

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
        // Return cached data if fresh (re-intern strings for current interner state)
        if let Some(cache_time) = self.indexes_cache_time
            && self.statements_collect_interval > std::time::Duration::ZERO
            && cache_time.elapsed() < self.statements_collect_interval
            && !self.indexes_cache.is_empty()
        {
            return Ok(self
                .indexes_cache
                .iter()
                .map(|entry| {
                    let mut info = entry.info.clone();
                    info.datname_hash = interner.intern(&entry.datname);
                    info.schemaname_hash = interner.intern(&entry.schemaname);
                    info.relname_hash = interner.intern(&entry.relname);
                    info.indexrelname_hash = interner.intern(&entry.indexrelname);
                    info
                })
                .collect());
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

        Ok(all_results)
    }
}

/// Safely parses a single row from pg_stat_user_indexes query.
/// Returns None if any column fails to deserialize (instead of panicking).
fn parse_index_row(
    row: &postgres::Row,
    interner: &mut StringInterner,
    collected_at: i64,
    datname: &str,
) -> Option<(PgStatUserIndexesInfo, String, String, String)> {
    let indexrelid: u32 = row.try_get::<_, i64>(0).ok()? as u32;
    let relid: u32 = row.try_get::<_, i64>(1).ok()? as u32;
    let schemaname: String = row.try_get(2).unwrap_or_default();
    let relname: String = row.try_get(3).unwrap_or_default();
    let indexrelname: String = row.try_get(4).unwrap_or_default();

    let info = PgStatUserIndexesInfo {
        indexrelid,
        relid,
        datname_hash: interner.intern(datname),
        schemaname_hash: interner.intern(&schemaname),
        relname_hash: interner.intern(&relname),
        indexrelname_hash: interner.intern(&indexrelname),
        idx_scan: row.try_get(5).unwrap_or(0),
        idx_tup_read: row.try_get(6).unwrap_or(0),
        idx_tup_fetch: row.try_get(7).unwrap_or(0),
        size_bytes: row.try_get(8).unwrap_or(0),
        idx_blks_read: 0,
        idx_blks_hit: 0,
        collected_at,
    };

    Some((info, schemaname, relname, indexrelname))
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
