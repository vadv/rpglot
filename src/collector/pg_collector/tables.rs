//! Collector for pg_stat_user_tables (per-database view).

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatUserTablesInfo;
use std::time::Instant;

use super::PgCollectError;
use super::PostgresCollector;
use super::format_postgres_error;
use super::queries::build_stat_user_tables_query;

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
        if let Some(cache_time) = self.tables_cache_time {
            if self.statements_collect_interval > std::time::Duration::ZERO
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

        let mut results = Vec::with_capacity(rows.len());
        let mut cache = Vec::with_capacity(rows.len());

        for row in &rows {
            let Some(info) = parse_table_row(row, interner) else {
                continue; // skip rows that fail to deserialize
            };

            cache.push(PgStatUserTablesCacheEntry {
                info: info.0.clone(),
                schemaname: info.1,
                relname: info.2,
            });
            results.push(info.0);
        }

        self.tables_cache = cache;
        self.tables_cache_time = Some(Instant::now());

        Ok(results)
    }
}

/// Safely parses a single row from pg_stat_user_tables query.
/// Returns None if any column fails to deserialize (instead of panicking).
fn parse_table_row(
    row: &postgres::Row,
    interner: &mut StringInterner,
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
    };

    Some((info, schemaname, relname))
}
