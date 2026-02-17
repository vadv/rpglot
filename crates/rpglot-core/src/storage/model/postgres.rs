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

/// Database-level statistics from pg_stat_database.
///
/// Source: `SELECT * FROM pg_stat_database`
///
/// This view contains one row per database, showing cumulative statistics
/// about transactions, I/O, tuple operations, temp usage, and deadlocks.
/// All numeric fields are cumulative counters; rates are computed in the TUI
/// from deltas between consecutive snapshots.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PgStatDatabaseInfo {
    /// Database OID (diff key).
    /// Source: `pg_stat_database.datid`
    pub datid: u32,

    /// Hash of database name.
    /// Source: `pg_stat_database.datname` — interned via StringInterner
    pub datname_hash: u64,

    /// Committed transactions (cumulative).
    /// Source: `pg_stat_database.xact_commit`
    pub xact_commit: i64,

    /// Rolled back transactions (cumulative).
    /// Source: `pg_stat_database.xact_rollback`
    pub xact_rollback: i64,

    /// Disk blocks read (cumulative).
    /// Source: `pg_stat_database.blks_read`
    pub blks_read: i64,

    /// Buffer cache hits (cumulative).
    /// Source: `pg_stat_database.blks_hit`
    pub blks_hit: i64,

    /// Rows returned by sequential scans (cumulative).
    /// Source: `pg_stat_database.tup_returned`
    pub tup_returned: i64,

    /// Rows fetched by index scans (cumulative).
    /// Source: `pg_stat_database.tup_fetched`
    pub tup_fetched: i64,

    /// Rows inserted (cumulative).
    /// Source: `pg_stat_database.tup_inserted`
    pub tup_inserted: i64,

    /// Rows updated (cumulative).
    /// Source: `pg_stat_database.tup_updated`
    pub tup_updated: i64,

    /// Rows deleted (cumulative).
    /// Source: `pg_stat_database.tup_deleted`
    pub tup_deleted: i64,

    /// Queries canceled due to recovery conflicts (cumulative).
    /// Source: `pg_stat_database.conflicts`
    pub conflicts: i64,

    /// Temp files created (cumulative).
    /// Source: `pg_stat_database.temp_files`
    pub temp_files: i64,

    /// Temp bytes written (cumulative).
    /// Source: `pg_stat_database.temp_bytes`
    pub temp_bytes: i64,

    /// Deadlocks detected (cumulative).
    /// Source: `pg_stat_database.deadlocks`
    pub deadlocks: i64,

    /// Data checksum failures (PG 12+, cumulative).
    /// Source: `pg_stat_database.checksum_failures`
    #[serde(default)]
    pub checksum_failures: i64,

    /// Time spent reading blocks, milliseconds (requires track_io_timing).
    /// Source: `pg_stat_database.blk_read_time`
    pub blk_read_time: f64,

    /// Time spent writing blocks, milliseconds (requires track_io_timing).
    /// Source: `pg_stat_database.blk_write_time`
    pub blk_write_time: f64,

    /// Total session time, milliseconds (PG 14+).
    /// Source: `pg_stat_database.session_time`
    #[serde(default)]
    pub session_time: f64,

    /// Time spent in active state, milliseconds (PG 14+).
    /// Source: `pg_stat_database.active_time`
    #[serde(default)]
    pub active_time: f64,

    /// Time idle in transaction, milliseconds (PG 14+).
    /// Source: `pg_stat_database.idle_in_transaction_time`
    #[serde(default)]
    pub idle_in_transaction_time: f64,

    /// Total sessions (PG 14+, cumulative).
    /// Source: `pg_stat_database.sessions`
    #[serde(default)]
    pub sessions: i64,

    /// Abandoned sessions (PG 14+, cumulative).
    /// Source: `pg_stat_database.sessions_abandoned`
    #[serde(default)]
    pub sessions_abandoned: i64,

    /// Fatal sessions (PG 14+, cumulative).
    /// Source: `pg_stat_database.sessions_fatal`
    #[serde(default)]
    pub sessions_fatal: i64,

    /// Killed sessions (PG 14+, cumulative).
    /// Source: `pg_stat_database.sessions_killed`
    #[serde(default)]
    pub sessions_killed: i64,
}

