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
            let relid: u32 = row.get::<_, i64>(0) as u32;
            let schemaname: String = row.get(1);
            let relname: String = row.get(2);

            let info = PgStatUserTablesInfo {
                relid,
                schemaname_hash: interner.intern(&schemaname),
                relname_hash: interner.intern(&relname),
                seq_scan: row.get(3),
                seq_tup_read: row.get(4),
                idx_scan: row.get(5),
                idx_tup_fetch: row.get(6),
                n_tup_ins: row.get(7),
                n_tup_upd: row.get(8),
                n_tup_del: row.get(9),
                n_tup_hot_upd: row.get(10),
                n_live_tup: row.get(11),
                n_dead_tup: row.get(12),
                vacuum_count: row.get(13),
                autovacuum_count: row.get(14),
                analyze_count: row.get(15),
                autoanalyze_count: row.get(16),
                last_vacuum: row.get(17),
                last_autovacuum: row.get(18),
                last_analyze: row.get(19),
                last_autoanalyze: row.get(20),
            };

            cache.push(PgStatUserTablesCacheEntry {
                info: info.clone(),
                schemaname,
                relname,
            });
            results.push(info);
        }

        self.tables_cache = cache;
        self.tables_cache_time = Some(Instant::now());

        Ok(results)
    }
}
