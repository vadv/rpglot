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
mod progress_vacuum;
mod queries;
mod replication;
mod settings;
mod statements;
mod store_plans;
mod tables;

use postgres::{Client, NoTls};
use std::collections::{HashMap, HashSet};
use std::env;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use super::log_collector::LogCollector;
use crate::storage::model::{
    ActivityFiltered, PgSettingEntry, PgStatStatementsInfo, PgStatUserIndexesInfo,
    PgStatUserTablesInfo, PgStorePlansInfo, ReplicationStatus,
};
use indexes::PgStatUserIndexesCacheEntry;
use queries::StorePlansFork;
use statements::{PgStatStatementsCacheEntry, STATEMENTS_COLLECT_INTERVAL};
use store_plans::PgStorePlansCacheEntry;
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

/// Filters rows to only include those where cumulative counters changed.
///
/// On first collect (`first_collect == true`), returns all rows (full snapshot baseline).
/// New rows (not present in `prev`) are always included.
pub(crate) fn filter_active<T: ActivityFiltered>(
    rows: &[T],
    prev: &HashMap<T::Key, T>,
    first_collect: bool,
) -> Vec<T> {
    if first_collect {
        return rows.to_vec();
    }
    rows.iter()
        .filter(|row| match prev.get(&row.activity_key()) {
            Some(prev_row) => row.activity_changed(prev_row),
            None => true,
        })
        .cloned()
        .collect()
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
    /// Name of the largest non-template database (heuristic instance identifier).
    pub(crate) largest_dbname: Option<String>,
    pub(crate) statements_ext_version: Option<String>,
    pub(crate) statements_last_check: Option<Instant>,
    /// Index into `db_clients` for the connection where pg_stat_statements is available.
    /// `None` means use the main client (extension found in main DB or not yet searched).
    pub(crate) statements_client_idx: Option<usize>,
    pub(crate) statements_cache: Vec<PgStatStatementsCacheEntry>,
    pub(crate) statements_cache_time: Option<Instant>,
    /// Interval for pg_stat_statements caching. Default: 30 seconds.
    /// Set to Duration::ZERO to disable caching (fetch fresh data every call).
    pub(crate) statements_collect_interval: Duration,
    pub(crate) tables_cache: Vec<PgStatUserTablesCacheEntry>,
    pub(crate) tables_cache_time: Option<Instant>,
    pub(crate) indexes_cache: Vec<PgStatUserIndexesCacheEntry>,
    pub(crate) indexes_cache_time: Option<Instant>,
    pub(crate) settings_cache: Vec<PgSettingEntry>,
    pub(crate) settings_cache_time: Option<Instant>,
    // --- Activity-only storage: prev snapshots for filtering unchanged rows ---
    /// Previous full pg_stat_statements snapshot (by queryid), used to filter unchanged rows.
    pub(crate) pgs_prev: HashMap<i64, PgStatStatementsInfo>,
    /// True until first successful pg_stat_statements collection (write all rows on first collect).
    pub(crate) pgs_first_collect: bool,
    /// Cached filtered result for pg_stat_statements (returned when collector cache is fresh).
    pub(crate) pgs_filtered_cache: Vec<PgStatStatementsInfo>,

    // --- pg_store_plans ---
    pub(crate) store_plans_ext_version: Option<String>,
    pub(crate) store_plans_last_check: Option<Instant>,
    pub(crate) store_plans_fork: Option<StorePlansFork>,
    pub(crate) store_plans_client_idx: Option<usize>,
    pub(crate) store_plans_cache: Vec<PgStorePlansCacheEntry>,
    pub(crate) store_plans_cache_time: Option<Instant>,
    pub(crate) pgp_filtered_cache: Vec<PgStorePlansInfo>,

    /// Previous full pg_stat_user_tables snapshot (by relid).
    pub(crate) pgt_prev: HashMap<u32, PgStatUserTablesInfo>,
    /// True until first successful tables collection.
    pub(crate) pgt_first_collect: bool,
    /// Cached filtered result for pg_stat_user_tables.
    pub(crate) pgt_filtered_cache: Vec<PgStatUserTablesInfo>,

    /// Previous full pg_stat_user_indexes snapshot (by indexrelid).
    pub(crate) pgi_prev: HashMap<u32, PgStatUserIndexesInfo>,
    /// True until first successful indexes collection.
    pub(crate) pgi_first_collect: bool,
    /// Cached filtered result for pg_stat_user_indexes.
    pub(crate) pgi_filtered_cache: Vec<PgStatUserIndexesInfo>,

    /// true if PGDATABASE was explicitly set by the user (disables multi-db collection).
    explicit_database: bool,
    /// Per-database connections for tables/indexes collection.
    pub(crate) db_clients: Vec<DatabaseClient>,
    /// Last time we refreshed the database connection pool.
    db_clients_last_check: Option<Instant>,
    /// Cached replication status.
    pub(crate) replication_cache: Option<ReplicationStatus>,
    /// Last time replication status was collected.
    pub(crate) replication_cache_time: Option<Instant>,
    /// PostgreSQL log file collector.
    log_collector: LogCollector,
}