/// Per-table statistics from pg_stat_user_tables.
///
/// Source: `SELECT * FROM pg_stat_user_tables`
///
/// This per-database view shows one row per user table, with cumulative
/// counters for scans, tuple operations, and maintenance activity.
/// Only tables in the currently connected database are visible.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PgStatUserTablesInfo {
    /// Table OID (diff key).
    /// Source: `pg_stat_user_tables.relid`
    pub relid: u32,

    /// Hash of database name.
    /// Source: set by collector from the connected database name — interned via StringInterner.
    #[serde(default)]
    pub datname_hash: u64,

    /// Hash of schema name.
    /// Source: `pg_stat_user_tables.schemaname` — interned via StringInterner
    pub schemaname_hash: u64,

    /// Hash of table name.
    /// Source: `pg_stat_user_tables.relname` — interned via StringInterner
    pub relname_hash: u64,

    /// Sequential scans initiated (cumulative).
    /// Source: `pg_stat_user_tables.seq_scan`
    pub seq_scan: i64,

    /// Rows returned by sequential scans (cumulative).
    /// Source: `pg_stat_user_tables.seq_tup_read`
    pub seq_tup_read: i64,

    /// Index scans initiated (cumulative).
    /// Source: `pg_stat_user_tables.idx_scan`
    pub idx_scan: i64,

    /// Rows fetched by index scans (cumulative).
    /// Source: `pg_stat_user_tables.idx_tup_fetch`
    pub idx_tup_fetch: i64,

    /// Rows inserted (cumulative).
    /// Source: `pg_stat_user_tables.n_tup_ins`
    pub n_tup_ins: i64,

    /// Rows updated (cumulative).
    /// Source: `pg_stat_user_tables.n_tup_upd`
    pub n_tup_upd: i64,

    /// Rows deleted (cumulative).
    /// Source: `pg_stat_user_tables.n_tup_del`
    pub n_tup_del: i64,

    /// Rows HOT-updated (cumulative).
    /// Source: `pg_stat_user_tables.n_tup_hot_upd`
    pub n_tup_hot_upd: i64,

    /// Estimated live rows (gauge).
    /// Source: `pg_stat_user_tables.n_live_tup`
    pub n_live_tup: i64,

    /// Estimated dead rows (gauge, bloat indicator).
    /// Source: `pg_stat_user_tables.n_dead_tup`
    pub n_dead_tup: i64,

    /// Manual vacuum count (cumulative).
    /// Source: `pg_stat_user_tables.vacuum_count`
    pub vacuum_count: i64,

    /// Autovacuum count (cumulative).
    /// Source: `pg_stat_user_tables.autovacuum_count`
    pub autovacuum_count: i64,

    /// Manual analyze count (cumulative).
    /// Source: `pg_stat_user_tables.analyze_count`
    pub analyze_count: i64,

    /// Autoanalyze count (cumulative).
    /// Source: `pg_stat_user_tables.autoanalyze_count`
    pub autoanalyze_count: i64,

    /// Last manual vacuum time (epoch secs, 0 = never).
    /// Source: `pg_stat_user_tables.last_vacuum`
    pub last_vacuum: i64,

    /// Last autovacuum time (epoch secs, 0 = never).
    /// Source: `pg_stat_user_tables.last_autovacuum`
    pub last_autovacuum: i64,

    /// Last manual analyze time (epoch secs, 0 = never).
    /// Source: `pg_stat_user_tables.last_analyze`
    pub last_analyze: i64,

    /// Last autoanalyze time (epoch secs, 0 = never).
    /// Source: `pg_stat_user_tables.last_autoanalyze`
    pub last_autoanalyze: i64,

    /// Table size in bytes.
    /// Source: `pg_relation_size(relid)`
    pub size_bytes: i64,

    // ---- pg_statio_user_tables — I/O block counters (cumulative) ----
    /// Heap blocks read from disk (cumulative).
    /// Source: `pg_statio_user_tables.heap_blks_read`
    #[serde(default)]
    pub heap_blks_read: i64,

    /// Heap blocks found in buffer cache (cumulative).
    /// Source: `pg_statio_user_tables.heap_blks_hit`
    #[serde(default)]
    pub heap_blks_hit: i64,

    /// Index blocks read from disk (cumulative).
    /// Source: `pg_statio_user_tables.idx_blks_read`
    #[serde(default)]
    pub idx_blks_read: i64,

    /// Index blocks found in buffer cache (cumulative).
    /// Source: `pg_statio_user_tables.idx_blks_hit`
    #[serde(default)]
    pub idx_blks_hit: i64,

    /// TOAST blocks read from disk (cumulative).
    /// Source: `pg_statio_user_tables.toast_blks_read`
    #[serde(default)]
    pub toast_blks_read: i64,

    /// TOAST blocks found in buffer cache (cumulative).
    /// Source: `pg_statio_user_tables.toast_blks_hit`
    #[serde(default)]
    pub toast_blks_hit: i64,

    /// TOAST index blocks read from disk (cumulative).
    /// Source: `pg_statio_user_tables.tidx_blks_read`
    #[serde(default)]
    pub tidx_blks_read: i64,

    /// TOAST index blocks found in buffer cache (cumulative).
    /// Source: `pg_statio_user_tables.tidx_blks_hit`
    #[serde(default)]
    pub tidx_blks_hit: i64,

    /// Unix timestamp (seconds since epoch) when this data was collected from PostgreSQL.
    /// Used to calculate accurate rates when collector caches pg_stat_user_tables.
    /// Source: set by collector at collection time
    #[serde(default)]
    pub collected_at: i64,
}

