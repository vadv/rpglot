//! PostgreSQL metrics collected from system views.
//!
//! These structures store PostgreSQL server activity and statistics,
//! enabling monitoring of database performance and query patterns.

use serde::{Deserialize, Serialize};

/// Active session information from pg_stat_activity.
///
/// Source: `SELECT * FROM pg_stat_activity`
///
/// This view shows information about the current activity of server processes.
/// Each row represents a PostgreSQL backend process.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PgStatActivityInfo {
    /// Process ID of the backend.
    /// Source: `pg_stat_activity.pid`
    pub pid: i32,

    /// Hash of database name.
    /// Source: `pg_stat_activity.datname` - interned via StringInterner
    pub datname_hash: u64,

    /// Hash of user name.
    /// Source: `pg_stat_activity.usename` - interned via StringInterner
    pub usename_hash: u64,

    /// Hash of application name.
    /// Source: `pg_stat_activity.application_name` - interned via StringInterner
    pub application_name_hash: u64,

    /// IP address of the client connected to this backend.
    /// Source: `pg_stat_activity.client_addr`
    /// Note: Stored as String since IP addresses vary in length
    pub client_addr: String,

    /// Hash of current state (active, idle, idle in transaction, etc.).
    /// Source: `pg_stat_activity.state` - interned via StringInterner
    pub state_hash: u64,

    /// Hash of the currently executing query text.
    /// Source: `pg_stat_activity.query` - interned via StringInterner
    /// Note: Truncated to track_activity_query_size
    pub query_hash: u64,

    /// Query identifier computed by PostgreSQL.
    /// Source: `pg_stat_activity.query_id` (PostgreSQL 14+)
    /// Note: For PostgreSQL versions that don't expose this column, stored as 0.
    #[serde(default)]
    pub query_id: i64,

    /// Hash of wait event type (Lock, LWLock, BufferPin, etc.).
    /// Source: `pg_stat_activity.wait_event_type` - interned via StringInterner
    pub wait_event_type_hash: u64,

    /// Hash of specific wait event name.
    /// Source: `pg_stat_activity.wait_event` - interned via StringInterner
    pub wait_event_hash: u64,

    /// Hash of backend type (client backend, autovacuum worker, etc.).
    /// Source: `pg_stat_activity.backend_type` - interned via StringInterner
    pub backend_type_hash: u64,

    /// Backend start time (seconds since Unix epoch).
    /// Source: `pg_stat_activity.backend_start`
    /// When the backend process was started.
    pub backend_start: i64,

    /// Transaction start time (seconds since Unix epoch).
    /// Source: `pg_stat_activity.xact_start`
    /// When the current transaction was started (0 if no active transaction).
    pub xact_start: i64,

    /// Current query start time (seconds since Unix epoch).
    /// Source: `pg_stat_activity.query_start`
    /// When the currently active query was started.
    pub query_start: i64,
}

/// Query statistics from pg_stat_statements extension.
///
/// Source: `SELECT * FROM pg_stat_statements`
///
/// This extension tracks execution statistics of all SQL statements
/// executed by the server. Must be enabled via shared_preload_libraries.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PgStatStatementsInfo {
    /// OID of the user who executed the statement.
    /// Source: `pg_stat_statements.userid`
    pub userid: u32,

    /// OID of the database in which the statement was executed.
    /// Source: `pg_stat_statements.dbid`
    pub dbid: u32,

    /// Internal hash code identifying the query.
    /// Source: `pg_stat_statements.queryid`
    /// Note: Used as unique key for delta computation
    pub queryid: i64,

    /// Hash of database name.
    /// Source: join `pg_database.oid = pg_stat_statements.dbid`.
    /// Interned via StringInterner.
    #[serde(default)]
    pub datname_hash: u64,

    /// Hash of user/role name.
    /// Source: join `pg_roles.oid = pg_stat_statements.userid`.
    /// Interned via StringInterner.
    #[serde(default)]
    pub usename_hash: u64,

    /// Hash of normalized query text.
    /// Source: `pg_stat_statements.query` - interned via StringInterner
    /// Note: Parameters are replaced with $1, $2, etc.
    pub query_hash: u64,

    /// Number of times the statement was executed.
    /// Source: `pg_stat_statements.calls`
    pub calls: i64,

    /// Total time spent executing the statement (milliseconds).
    /// Source: `pg_stat_statements.total_exec_time`
    pub total_exec_time: f64,

    /// Mean time spent executing the statement (milliseconds).
    /// Source: `pg_stat_statements.mean_exec_time` (PG 13+) or `mean_time` (older)
    #[serde(default)]
    pub mean_exec_time: f64,

    /// Minimum time spent executing the statement (milliseconds).
    /// Source: `pg_stat_statements.min_exec_time` (PG 13+) or `min_time` (older)
    #[serde(default)]
    pub min_exec_time: f64,

    /// Maximum time spent executing the statement (milliseconds).
    /// Source: `pg_stat_statements.max_exec_time` (PG 13+) or `max_time` (older)
    #[serde(default)]
    pub max_exec_time: f64,

    /// Standard deviation of execution time (milliseconds).
    /// Source: `pg_stat_statements.stddev_exec_time` (PG 13+) or `stddev_time` (older)
    #[serde(default)]
    pub stddev_exec_time: f64,

    /// Total number of rows retrieved or affected.
    /// Source: `pg_stat_statements.rows`
    pub rows: i64,

    /// Total number of shared blocks read from buffer cache.
    /// Source: `pg_stat_statements.shared_blks_read`
    pub shared_blks_read: i64,

    /// Total number of shared blocks hit in buffer cache.
    /// Source: `pg_stat_statements.shared_blks_hit`
    #[serde(default)]
    pub shared_blks_hit: i64,

    /// Total number of shared blocks written.
    /// Source: `pg_stat_statements.shared_blks_written`
    pub shared_blks_written: i64,

    /// Total number of shared blocks dirtied.
    /// Source: `pg_stat_statements.shared_blks_dirtied`
    #[serde(default)]
    pub shared_blks_dirtied: i64,

    /// Total number of local blocks read (temporary tables).
    /// Source: `pg_stat_statements.local_blks_read`
    pub local_blks_read: i64,

    /// Total number of local blocks written.
    /// Source: `pg_stat_statements.local_blks_written`
    pub local_blks_written: i64,

    /// Total number of temp blocks read.
    /// Source: `pg_stat_statements.temp_blks_read`
    #[serde(default)]
    pub temp_blks_read: i64,

    /// Total number of temp blocks written.
    /// Source: `pg_stat_statements.temp_blks_written`
    #[serde(default)]
    pub temp_blks_written: i64,

    /// Total number of WAL records generated by the statement.
    /// Source: `pg_stat_statements.wal_records` (PG 13+)
    #[serde(default)]
    pub wal_records: i64,

    /// Total amount of WAL generated by the statement (bytes).
    /// Source: `pg_stat_statements.wal_bytes` (PG 13+)
    #[serde(default)]
    pub wal_bytes: i64,

    /// Total time spent planning the statement (milliseconds).
    /// Source: `pg_stat_statements.total_plan_time` (PG 13+)
    #[serde(default)]
    pub total_plan_time: f64,

    /// Unix timestamp (seconds since epoch) when this data was collected from PostgreSQL.
    /// Used by TUI to calculate accurate rates when collector caches pg_stat_statements.
    /// Source: set by collector at collection time
    #[serde(default)]
    pub collected_at: i64,
}
