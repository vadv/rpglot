//! Collector for pg_stat_user_indexes (per-database view).

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatUserIndexesInfo;
use std::time::Instant;

use super::PgCollectError;
use super::PostgresCollector;
use super::format_postgres_error;
use super::queries::build_stat_user_indexes_query;

/// Cache entry for pg_stat_user_indexes rows.
/// Stores original strings for re-interning on each call.
#[derive(Clone)]
pub(crate) struct PgStatUserIndexesCacheEntry {
    pub info: PgStatUserIndexesInfo,
    pub schemaname: String,
    pub relname: String,
    pub indexrelname: String,
}

impl PostgresCollector {
    /// Collects pg_stat_user_indexes statistics.
    ///
    /// Uses 30-second caching (same interval as pg_stat_statements).
    /// Returns cached data with re-interned strings if cache is fresh.
    pub fn collect_indexes(
        &mut self,
        interner: &mut StringInterner,
    ) -> Result<Vec<PgStatUserIndexesInfo>, PgCollectError> {
        // Return cached data if fresh (re-intern strings for current interner state)
        if let Some(cache_time) = self.indexes_cache_time {
            if self.statements_collect_interval > std::time::Duration::ZERO
                && cache_time.elapsed() < self.statements_collect_interval
                && !self.indexes_cache.is_empty()
            {
                return Ok(self
                    .indexes_cache
                    .iter()
                    .map(|entry| {
                        let mut info = entry.info.clone();
                        info.schemaname_hash = interner.intern(&entry.schemaname);
                        info.relname_hash = interner.intern(&entry.relname);
                        info.indexrelname_hash = interner.intern(&entry.indexrelname);
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

        let query = build_stat_user_indexes_query();
        let rows = client
            .query(query, &[])
            .map_err(|e| PgCollectError::QueryError(format_postgres_error(&e)))?;

        let mut results = Vec::with_capacity(rows.len());
        let mut cache = Vec::with_capacity(rows.len());

        for row in &rows {
            let indexrelid: u32 = row.get::<_, i64>(0) as u32;
            let relid: u32 = row.get::<_, i64>(1) as u32;
            let schemaname: String = row.get(2);
            let relname: String = row.get(3);
            let indexrelname: String = row.get(4);

            let info = PgStatUserIndexesInfo {
                indexrelid,
                relid,
                schemaname_hash: interner.intern(&schemaname),
                relname_hash: interner.intern(&relname),
                indexrelname_hash: interner.intern(&indexrelname),
                idx_scan: row.get(5),
                idx_tup_read: row.get(6),
                idx_tup_fetch: row.get(7),
                size_bytes: row.get(8),
            };

            cache.push(PgStatUserIndexesCacheEntry {
                info: info.clone(),
                schemaname,
                relname,
                indexrelname,
            });
            results.push(info);
        }

        self.indexes_cache = cache;
        self.indexes_cache_time = Some(Instant::now());

        Ok(results)
    }
}
