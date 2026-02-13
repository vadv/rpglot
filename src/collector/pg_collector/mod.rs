//! PostgreSQL metrics collector.
//!
//! Collects metrics from PostgreSQL statistics views:
//! - `pg_stat_activity` — active sessions
//! - `pg_stat_statements` — query statistics (requires extension)
//! - `pg_stat_database` — per-database statistics
//! - `pg_stat_user_tables` — per-database table statistics
//! - `pg_stat_user_indexes` — per-database index statistics
//!
//! ## Target database selection
//!
//! `pg_stat_user_tables` and `pg_stat_user_indexes` are per-database views that only
//! show data for the currently connected database. To collect meaningful data,
//! the collector automatically detects the largest non-template database and connects to it.
//!
//! - If `PGDATABASE` is explicitly set, auto-detection is **disabled** — the collector
//!   always connects to the specified database.
//! - If `PGDATABASE` is not set (default = `$PGUSER`), auto-detection runs every 10 minutes,
//!   selecting the largest database by `pg_database_size()`. If the largest database changes,
//!   the collector reconnects automatically.
//!
//! A single connection is used for all views (both cluster-wide and per-database).

mod activity;
mod bgwriter;
mod database;
mod indexes;
mod locks;
mod queries;
mod statements;
mod tables;

use postgres::{Client, NoTls};
use std::time::{Duration, Instant};

use indexes::PgStatUserIndexesCacheEntry;
use statements::{PgStatStatementsCacheEntry, STATEMENTS_COLLECT_INTERVAL};
use tables::PgStatUserTablesCacheEntry;

/// Interval between target database auto-detection checks.
const TARGET_DATABASE_CHECK_INTERVAL: Duration = Duration::from_secs(10 * 60);

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
    pub(crate) client: Option<Client>,
    pub(crate) last_error: Option<String>,
    pub(crate) server_version_num: Option<i32>,
    pub(crate) statements_ext_version: Option<String>,
    pub(crate) statements_last_check: Option<Instant>,
    pub(crate) statements_cache: Vec<PgStatStatementsCacheEntry>,
    pub(crate) statements_cache_time: Option<Instant>,
    /// Interval for pg_stat_statements caching. Default: 30 seconds.
    /// Set to Duration::ZERO to disable caching (fetch fresh data every call).
    pub(crate) statements_collect_interval: Duration,
    pub(crate) tables_cache: Vec<PgStatUserTablesCacheEntry>,
    pub(crate) tables_cache_time: Option<Instant>,
    pub(crate) indexes_cache: Vec<PgStatUserIndexesCacheEntry>,
    pub(crate) indexes_cache_time: Option<Instant>,
    /// true if PGDATABASE was explicitly set by the user (disables auto-detection).
    explicit_database: bool,
    /// Current target database name (from auto-detection or explicit PGDATABASE).
    pub(crate) target_database: Option<String>,
    /// Last time we checked for the largest database.
    target_database_last_check: Option<Instant>,
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
        let explicit_database = std::env::var("PGDATABASE").is_ok();
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
            tables_cache: Vec::new(),
            tables_cache_time: None,
            indexes_cache: Vec::new(),
            indexes_cache_time: None,
            explicit_database,
            target_database: None,
            target_database_last_check: None,
        })
    }

    /// Creates a collector with explicit connection string.
    ///
    /// Auto-detection is disabled (treated as explicit database).
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
            tables_cache: Vec::new(),
            tables_cache_time: None,
            indexes_cache: Vec::new(),
            indexes_cache_time: None,
            explicit_database: true,
            target_database: None,
            target_database_last_check: None,
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

    /// Attempts to connect to PostgreSQL.
    ///
    /// Returns `Ok(())` if connection succeeds, or an error describing the failure.
    /// Useful for startup checks before launching the TUI.
    pub fn try_connect(&mut self) -> Result<(), PgCollectError> {
        self.ensure_connected()
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
    ///
    /// When auto-detection is enabled (`PGDATABASE` not set), periodically checks
    /// which database is the largest and reconnects if it changed.
    pub(crate) fn ensure_connected(&mut self) -> Result<(), PgCollectError> {
        // If already connected, check if we need to re-detect target database.
        if self.client.is_some() {
            if !self.explicit_database {
                let should_check = match self.target_database_last_check {
                    None => true,
                    Some(t) => t.elapsed() >= TARGET_DATABASE_CHECK_INTERVAL,
                };
                if should_check {
                    self.detect_target_database();
                    // detect_target_database() may have set client=None to trigger reconnect.
                    if self.client.is_none() {
                        return self.ensure_connected();
                    }
                }
            }
            return Ok(());
        }

        // Build connection string: use target_database if auto-detected.
        let conn_str = match &self.target_database {
            Some(db) => replace_dbname(&self.connection_string, db),
            None => self.connection_string.clone(),
        };

        match Client::connect(&conn_str, NoTls) {
            Ok(client) => {
                let mut client = client;

                // Clear per-connection caches on (re)connect.
                self.clear_caches();

                // Determine server version once per (re)connect.
                self.server_version_num = client
                    .query_one("SHOW server_version_num", &[])
                    .ok()
                    .and_then(|row| row.try_get::<_, String>(0).ok())
                    .and_then(|v| v.parse::<i32>().ok());

                self.client = Some(client);
                self.last_error = None;

                // On first connect with auto-detection, detect target database immediately.
                if !self.explicit_database && self.target_database_last_check.is_none() {
                    self.detect_target_database();
                    // detect_target_database() may have set client=None to trigger reconnect.
                    if self.client.is_none() {
                        return self.ensure_connected();
                    }
                }

                Ok(())
            }
            Err(e) => {
                let msg = format_postgres_error(&e);
                self.last_error = Some(msg.clone());
                self.server_version_num = None;
                self.clear_caches();
                Err(PgCollectError::ConnectionError(msg))
            }
        }
    }

    /// Clears all per-connection caches.
    fn clear_caches(&mut self) {
        self.statements_ext_version = None;
        self.statements_last_check = None;
        self.statements_cache.clear();
        self.statements_cache_time = None;
        self.tables_cache.clear();
        self.tables_cache_time = None;
        self.indexes_cache.clear();
        self.indexes_cache_time = None;
    }

    /// Detects the largest non-template database and reconnects if it changed.
    ///
    /// Queries `pg_database` for the largest database by size, excluding templates
    /// and 'postgres'. If the result differs from the current connection, disconnects
    /// and sets `target_database` so the next `ensure_connected()` call reconnects.
    fn detect_target_database(&mut self) {
        self.target_database_last_check = Some(Instant::now());

        let client = match &mut self.client {
            Some(c) => c,
            None => return,
        };

        let result = client.query_one(
            "SELECT datname FROM pg_database \
             WHERE NOT datistemplate AND datname NOT IN ('postgres') \
             ORDER BY pg_database_size(datname) DESC LIMIT 1",
            &[],
        );

        let detected_db = match result {
            Ok(row) => row.get::<_, String>(0),
            Err(_) => return, // query failed (no permissions etc.) — stay on current DB
        };

        // Get current database name to compare.
        let current_db = client
            .query_one("SELECT current_database()", &[])
            .ok()
            .map(|row| row.get::<_, String>(0));

        if current_db.as_deref() == Some(detected_db.as_str()) {
            // Already connected to the largest DB — update target and return.
            self.target_database = Some(detected_db);
            return;
        }

        // Need to reconnect to the detected database.
        self.target_database = Some(detected_db);
        self.client = None; // disconnect — next ensure_connected() will reconnect
    }
}