impl PostgresCollector {
    fn new_inner(connection_string: String, explicit_database: bool) -> Self {
        Self {
            connection_string,
            client: None,
            last_error: None,
            server_version_num: None,
            largest_dbname: None,
            statements_ext_version: None,
            statements_last_check: None,
            statements_client_idx: None,
            statements_cache: Vec::new(),
            statements_cache_time: None,
            statements_collect_interval: STATEMENTS_COLLECT_INTERVAL,
            tables_cache: Vec::new(),
            tables_cache_time: None,
            indexes_cache: Vec::new(),
            indexes_cache_time: None,
            settings_cache: Vec::new(),
            settings_cache_time: None,
            pgs_prev: HashMap::new(),
            pgs_first_collect: true,
            pgs_filtered_cache: Vec::new(),
            store_plans_ext_version: None,
            store_plans_last_check: None,
            store_plans_fork: None,
            store_plans_client_idx: None,
            store_plans_cache: Vec::new(),
            store_plans_cache_time: None,
            pgp_filtered_cache: Vec::new(),
            pgt_prev: HashMap::new(),
            pgt_first_collect: true,
            pgt_filtered_cache: Vec::new(),
            pgi_prev: HashMap::new(),
            pgi_first_collect: true,
            pgi_filtered_cache: Vec::new(),
            explicit_database,
            db_clients: Vec::new(),
            db_clients_last_check: None,
            replication_cache: None,
            replication_cache_time: None,
            log_collector: LogCollector::new(),
        }
    }

    /// Creates a new PostgreSQL collector from environment variables.
    ///
    /// Uses $USER as default if PGUSER is not set.
    pub fn from_env() -> Result<Self, PgCollectError> {
        let user = env::var("PGUSER")
            .or_else(|_| env::var("USER"))
            .map_err(|_| PgCollectError::EnvNotSet("PGUSER or USER".to_string()))?;

        let host = env::var("PGHOST").unwrap_or_else(|_| "localhost".to_string());
        let port = env::var("PGPORT").unwrap_or_else(|_| "5432".to_string());
        let password = env::var("PGPASSWORD").unwrap_or_default();
        let explicit_database = env::var("PGDATABASE").is_ok();
        let database = env::var("PGDATABASE").unwrap_or_else(|_| user.clone());

        let connection_string = if password.is_empty() {
            format!(
                "host={} port={} user={} dbname={} application_name=rpglot",
                host, port, user, database
            )
        } else {
            format!(
                "host={} port={} user={} password={} dbname={} application_name=rpglot",
                host, port, user, password, database
            )
        };

        Ok(Self::new_inner(connection_string, explicit_database))
    }

    /// Creates a collector with explicit connection string.
    ///
    /// Multi-database collection is disabled (treated as explicit database).
    pub fn with_connection_string(connection_string: String) -> Self {
        Self::new_inner(connection_string, true)
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

                // Determine largest database name (instance identifier heuristic).
                self.largest_dbname = client
                    .query_one(
                        "SELECT datname FROM pg_database \
                         WHERE NOT datistemplate \
                         ORDER BY pg_database_size(datname) DESC LIMIT 1",
                        &[],
                    )
                    .ok()
                    .and_then(|row| row.try_get::<_, String>(0).ok())
                    .or_else(|| {
                        client
                            .query_one("SELECT current_database()", &[])
                            .ok()
                            .and_then(|row| row.try_get::<_, String>(0).ok())
                    });

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
                self.largest_dbname = None;
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
        let existing: HashSet<String> = self.db_clients.iter().map(|c| c.datname.clone()).collect();

        let target_set: HashSet<&str> = databases.iter().map(|s| s.as_str()).collect();

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
            // Pool changed — reset statements discovery so it re-searches on next collect.
            if self.statements_client_idx.is_some() {
                self.statements_client_idx = None;
                self.statements_ext_version = None;
                self.statements_last_check = None;
            }
            if self.store_plans_client_idx.is_some() {
                self.store_plans_client_idx = None;
                self.store_plans_ext_version = None;
                self.store_plans_last_check = None;
                self.store_plans_fork = None;
            }

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
        self.statements_client_idx = None;
        self.statements_cache.clear();
        self.statements_cache_time = None;
        self.tables_cache.clear();
        self.tables_cache_time = None;
        self.indexes_cache.clear();
        self.indexes_cache_time = None;
        self.settings_cache.clear();
        self.settings_cache_time = None;
        self.store_plans_ext_version = None;
        self.store_plans_last_check = None;
        self.store_plans_fork = None;
        self.store_plans_client_idx = None;
        self.store_plans_cache.clear();
        self.store_plans_cache_time = None;
        // Reset activity-only filtering state — next collect will write all rows.
        self.pgs_prev.clear();
        self.pgs_first_collect = true;
        self.pgs_filtered_cache.clear();
        self.pgp_filtered_cache.clear();
        self.pgt_prev.clear();
        self.pgt_first_collect = true;
        self.pgt_filtered_cache.clear();
        self.pgi_prev.clear();
        self.pgi_first_collect = true;
        self.pgi_filtered_cache.clear();
        self.replication_cache = None;
        self.replication_cache_time = None;
    }

    /// Returns PostgreSQL version as human-readable string (e.g. "16.2").
    pub fn pg_version_string(&self) -> Option<String> {
        let v = self.server_version_num?;
        Some(format!("{}.{}", v / 10000, (v / 100) % 100))
    }

    /// Returns instance metadata (database name + PG version).
    pub fn instance_info(&self) -> Option<(String, String)> {
        Some((self.largest_dbname.clone()?, self.pg_version_string()?))
    }

    /// Returns whether the PostgreSQL instance is in recovery mode (standby).
    ///
    /// Value is taken from the cached replication status (updated every 30s).
    pub fn is_in_recovery(&self) -> Option<bool> {
        self.replication_cache.as_ref().map(|r| r.is_in_recovery)
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
