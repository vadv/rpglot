//! PostgreSQL metrics collector.
//!
//! Collects metrics from PostgreSQL statistics views:
//! - `pg_stat_activity` — active sessions (instance-level)
//! - `pg_stat_statements` — query statistics (instance-level, requires extension)
//! - `pg_stat_database` — per-database statistics (instance-level)
//! - `pg_stat_user_tables` — per-database table statistics
//! - `pg_stat_user_indexes` — per-database index statistics
//!
//! ## Multi-database collection
//!
//! Instance-level metrics (activity, statements, database, bgwriter, locks, logs) are
//! collected via a single "main" connection to any database.
//!
//! Per-database metrics (tables, indexes) require a separate connection to each database.
//! The collector maintains a pool of `DatabaseClient` connections — one per accessible
//! non-template database. The pool is refreshed every 10 minutes to pick up newly
//! created databases and drop connections to removed ones.
//!
//! If `PGDATABASE` is explicitly set, multi-database collection is **disabled** — only
//! the specified database is used for both instance-level and per-database metrics.

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
use tracing::{debug, info, warn};

use super::log_collector::LogCollector;
use indexes::PgStatUserIndexesCacheEntry;
use statements::{PgStatStatementsCacheEntry, STATEMENTS_COLLECT_INTERVAL};
use tables::PgStatUserTablesCacheEntry;

/// Interval between database pool refresh checks.
const DB_POOL_REFRESH_INTERVAL: Duration = Duration::from_secs(10 * 60);

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

