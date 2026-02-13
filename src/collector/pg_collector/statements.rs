//! pg_stat_statements collection with caching.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatStatementsInfo;

use super::PostgresCollector;
use super::queries::build_stat_statements_query;

pub(super) const STATEMENTS_EXT_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub(super) const STATEMENTS_COLLECT_INTERVAL: Duration = Duration::from_secs(30);
/// Maximum number of statements to cache. Limits memory usage when there are many unique queries.
pub(super) const MAX_CACHED_STATEMENTS: usize = 1000;

pub(super) fn statements_collect_due(
    last_collect: Option<Instant>,
    now: Instant,
    interval: Duration,
) -> bool {
    if interval.is_zero() {
        return true; // No caching
    }
    match last_collect {
        Some(last) => now.duration_since(last) >= interval,
        None => true,
    }
}

pub(super) fn statements_ext_check_due(last_check: Option<Instant>, now: Instant) -> bool {
    match last_check {
        Some(last) => now.duration_since(last) >= STATEMENTS_EXT_CHECK_INTERVAL,
        None => true,
    }
}

#[derive(Clone)]
pub(crate) struct PgStatStatementsCacheEntry {
    pub info: PgStatStatementsInfo,
    pub query_text: String,
    pub datname: String,
    pub usename: String,
}

impl PostgresCollector {
    pub(super) fn statements_extension_available(&mut self) -> bool {
        let Some(client) = self.client.as_mut() else {
            return false;
        };

        let now = Instant::now();
        if !statements_ext_check_due(self.statements_last_check, now) {
            return self.statements_ext_version.is_some();
        }

        self.statements_last_check = Some(now);

        let query = "SELECT extversion FROM pg_extension WHERE extname = 'pg_stat_statements'";
        match client.query_opt(query, &[]) {
            Ok(Some(row)) => {
                let v: String = row.get(0);
                self.statements_ext_version = Some(v);
                true
            }
            Ok(None) => {
                self.statements_ext_version = None;
                false
            }
            Err(e) => {
                let msg = super::format_postgres_error(&e);
                self.last_error = Some(msg);
                self.statements_ext_version = None;
                false
            }
        }
    }

    /// Collects pg_stat_statements data.
    ///
    /// Requires `pg_stat_statements` extension to be installed. Extension presence is checked
    /// at most once per 5 minutes.
    pub fn collect_statements(
        &mut self,
        interner: &mut StringInterner,
    ) -> Vec<PgStatStatementsInfo> {
        let now = Instant::now();
        if !statements_collect_due(
            self.statements_cache_time,
            now,
            self.statements_collect_interval,
        ) {
            return self
                .statements_cache
                .iter()
                .map(|e| {
                    let mut info = e.info.clone();
                    info.query_hash = interner.intern(&e.query_text);
                    info.datname_hash = interner.intern(&e.datname);
                    info.usename_hash = interner.intern(&e.usename);
                    info
                })
                .collect();
        }

        // Mark the attempt time first to ensure we don't hit the server more often than
        // statements_collect_interval even on failures.
        self.statements_cache_time = Some(now);

        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return self
                .statements_cache
                .iter()
                .map(|e| {
                    let mut info = e.info.clone();
                    info.query_hash = interner.intern(&e.query_text);
                    info.datname_hash = interner.intern(&e.datname);
                    info.usename_hash = interner.intern(&e.usename);
                    info
                })
                .collect();
        }

        if !self.statements_extension_available() {
            self.statements_cache.clear();
            return Vec::new();
        }

        let client = self.client.as_mut().unwrap();
        let query = build_stat_statements_query(self.server_version_num);