/// Per-index statistics from pg_stat_user_indexes.
///
/// Source: `SELECT * FROM pg_stat_user_indexes`
///
/// This per-database view shows one row per user index, with cumulative
/// counters for index scans and tuple operations.
/// Only indexes in the currently connected database are visible.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PgStatUserIndexesInfo {
    /// Index OID (diff key).
    /// Source: `pg_stat_user_indexes.indexrelid`
    pub indexrelid: u32,

    /// Parent table OID.
    /// Source: `pg_stat_user_indexes.relid`
    pub relid: u32,

    /// Hash of database name.
    /// Source: set by collector from the connected database name — interned via StringInterner.
    #[serde(default)]
    pub datname_hash: u64,

    /// Hash of schema name.
    /// Source: `pg_stat_user_indexes.schemaname` — interned via StringInterner
    pub schemaname_hash: u64,

    /// Hash of table name.
    /// Source: `pg_stat_user_indexes.relname` — interned via StringInterner
    pub relname_hash: u64,

    /// Hash of index name.
    /// Source: `pg_stat_user_indexes.indexrelname` — interned via StringInterner
    pub indexrelname_hash: u64,

    /// Index scans initiated (cumulative).
    /// Source: `pg_stat_user_indexes.idx_scan`
    pub idx_scan: i64,

    /// Index entries returned (cumulative).
    /// Source: `pg_stat_user_indexes.idx_tup_read`
    pub idx_tup_read: i64,

    /// Live table rows fetched by index scans (cumulative).
    /// Source: `pg_stat_user_indexes.idx_tup_fetch`
    pub idx_tup_fetch: i64,

    /// Index size in bytes.
    /// Source: `pg_relation_size(indexrelid)`
    pub size_bytes: i64,

    // pg_statio_user_indexes — I/O block counters (cumulative)
    #[serde(default)]
    pub idx_blks_read: i64,
    #[serde(default)]
    pub idx_blks_hit: i64,

    /// Unix timestamp (seconds since epoch) when this data was collected from PostgreSQL.
    /// Used to calculate accurate rates when collector caches pg_stat_user_indexes.
    /// Source: set by collector at collection time
    #[serde(default)]
    pub collected_at: i64,
}