/// A connection to a specific database for per-database metric collection.
pub(crate) struct DatabaseClient {
    pub datname: String,
    pub client: Client,
}

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
    /// Main connection for instance-level metrics.
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
    /// true if PGDATABASE was explicitly set by the user (disables multi-db collection).
    explicit_database: bool,
    /// Per-database connections for tables/indexes collection.
    pub(crate) db_clients: Vec<DatabaseClient>,
    /// Last time we refreshed the database connection pool.
    db_clients_last_check: Option<Instant>,
    /// PostgreSQL log file collector.
    log_collector: LogCollector,
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
            db_clients: Vec::new(),
            db_clients_last_check: None,
            log_collector: LogCollector::new(),
        })
    }

    /// Creates a collector with explicit connection string.
    ///
    /// Multi-database collection is disabled (treated as explicit database).
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
            db_clients: Vec::new(),
            db_clients_last_check: None,
            log_collector: LogCollector::new(),
        }
    }

    /// Sets the interval for pg_stat_statements caching.
    ///
    /// Default: 30 seconds. Set to `Duration::ZERO` to disable caching
    /// and fetch fresh data on every call.
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

    /// Ensures the main connection is established, reconnecting if needed.
    pub(crate) fn ensure_connected(&mut self) -> Result<(), PgCollectError> {
        if self.client.is_some() {
            return Ok(());
        }

        match Client::connect(&self.connection_string, NoTls) {
            Ok(client) => {
                let mut client = client;

                // Clear per-connection caches on (re)connect.
                self.clear_caches();
                // Reset db_clients — will be rebuilt on next ensure_db_clients().
                self.db_clients.clear();
                self.db_clients_last_check = None;

                // Determine server version once per (re)connect.
                self.server_version_num = client
                    .query_one("SHOW server_version_num", &[])
                    .ok()
                    .and_then(|row| row.try_get::<_, String>(0).ok())
                    .and_then(|v| v.parse::<i32>().ok());

                // Initialize log collector with the new connection.
                self.log_collector.init(&mut client);

                self.client = Some(client);
                self.last_error = None;

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

    /// Ensures per-database connections are established for tables/indexes collection.
    ///
    /// In explicit database mode (PGDATABASE set), creates a single DatabaseClient
    /// for the configured database. In auto mode, queries pg_database for all
    /// accessible non-template databases and maintains connections to each.
    ///
    /// Refreshes the connection pool every 10 minutes.
    pub(crate) fn ensure_db_clients(&mut self) {
        // Check if refresh is needed.
        if let Some(last_check) = self.db_clients_last_check
            && last_check.elapsed() < DB_POOL_REFRESH_INTERVAL
            && !self.db_clients.is_empty()
        {
            return;
        }

        self.db_clients_last_check = Some(Instant::now());

        if self.explicit_database {
            // Single database mode — reuse the main connection's database.
            if self.db_clients.is_empty()
                && let Some(ref mut main_client) = self.client
            {
                // Get current database name from the main connection.
                if let Ok(row) = main_client.query_one("SELECT current_database()", &[]) {
                    let datname: String = row.get(0);
                    let conn_str = replace_dbname(&self.connection_string, &datname);
                    match Client::connect(&conn_str, NoTls) {
                        Ok(client) => {
                            self.db_clients.push(DatabaseClient {
                                datname: datname.clone(),
                                client,
                            });
                            debug!(database = %datname, "per-database connection established");
                        }
                        Err(e) => {
                            warn!(database = %datname, error = %format_postgres_error(&e),
                                "failed to connect for per-database metrics");
                        }
                    }
                }
            }
            return;
        }

        // Multi-database mode: discover all accessible databases.
        let Some(ref mut main_client) = self.client else {
            return;
        };

        let databases = match main_client.query(
            "SELECT datname FROM pg_database \
             WHERE NOT datistemplate AND datallowconn \
             ORDER BY datname",
            &[],
        ) {
            Ok(rows) => rows
                .iter()
                .map(|row| row.get::<_, String>(0))
                .collect::<Vec<_>>(),
            Err(e) => {
                warn!(error = %format_postgres_error(&e), "failed to list databases");
                return;
            }
        };

        // Build set of currently connected databases (owned to avoid borrow conflicts).
        let existing: std::collections::HashSet<String> =
            self.db_clients.iter().map(|c| c.datname.clone()).collect();

        let target_set: std::collections::HashSet<&str> =
            databases.iter().map(|s| s.as_str()).collect();

        // Remove connections to databases that no longer exist.
        let before = self.db_clients.len();
        self.db_clients
            .retain(|c| target_set.contains(c.datname.as_str()));
        let removed = before - self.db_clients.len();

        // Add connections to new databases.
        let mut added = 0;
        for db in &databases {
            if existing.contains(db.as_str()) {
                // Check if existing connection is still alive.
                let alive = self
                    .db_clients
                    .iter_mut()
                    .find(|c| c.datname == *db)
                    .map(|c| c.client.simple_query("").is_ok())
                    .unwrap_or(false);
                if !alive {
                    // Remove dead connection, will be re-added below.
                    self.db_clients.retain(|c| c.datname != *db);
                } else {
                    continue;
                }
            }

            let conn_str = replace_dbname(&self.connection_string, db);
            match Client::connect(&conn_str, NoTls) {
                Ok(client) => {
                    self.db_clients.push(DatabaseClient {
                        datname: db.clone(),
                        client,
                    });
                    added += 1;
                }
                Err(e) => {
                    warn!(database = %db, error = %format_postgres_error(&e),
                        "failed to connect for per-database metrics");
                }
            }
        }

        if added > 0 || removed > 0 {
            let names: Vec<&str> = self.db_clients.iter().map(|c| c.datname.as_str()).collect();
            info!(
                databases = ?names, added, removed,
                "per-database connection pool updated"
            );
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

    /// Collects log data from PostgreSQL log files.
    ///
    /// Returns grouped ERROR/FATAL/PANIC entries and operational event counts
    /// (checkpoints, autovacuum) for the current snapshot interval.
    pub fn collect_log_data(
        &mut self,
        interner: &mut crate::storage::interner::StringInterner,
    ) -> crate::collector::log_collector::LogCollectResult {
        let mut client = match self.client.take() {
            Some(c) => c,
            None => return crate::collector::log_collector::LogCollectResult::default(),
        };
        let result = self.log_collector.collect(&mut client, interner);
        self.client = Some(client);
        result
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
