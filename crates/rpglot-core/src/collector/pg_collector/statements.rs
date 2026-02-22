//! pg_stat_statements collection with caching and activity-only filtering.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing::debug;

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
        let now = Instant::now();
        if !statements_ext_check_due(self.statements_last_check, now) {
            return self.statements_ext_version.is_some();
        }

        self.statements_last_check = Some(now);

        let query = "SELECT extversion FROM pg_extension WHERE extname = 'pg_stat_statements'";

        // 1. Check main client first.
        if let Some(ref mut client) = self.client {
            match client.query_opt(query, &[]) {
                Ok(Some(row)) => {
                    let v: String = row.get(0);
                    self.statements_ext_version = Some(v);
                    self.statements_client_idx = None; // use main client
                    return true;
                }
                Ok(None) => {} // not found in main — continue searching db_clients
                Err(e) => {
                    let msg = super::format_postgres_error(&e);
                    self.last_error = Some(msg);
                }
            }
        }

        // 2. Search through per-database clients.
        for (idx, db_client) in self.db_clients.iter_mut().enumerate() {
            match db_client.client.query_opt(query, &[]) {
                Ok(Some(row)) => {
                    let v: String = row.get(0);
                    debug!(
                        database = %db_client.datname,
                        version = %v,
                        "pg_stat_statements found via per-database client"
                    );
                    self.statements_ext_version = Some(v);
                    self.statements_client_idx = Some(idx);
                    return true;
                }
                Ok(None) => {} // not in this DB, try next
                Err(_) => {}   // connection issue, skip
            }
        }

        // Not found anywhere.
        self.statements_ext_version = None;
        self.statements_client_idx = None;
        false
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
            // Cache is fresh — return previously filtered result (re-intern strings).
            return self.return_filtered_cached(interner);
        }

        // Mark the attempt time first to ensure we don't hit the server more often than
        // statements_collect_interval even on failures.
        self.statements_cache_time = Some(now);

        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return self.return_filtered_cached(interner);
        }

        if !self.statements_extension_available() {
            self.statements_cache.clear();
            return Vec::new();
        }

        // Validate statements_client_idx before use.
        let using_db_client = if let Some(idx) = self.statements_client_idx {
            if idx >= self.db_clients.len() {
                // Pool changed, idx is stale — reset and return cache.
                self.statements_client_idx = None;
                self.statements_ext_version = None;
                self.statements_last_check = None;
                return self.return_filtered_cached(interner);
            }
            true
        } else {
            false
        };

        let query = build_stat_statements_query(self.server_version_num);

        let result = if using_db_client {
            let idx = self.statements_client_idx.unwrap();
            self.db_clients[idx].client.query(&query, &[])
        } else {
            self.client.as_mut().unwrap().query(&query, &[])
        };

        match result {
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

                // Activity-only filtering: only keep rows where cumulative counters changed.
                let filtered = super::filter_active(&out, &self.pgs_prev, self.pgs_first_collect);

                // Update prev with the full (unfiltered) snapshot.
                self.pgs_prev = out
                    .iter()
                    .map(|info| (info.queryid, info.clone()))
                    .collect();
                self.pgs_first_collect = false;
                self.pgs_filtered_cache = filtered.clone();

                filtered
            }
            Err(e) => {
                let msg = super::format_postgres_error(&e);
                self.last_error = Some(msg);

                if using_db_client {
                    // Error on a per-database client — reset statements discovery
                    // so we re-search on next cycle. Don't touch main client.
                    self.statements_client_idx = None;
                    self.statements_ext_version = None;
                    self.statements_last_check = None;
                } else {
                    // Error on main client — assume connection is dead.
                    self.client = None;
                    self.server_version_num = None;
                    self.statements_ext_version = None;
                    self.statements_last_check = None;
                }

                self.return_filtered_cached(interner)
            }
        }
    }

    /// Returns the cached filtered result with re-interned strings.
    ///
    /// Uses `pgs_filtered_cache` (activity-only rows) and looks up original strings
    /// from the full `statements_cache` for re-interning.
    fn return_filtered_cached(&self, interner: &mut StringInterner) -> Vec<PgStatStatementsInfo> {
        // Build a lookup from queryid → cache entry for string re-interning.
        let cache_map: HashMap<i64, &PgStatStatementsCacheEntry> = self
            .statements_cache
            .iter()
            .map(|e| (e.info.queryid, e))
            .collect();

        self.pgs_filtered_cache
            .iter()
            .filter_map(|info| {
                let entry = cache_map.get(&info.queryid)?;
                let mut out = info.clone();
                out.query_hash = interner.intern(&entry.query_text);
                out.datname_hash = interner.intern(&entry.datname);
                out.usename_hash = interner.intern(&entry.usename);
                Some(out)
            })
            .collect()
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
        let info = PgStatStatementsInfo {
            queryid: 42,
            query_hash: 0,
            total_exec_time: 123.0,
            ..PgStatStatementsInfo::default()
        };
        collector.statements_cache = vec![PgStatStatementsCacheEntry {
            info: info.clone(),
            query_text: "SELECT 1".to_string(),
            datname: "testdb".to_string(),
            usename: "testuser".to_string(),
        }];
        // Populate filtered cache (simulates a previous fresh collect).
        collector.pgs_filtered_cache = vec![info];

        let mut interner = StringInterner::new();
        let rows = collector.collect_statements(&mut interner);

        assert_eq!(rows.len(), 1);
        assert_eq!(interner.resolve(rows[0].query_hash), Some("SELECT 1"));
        assert_eq!(rows[0].total_exec_time, 123.0);
        assert_eq!(interner.resolve(rows[0].datname_hash), Some("testdb"));
        assert_eq!(interner.resolve(rows[0].usename_hash), Some("testuser"));
    }

    #[test]
    fn filter_active_first_collect_returns_all() {
        use crate::collector::pg_collector::filter_active;

        let rows = vec![
            PgStatStatementsInfo {
                queryid: 1,
                calls: 10,
                ..Default::default()
            },
            PgStatStatementsInfo {
                queryid: 2,
                calls: 5,
                ..Default::default()
            },
        ];
        let prev = HashMap::new();
        let filtered = filter_active(&rows, &prev, true);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_active_removes_unchanged() {
        use crate::collector::pg_collector::filter_active;

        let unchanged = PgStatStatementsInfo {
            queryid: 1,
            calls: 10,
            rows: 100,
            total_exec_time: 50.0,
            ..Default::default()
        };
        let changed = PgStatStatementsInfo {
            queryid: 2,
            calls: 6,
            rows: 60,
            ..Default::default()
        };
        let new_row = PgStatStatementsInfo {
            queryid: 3,
            calls: 1,
            ..Default::default()
        };

        let rows = vec![unchanged.clone(), changed.clone(), new_row.clone()];

        let mut prev = HashMap::new();
        prev.insert(1, unchanged.clone()); // same → should be filtered out
        prev.insert(
            2,
            PgStatStatementsInfo {
                queryid: 2,
                calls: 5, // different → should be kept
                rows: 50,
                ..Default::default()
            },
        );
        // queryid 3 not in prev → new → should be kept

        let filtered = filter_active(&rows, &prev, false);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].queryid, 2);
        assert_eq!(filtered[1].queryid, 3);
    }

    #[test]
    fn activity_changed_detects_all_cumulative_fields() {
        use crate::storage::model::ActivityFiltered;

        let base = PgStatStatementsInfo::default();

        // No change
        assert!(!base.activity_changed(&base));

        // Each cumulative field individually
        let mut m = base.clone();
        m.calls = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.rows = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.total_exec_time = 1.0;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.total_plan_time = 1.0;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.shared_blks_read = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.shared_blks_hit = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.shared_blks_written = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.shared_blks_dirtied = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.local_blks_read = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.local_blks_written = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.temp_blks_read = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.temp_blks_written = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.wal_records = 1;
        assert!(m.activity_changed(&base));

        let mut m = base.clone();
        m.wal_bytes = 1;
        assert!(m.activity_changed(&base));
    }
}