/// Lock tree node from recursive CTE on pg_locks + pg_stat_activity.
///
/// Each node represents a session participating in a blocking chain.
/// Rows are ordered by (root_pid, path) for DFS traversal.
/// `depth=1` = root blocker, `depth>1` = blocked sessions.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PgLockTreeNode {
    /// Process ID of the backend.
    pub pid: i32,
    /// Depth in the blocking tree (1 = root blocker).
    pub depth: i32,
    /// PID of the root blocker in this tree.
    pub root_pid: i32,

    /// Hash of database name.
    pub datname_hash: u64,
    /// Hash of user name.
    pub usename_hash: u64,
    /// Hash of session state (active, idle in transaction, etc.).
    pub state_hash: u64,
    /// Hash of wait event type (Lock, LWLock, etc.).
    pub wait_event_type_hash: u64,
    /// Hash of wait event name.
    pub wait_event_hash: u64,
    /// Hash of query text.
    pub query_hash: u64,
    /// Hash of application_name.
    pub application_name_hash: u64,
    /// Hash of backend_type.
    pub backend_type_hash: u64,

    /// Transaction start (epoch seconds).
    pub xact_start: i64,
    /// Query start (epoch seconds).
    pub query_start: i64,
    /// Last state change (epoch seconds, proxy for wait start).
    pub state_change: i64,

    /// Hash of lock type (relation, transactionid, etc.).
    pub lock_type_hash: u64,
    /// Hash of lock mode (AccessExclusiveLock, etc.).
    pub lock_mode_hash: u64,
    /// Whether this lock is granted (true) or being waited for (false).
    pub lock_granted: bool,
    /// Hash of lock target (schema.table or relation OID).
    pub lock_target_hash: u64,
}

/// Background writer and checkpointer statistics.
///
/// Source: `pg_stat_bgwriter` (PG < 17: combined view)
///         `pg_stat_bgwriter` + `pg_stat_checkpointer` (PG 17+: split views)
///
/// This is a singleton view (one row). All fields are cumulative counters.
/// Rates are computed in the TUI from deltas between consecutive snapshots.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PgStatBgwriterInfo {
    /// Scheduled checkpoints performed (cumulative).
    pub checkpoints_timed: i64,
    /// Requested checkpoints performed (cumulative).
    pub checkpoints_req: i64,
    /// Total time spent writing checkpoint files to disk (ms, cumulative).
    pub checkpoint_write_time: f64,
    /// Total time spent synchronizing checkpoint files to disk (ms, cumulative).
    pub checkpoint_sync_time: f64,
    /// Buffers written during checkpoints (cumulative).
    pub buffers_checkpoint: i64,
    /// Buffers written by the background writer (cumulative).
    pub buffers_clean: i64,
    /// Times background writer stopped due to bgwriter_lru_maxpages (cumulative).
    pub maxwritten_clean: i64,
    /// Buffers written directly by backends (cumulative). 0 on PG 17+.
    pub buffers_backend: i64,
    /// Times backends had to execute their own fsync (cumulative). 0 on PG 17+.
    pub buffers_backend_fsync: i64,
    /// Buffers allocated (cumulative).
    pub buffers_alloc: i64,
}

// ---------------------------------------------------------------------------
// PostgreSQL log errors
// ---------------------------------------------------------------------------

/// Severity level of a PostgreSQL log entry.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub enum PgLogSeverity {
    Error = 0,
    Fatal = 1,
    Panic = 2,
}

/// A grouped PostgreSQL log error entry within a snapshot interval.
///
/// Errors are normalized into patterns (concrete values replaced with `"..."`)
/// and grouped by pattern + severity. Each entry represents one unique pattern
/// with its occurrence count.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PgLogEntry {
    /// Normalized error pattern (through StringInterner).
    /// e.g. `relation "..." does not exist`
    pub pattern_hash: u64,
    /// Severity level.
    pub severity: PgLogSeverity,
    /// Number of occurrences in this snapshot interval.
    pub count: u32,
    /// One concrete sample of the original message (through StringInterner).
    /// e.g. `relation "users" does not exist`
    pub sample_hash: u64,
    /// SQL statement that caused the error (from STATEMENT: log line, through StringInterner).
    /// 0 if not available.
    #[serde(default)]
    pub statement_hash: u64,
}

