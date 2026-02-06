//! PostgreSQL metrics collector.
//!
//! Collects active session information from `pg_stat_activity` view.

use postgres::{Client, NoTls};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::storage::interner::StringInterner;
use crate::storage::model::{PgStatActivityInfo, PgStatStatementsInfo};

const STATEMENTS_EXT_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);
const STATEMENTS_COLLECT_INTERVAL: Duration = Duration::from_secs(30);

fn statements_collect_due(last_collect: Option<Instant>, now: Instant, interval: Duration) -> bool {
    if interval.is_zero() {
        return true; // No caching
    }
    match last_collect {
        Some(last) => now.duration_since(last) >= interval,
        None => true,
    }
}

fn statements_ext_check_due(last_check: Option<Instant>, now: Instant) -> bool {
    match last_check {
        Some(last) => now.duration_since(last) >= STATEMENTS_EXT_CHECK_INTERVAL,
        None => true,
    }
}

#[derive(Clone)]
struct PgStatStatementsCacheEntry {
    info: PgStatStatementsInfo,
    query_text: String,
    datname: String,
    usename: String,
}

/// Error type for PostgreSQL collection.
#[derive(Debug)]
pub enum PgCollectError {
    /// Environment variable not set.
    EnvNotSet(String),
    /// Connection failed.
    ConnectionError(String),
    /// Query execution failed.
    QueryError(String),
}

impl std::fmt::Display for PgCollectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgCollectError::EnvNotSet(var) => write!(f, "PostgreSQL: {} not set", var),
            PgCollectError::ConnectionError(msg) => write!(f, "PostgreSQL: {}", msg),
            PgCollectError::QueryError(msg) => write!(f, "PostgreSQL query error: {}", msg),
        }
    }
}

impl std::error::Error for PgCollectError {}

/// PostgreSQL metrics collector.
///
/// Connects to PostgreSQL using standard environment variables:
/// - PGHOST (default: localhost)
/// - PGPORT (default: 5432)
/// - PGUSER (default: $USER)
/// - PGPASSWORD (default: empty)
/// - PGDATABASE (default: same as PGUSER)
pub struct PostgresCollector {
    connection_string: String,
    client: Option<Client>,
    last_error: Option<String>,
    server_version_num: Option<i32>,
    statements_ext_version: Option<String>,
    statements_last_check: Option<Instant>,
    statements_cache: Vec<PgStatStatementsCacheEntry>,
    statements_cache_time: Option<Instant>,
    /// Interval for pg_stat_statements caching. Default: 30 seconds.
    /// Set to Duration::ZERO to disable caching (fetch fresh data every call).
    statements_collect_interval: Duration,
}

impl PostgresCollector {
    /// Creates a new PostgreSQL collector from environment variables.
    ///
    /// Uses $USER as default if PGUSER is not set.
    pub fn from_env() -> Result<Self, PgCollectError> {
        let user = std::env::var("PGUSER")
            .or_else(|_| std::env::var("USER"))
            .map_err(|_| PgCollectError::EnvNotSet("PGUSER or USER".to_string()))?;

        let host = std::env::var("PGHOST").unwrap_or_else(|_| "localhost".to_string());
        let port = std::env::var("PGPORT").unwrap_or_else(|_| "5432".to_string());
        let password = std::env::var("PGPASSWORD").unwrap_or_default();
        let database = std::env::var("PGDATABASE").unwrap_or_else(|_| user.clone());

        let connection_string = if password.is_empty() {
            format!(
                "host={} port={} user={} dbname={}",
                host, port, user, database
            )
        } else {
            format!(
                "host={} port={} user={} password={} dbname={}",
                host, port, user, password, database
            )
        };

        Ok(Self {
            connection_string,
            client: None,
            last_error: None,
            server_version_num: None,
            statements_ext_version: None,
            statements_last_check: None,
            statements_cache: Vec::new(),
            statements_cache_time: None,
            statements_collect_interval: STATEMENTS_COLLECT_INTERVAL,
        })
    }

    /// Creates a collector with explicit connection string.
    pub fn with_connection_string(connection_string: String) -> Self {
        Self {
            connection_string,
            client: None,
            last_error: None,
            server_version_num: None,
            statements_ext_version: None,
            statements_last_check: None,
            statements_cache: Vec::new(),
            statements_cache_time: None,
            statements_collect_interval: STATEMENTS_COLLECT_INTERVAL,
        }
    }

    /// Sets the interval for pg_stat_statements caching.
    ///
    /// Default: 30 seconds. Set to `Duration::ZERO` to disable caching
    /// and fetch fresh data on every call.
    ///
    /// # Example
    /// ```ignore
    /// // TUI live mode: no caching for real-time data
    /// let collector = PostgresCollector::from_env()?
    ///     .with_statements_interval(Duration::ZERO);
    ///
    /// // Daemon mode: cache for 30 seconds (default)
    /// let collector = PostgresCollector::from_env()?;
    /// ```
    pub fn with_statements_interval(mut self, interval: Duration) -> Self {
        self.statements_collect_interval = interval;
        self
    }