/// Replaces the `dbname=xxx` parameter in a libpq-style connection string.
///
/// If the connection string contains `dbname=...`, it is replaced with the new database name.
/// If it does not contain `dbname=`, the parameter is appended.
fn replace_dbname(connection_string: &str, new_db: &str) -> String {
    // libpq key=value format: tokens separated by spaces
    let mut found = false;
    let parts: Vec<String> = connection_string
        .split_whitespace()
        .map(|token| {
            if token.starts_with("dbname=") {
                found = true;
                format!("dbname={}", new_db)
            } else {
                token.to_string()
            }
        })
        .collect();

    if found {
        parts.join(" ")
    } else {
        format!("{} dbname={}", connection_string, new_db)
    }
}

/// Formats PostgreSQL error message for display.
pub(crate) fn format_postgres_error(e: &postgres::Error) -> String {
    if let Some(db_error) = e.as_db_error() {
        format!("{}: {}", db_error.severity(), db_error.message())
    } else {
        let msg = e.to_string();
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
    fn replace_dbname_replaces_existing() {
        let conn = "host=localhost port=5432 user=app dbname=postgres";
        assert_eq!(
            replace_dbname(conn, "mydb"),
            "host=localhost port=5432 user=app dbname=mydb"
        );
    }

    #[test]
    fn replace_dbname_appends_when_missing() {
        let conn = "host=localhost port=5432 user=app";
        assert_eq!(
            replace_dbname(conn, "mydb"),
            "host=localhost port=5432 user=app dbname=mydb"
        );
    }

    #[test]
    fn replace_dbname_handles_dbname_at_start() {
        let conn = "dbname=old host=localhost user=app";
        assert_eq!(
            replace_dbname(conn, "new"),
            "dbname=new host=localhost user=app"
        );
    }
}
