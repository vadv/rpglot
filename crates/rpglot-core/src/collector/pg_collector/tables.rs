//! Collector for pg_stat_user_tables (per-database view).

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatUserTablesInfo;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::PgCollectError;
use super::PostgresCollector;
use super::format_postgres_error;
use super::queries::{build_stat_user_tables_query, build_statio_user_tables_query};

/// Cache entry for pg_stat_user_tables rows.
/// Stores original strings for re-interning on each call.
#[derive(Clone)]
pub(crate) struct PgStatUserTablesCacheEntry {
    pub info: PgStatUserTablesInfo,
    pub schemaname: String,
    pub relname: String,
}

impl PostgresCollector {
    /// Collects pg_stat_user_tables statistics.
    ///
    /// Uses 30-second caching (same interval as pg_stat_statements).
    /// Returns cached data with re-interned strings if cache is fresh.
    pub fn collect_tables(
        &mut self,
        interner: &mut StringInterner,
    ) -> Result<Vec<PgStatUserTablesInfo>, PgCollectError> {
        // Return cached data if fresh (re-intern strings for current interner state)
        if let Some(cache_time) = self.tables_cache_time
            && self.statements_collect_interval > std::time::Duration::ZERO
            && cache_time.elapsed() < self.statements_collect_interval
            && !self.tables_cache.is_empty()
        {
            return Ok(self
                .tables_cache
                .iter()
                .map(|entry| {
                    let mut info = entry.info.clone();
                    info.schemaname_hash = interner.intern(&entry.schemaname);
                    info.relname_hash = interner.intern(&entry.relname);
                    info
                })
                .collect());
        }

        self.ensure_connected()?;
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| PgCollectError::ConnectionError("not connected".to_string()))?;

        let query = build_stat_user_tables_query();
        let rows = client
            .query(query, &[])
            .map_err(|e| PgCollectError::QueryError(format_postgres_error(&e)))?;

        let collected_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut results = Vec::with_capacity(rows.len());
        let mut cache = Vec::with_capacity(rows.len());

        for row in &rows {
            let Some(info) = parse_table_row(row, interner, collected_at) else {
                continue; // skip rows that fail to deserialize
            };

            cache.push(PgStatUserTablesCacheEntry {
                info: info.0.clone(),
                schemaname: info.1,
                relname: info.2,
            });
            results.push(info.0);
        }

        // Merge pg_statio_user_tables I/O counters by relid
        let statio_query = build_statio_user_tables_query();
        let statio_rows = client.query(statio_query, &[]).unwrap_or_default();
        let statio_map: std::collections::HashMap<u32, StatioRow> = statio_rows
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

        self.tables_cache = cache;
        self.tables_cache_time = Some(Instant::now());

        Ok(results)
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
) -> Option<(PgStatUserTablesInfo, String, String)> {
    let relid: u32 = row.try_get::<_, i64>(0).ok()? as u32;
    let schemaname: String = row.try_get(1).unwrap_or_default();
    let relname: String = row.try_get(2).unwrap_or_default();

    let info = PgStatUserTablesInfo {
        relid,
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