    /// Returns the last error message, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Returns the current statements caching interval.
    ///
    /// Returns `Duration::ZERO` if caching is disabled.
    pub fn statements_cache_interval(&self) -> Duration {
        self.statements_collect_interval
    }

    /// Ensures connection is established, reconnecting if needed.
    fn ensure_connected(&mut self) -> Result<(), PgCollectError> {
        if self.client.is_some() {
            return Ok(());
        }

        match Client::connect(&self.connection_string, NoTls) {
            Ok(client) => {
                let mut client = client;

                // Clear per-connection caches on (re)connect.
                self.statements_ext_version = None;
                self.statements_last_check = None;
                self.statements_cache.clear();
                self.statements_cache_time = None;

                // Determine server version once per (re)connect.
                // Non-fatal: if this fails, we keep collecting metrics with conservative queries.
                self.server_version_num = client
                    .query_one("SHOW server_version_num", &[])
                    .ok()
                    .and_then(|row| row.try_get::<_, String>(0).ok())
                    .and_then(|v| v.parse::<i32>().ok());

                self.client = Some(client);
                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                let msg = format_postgres_error(&e);
                self.last_error = Some(msg.clone());
                self.server_version_num = None;
                self.statements_ext_version = None;
                self.statements_last_check = None;
                self.statements_cache.clear();
                self.statements_cache_time = None;
                Err(PgCollectError::ConnectionError(msg))
            }
        }
    }