        match client.query(&query, &[]) {
            Ok(rows) => {
                self.last_error = None;

                let mut entries = Vec::with_capacity(rows.len());
                let mut out = Vec::with_capacity(rows.len());
                for row in rows {
                    let query_text: String = row.get("query");
                    let datname: String = row.get("datname");
                    let usename: String = row.get("usename");
                    let collected_at = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let info = PgStatStatementsInfo {
                        userid: row.get("userid"),
                        dbid: row.get("dbid"),
                        queryid: row.get("queryid"),
                        datname_hash: interner.intern(&datname),
                        usename_hash: interner.intern(&usename),
                        query_hash: interner.intern(&query_text),
                        calls: row.get("calls"),
                        total_exec_time: row.get("total_exec_time"),
                        mean_exec_time: row.get("mean_exec_time"),
                        min_exec_time: row.get("min_exec_time"),
                        max_exec_time: row.get("max_exec_time"),
                        stddev_exec_time: row.get("stddev_exec_time"),
                        rows: row.get("rows"),
                        shared_blks_read: row.get("shared_blks_read"),
                        shared_blks_hit: row.get("shared_blks_hit"),
                        shared_blks_written: row.get("shared_blks_written"),
                        shared_blks_dirtied: row.get("shared_blks_dirtied"),
                        local_blks_read: row.get("local_blks_read"),
                        local_blks_written: row.get("local_blks_written"),
                        temp_blks_read: row.get("temp_blks_read"),
                        temp_blks_written: row.get("temp_blks_written"),
                        wal_records: row.get("wal_records"),
                        wal_bytes: row.get("wal_bytes"),
                        total_plan_time: row.get("total_plan_time"),
                        collected_at,
                    };
                    entries.push(PgStatStatementsCacheEntry {
                        info: info.clone(),
                        query_text,
                        datname,
                        usename,
                    });
                    out.push(info);
                }

                // Limit cache size to prevent unbounded memory growth
                if entries.len() > MAX_CACHED_STATEMENTS {
                    entries.truncate(MAX_CACHED_STATEMENTS);
                    out.truncate(MAX_CACHED_STATEMENTS);
                }

                self.statements_cache = entries;
                out
            }
            Err(e) => {
                let msg = super::format_postgres_error(&e);
                self.last_error = Some(msg);
                self.client = None;
                self.server_version_num = None;
                self.statements_ext_version = None;
                self.statements_last_check = None;
                self.statements_cache
                    .iter()
                    .map(|e| {
                        let mut info = e.info.clone();
                        info.query_hash = interner.intern(&e.query_text);
                        info.datname_hash = interner.intern(&e.datname);
                        info.usename_hash = interner.intern(&e.usename);
                        info
                    })
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::interner::StringInterner;

    #[test]
    fn statements_ext_check_due_respects_interval() {
        let now = Instant::now();
        assert!(statements_ext_check_due(None, now));

        let recent = now - Duration::from_secs(10);
        assert!(!statements_ext_check_due(Some(recent), now));

        let old = now - STATEMENTS_EXT_CHECK_INTERVAL;
        assert!(statements_ext_check_due(Some(old), now));
    }

    #[test]
    fn statements_collect_due_respects_interval() {
        let now = Instant::now();
        let interval = STATEMENTS_COLLECT_INTERVAL;

        assert!(statements_collect_due(None, now, interval));

        let recent = now - Duration::from_secs(10);
        assert!(!statements_collect_due(Some(recent), now, interval));

        let old = now - STATEMENTS_COLLECT_INTERVAL;
        assert!(statements_collect_due(Some(old), now, interval));
    }

    #[test]
    fn statements_collect_due_zero_interval_always_returns_true() {
        let now = Instant::now();
        let zero = Duration::ZERO;

        assert!(statements_collect_due(None, now, zero));
        assert!(statements_collect_due(Some(now), now, zero));
        assert!(statements_collect_due(
            Some(now - Duration::from_secs(1)),
            now,
            zero
        ));
    }

    #[test]
    fn collect_statements_returns_cached_without_connecting_when_fresh() {
        let mut collector = PostgresCollector::with_connection_string("host=invalid".to_string());
        collector.statements_cache_time = Some(Instant::now());
        collector.statements_cache = vec![PgStatStatementsCacheEntry {
            info: PgStatStatementsInfo {
                query_hash: 0,
                total_exec_time: 123.0,
                ..PgStatStatementsInfo::default()
            },
            query_text: "SELECT 1".to_string(),
            datname: "testdb".to_string(),
            usename: "testuser".to_string(),
        }];

        let mut interner = StringInterner::new();
        let rows = collector.collect_statements(&mut interner);

        assert_eq!(rows.len(), 1);
        assert_eq!(interner.resolve(rows[0].query_hash), Some("SELECT 1"));
        assert_eq!(rows[0].total_exec_time, 123.0);
        assert_eq!(interner.resolve(rows[0].datname_hash), Some("testdb"));
        assert_eq!(interner.resolve(rows[0].usename_hash), Some("testuser"));
    }
}