/// Operational event counts from PostgreSQL logs for a snapshot interval.
///
/// Tracks checkpoint and autovacuum/autoanalyze events detected from LOG-level
/// messages in PostgreSQL log files.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct PgLogEventsInfo {
    /// Number of checkpoint events (starting + complete) in this interval.
    pub checkpoint_count: u16,
    /// Number of autovacuum + autoanalyze events in this interval.
    pub autovacuum_count: u16,
}

/// Type of PostgreSQL log event.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum PgLogEventType {
    CheckpointStarting,
    CheckpointComplete,
    Autovacuum,
    Autoanalyze,
}

/// A detailed PostgreSQL log event entry within a snapshot interval.
///
/// Stores parsed fields from checkpoint/autovacuum LOG messages.
/// This is the source-of-truth data stored in snapshots (.zst files).
/// Heatmap counts are derived from these entries.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PgLogEventEntry {
    /// Type of event.
    pub event_type: PgLogEventType,
    /// Full raw log message.
    pub message: String,
    /// Table name for autovacuum/autoanalyze (e.g. "db.schema.table"), empty for checkpoint.
    pub table_name: String,
    /// Elapsed time in seconds (checkpoint total_time or vacuum elapsed).
    pub elapsed_s: f64,
    /// Extra numeric field 1: checkpoint buffers_written / autovacuum tuples_removed.
    pub extra_num1: i64,
    /// Extra numeric field 2: checkpoint distance_kb / autovacuum pages_removed.
    pub extra_num2: i64,
    /// Buffer cache hits (autovacuum/autoanalyze).
    #[serde(default)]
    pub buffer_hits: i64,
    /// Buffer cache misses (autovacuum/autoanalyze).
    #[serde(default)]
    pub buffer_misses: i64,
    /// Buffers dirtied (autovacuum/autoanalyze).
    #[serde(default)]
    pub buffer_dirtied: i64,
    /// Average read rate in MB/s (autovacuum/autoanalyze).
    #[serde(default)]
    pub avg_read_rate_mbs: f64,
    /// Average write rate in MB/s (autovacuum/autoanalyze).
    #[serde(default)]
    pub avg_write_rate_mbs: f64,
    /// CPU user time in seconds (autovacuum/autoanalyze).
    #[serde(default)]
    pub cpu_user_s: f64,
    /// CPU system time in seconds (autovacuum/autoanalyze).
    #[serde(default)]
    pub cpu_system_s: f64,
    /// WAL records generated (autovacuum) / WAL files added (checkpoint).
    #[serde(default)]
    pub wal_records: i64,
    /// WAL full page images (autovacuum) / WAL files removed (checkpoint).
    #[serde(default)]
    pub wal_fpi: i64,
    /// WAL bytes written (autovacuum) / WAL files recycled (checkpoint).
    #[serde(default)]
    pub wal_bytes: i64,
    /// Extra numeric field 3: checkpoint estimate_kb.
    #[serde(default)]
    pub extra_num3: i64,
}

/// Single PostgreSQL setting from pg_settings view.
///
/// Stored as raw (name, setting, unit) triple — no version-specific assumptions.
/// The `setting` column in pg_settings already contains values in GUC base units
/// (e.g. 8kB blocks for shared_buffers, seconds for checkpoint_timeout).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PgSettingEntry {
    /// Setting name (e.g. "shared_buffers", "work_mem").
    pub name: String,
    /// Value in base units as reported by pg_settings.setting.
    pub setting: String,
    /// Unit reported by pg_settings.unit (e.g. "8kB", "ms", "s", "kB", or empty).
    pub unit: String,
}