    fn statements_extension_available(&mut self) -> bool {
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
                // Non-fatal: treat as unavailable.
                let msg = format_postgres_error(&e);
                self.last_error = Some(msg);
                self.statements_ext_version = None;
                false
            }
        }
    }

    /// Collects pg_stat_activity data.
    ///
    /// Returns a vector of active sessions. On error, stores the error
    /// message in `last_error` and returns empty vector (collection
    /// continues for other metrics).
    pub fn collect(&mut self, interner: &mut StringInterner) -> Vec<PgStatActivityInfo> {
        // Try to connect
        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return Vec::new();
        }

        let client = self.client.as_mut().unwrap();

        let query = build_stat_activity_query(self.server_version_num);

        match client.query(&query, &[]) {
            Ok(rows) => {
                self.last_error = None;
                rows.iter()
                    .map(|row| {
                        let datname: String = row.get("datname");
                        let usename: String = row.get("usename");
                        let application_name: String = row.get("application_name");
                        let state: String = row.get("state");
                        let query_text: String = row.get("query");
                        let wait_event_type: String = row.get("wait_event_type");
                        let wait_event: String = row.get("wait_event");
                        let backend_type: String = row.get("backend_type");

                        PgStatActivityInfo {
                            pid: row.get("pid"),
                            datname_hash: interner.intern(&datname),
                            usename_hash: interner.intern(&usename),
                            application_name_hash: interner.intern(&application_name),
                            client_addr: row.get("client_addr"),
                            state_hash: interner.intern(&state),
                            query_hash: interner.intern(&query_text),
                            query_id: row.get("query_id"),
                            wait_event_type_hash: interner.intern(&wait_event_type),
                            wait_event_hash: interner.intern(&wait_event),
                            backend_type_hash: interner.intern(&backend_type),
                            backend_start: row.get("backend_start"),
                            xact_start: row.get("xact_start"),
                            query_start: row.get("query_start"),
                        }
                    })
                    .collect()
            }
            Err(e) => {
                let msg = format_postgres_error(&e);
                self.last_error = Some(msg);
                // Connection might be broken, clear it for reconnection
                self.client = None;
                self.server_version_num = None;
                self.statements_ext_version = None;
                self.statements_last_check = None;
                Vec::new()
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

                self.statements_cache = entries;
                out
            }
            Err(e) => {
                let msg = format_postgres_error(&e);
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

fn build_stat_activity_query(server_version_num: Option<i32>) -> String {
    let query_id_expr = if server_version_num.unwrap_or(0) >= 140000 {
        "COALESCE(query_id, 0)::bigint as query_id"
    } else {
        "0::bigint as query_id"
    };

    format!(
        r#"
            SELECT
                pid,
                COALESCE(datname, '') as datname,
                COALESCE(usename, '') as usename,
                COALESCE(application_name, '') as application_name,
                COALESCE(client_addr::text, '') as client_addr,
                COALESCE(state, '') as state,
                COALESCE(query, '') as query,
                {query_id_expr},
                COALESCE(wait_event_type, '') as wait_event_type,
                COALESCE(wait_event, '') as wait_event,
                COALESCE(backend_type, '') as backend_type,
                COALESCE(EXTRACT(EPOCH FROM backend_start)::bigint, 0) as backend_start,
                COALESCE(EXTRACT(EPOCH FROM xact_start)::bigint, 0) as xact_start,
                COALESCE(EXTRACT(EPOCH FROM query_start)::bigint, 0) as query_start
            FROM pg_stat_activity
        "#
    )
}

fn build_stat_statements_query(server_version_num: Option<i32>) -> String {
    let v = server_version_num.unwrap_or(0);
    let (
        total_exec_time_expr,
        mean_exec_time_expr,
        min_exec_time_expr,
        max_exec_time_expr,
        stddev_exec_time_expr,
        total_plan_time_expr,
        wal_records_expr,
        wal_bytes_expr,
    ) = if v >= 130000 {
        (
            "s.total_exec_time",
            "s.mean_exec_time",
            "s.min_exec_time",
            "s.max_exec_time",
            "s.stddev_exec_time",
            "s.total_plan_time",
            "s.wal_records",
            "s.wal_bytes",
        )
    } else {
        (
            "s.total_time",
            "s.mean_time",
            "s.min_time",
            "s.max_time",
            "s.stddev_time",
            "0",
            "0",
            "0",
        )
    };

    format!(
        r#"
            SELECT
                s.userid,
                s.dbid,
                s.queryid,
                COALESCE(d.datname, '') as datname,
                COALESCE(r.rolname, '') as usename,
                COALESCE(s.query, '') as query,
                s.calls,
                {total_exec_time_expr}::double precision as total_exec_time,
                {mean_exec_time_expr}::double precision as mean_exec_time,
                {min_exec_time_expr}::double precision as min_exec_time,
                {max_exec_time_expr}::double precision as max_exec_time,
                {stddev_exec_time_expr}::double precision as stddev_exec_time,
                s.rows,
                s.shared_blks_read,
                s.shared_blks_hit,
                s.shared_blks_written,
                s.shared_blks_dirtied,
                s.local_blks_read,
                s.local_blks_written,
                s.temp_blks_read,
                s.temp_blks_written,
                {wal_records_expr}::bigint as wal_records,
                {wal_bytes_expr}::bigint as wal_bytes,
                {total_plan_time_expr}::double precision as total_plan_time
            FROM pg_stat_statements s
            LEFT JOIN pg_database d ON d.oid = s.dbid
            LEFT JOIN pg_roles r ON r.oid = s.userid
            ORDER BY total_exec_time DESC
            LIMIT 500
        "#
    )
}

/// Formats PostgreSQL error message for display.
fn format_postgres_error(e: &postgres::Error) -> String {
    // Extract the most useful part of the error
    if let Some(db_error) = e.as_db_error() {
        format!("{}: {}", db_error.severity(), db_error.message())
    } else {
        // Connection errors, etc.
        let msg = e.to_string();
        // Simplify common errors
        if msg.contains("Connection refused") {
            "connection refused".to_string()
        } else if msg.contains("password authentication failed") {
            "password authentication failed".to_string()
        } else if msg.contains("does not exist") {
            msg.split("FATAL:")
                .last()
                .unwrap_or(&msg)
                .trim()
                .to_string()
        } else {
            msg
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // With zero interval, should always return true (no caching)
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

    #[test]
    fn stat_activity_query_includes_query_id_on_pg14_plus() {
        let q = build_stat_activity_query(Some(140000));
        assert!(q.contains("COALESCE(query_id, 0)::bigint as query_id"));
    }

    #[test]
    fn stat_activity_query_uses_zero_query_id_on_pg13_and_older() {
        let q = build_stat_activity_query(Some(130000));
        assert!(q.contains("0::bigint as query_id"));
        assert!(!q.contains("COALESCE(query_id"));
    }

    #[test]
    fn stat_statements_query_uses_exec_time_columns_on_pg13_plus() {
        let q = build_stat_statements_query(Some(130000));
        assert!(q.contains("s.total_exec_time::double precision as total_exec_time"));
        assert!(q.contains("s.mean_exec_time::double precision as mean_exec_time"));
        assert!(q.contains("s.total_plan_time::double precision as total_plan_time"));
        assert!(q.contains("s.wal_records::bigint as wal_records"));
        assert!(q.contains("s.wal_bytes::bigint as wal_bytes"));
        assert!(q.contains("LEFT JOIN pg_database"));
        assert!(q.contains("LEFT JOIN pg_roles"));
        assert!(q.contains("as datname"));
        assert!(q.contains("as usename"));
    }

    #[test]
    fn stat_statements_query_uses_legacy_time_columns_on_pg12_and_older() {
        let q = build_stat_statements_query(Some(120000));
        assert!(q.contains("s.total_time::double precision as total_exec_time"));
        assert!(q.contains("s.mean_time::double precision as mean_exec_time"));
        assert!(q.contains("0::double precision as total_plan_time"));
        assert!(q.contains("0::bigint as wal_records"));
        assert!(q.contains("0::bigint as wal_bytes"));
        assert!(q.contains("LEFT JOIN pg_database"));
        assert!(q.contains("LEFT JOIN pg_roles"));
        assert!(q.contains("as datname"));
        assert!(q.contains("as usename"));
    }
}
