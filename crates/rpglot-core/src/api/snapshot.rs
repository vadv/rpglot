//! API snapshot types — full atomic JSON payload.
//!
//! One `ApiSnapshot` = one complete point-in-time view of the system.
//! All strings resolved from interner, all rates pre-computed.

use serde::Serialize;
use utoipa::ToSchema;

/// Top-level atomic snapshot sent to clients.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiSnapshot {
    /// Unix timestamp (seconds since epoch).
    pub timestamp: i64,
    /// Timestamp of the previous snapshot in history. Absent for the first snapshot and in live mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_timestamp: Option<i64>,
    /// Timestamp of the next snapshot in history. Absent for the last snapshot and in live mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_timestamp: Option<i64>,
    /// System-level summary metrics.
    pub system: SystemSummary,
    /// PostgreSQL instance-level summary metrics.
    pub pg: PgSummary,
    /// OS processes.
    pub prc: Vec<ApiProcessRow>,
    /// pg_stat_activity rows.
    pub pga: Vec<PgActivityRow>,
    /// pg_stat_statements rows (with rates).
    pub pgs: Vec<PgStatementsRow>,
    /// pg_stat_user_tables rows (with rates).
    pub pgt: Vec<PgTablesRow>,
    /// pg_stat_user_indexes rows (with rates).
    pub pgi: Vec<PgIndexesRow>,
    /// PostgreSQL log events and errors.
    pub pge: Vec<PgEventsRow>,
    /// pg_locks blocking tree (flat, with depth).
    pub pgl: Vec<PgLocksRow>,
}

// ============================================================
// System summary
// ============================================================

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SystemSummary {
    pub cpu: Option<CpuSummary>,
    pub load: Option<LoadSummary>,
    pub memory: Option<MemorySummary>,
    pub swap: Option<SwapSummary>,
    pub disks: Vec<DiskSummary>,
    pub networks: Vec<NetworkSummary>,
    pub psi: Option<PsiSummary>,
    pub vmstat: Option<VmstatSummary>,
    /// Cgroup CPU metrics (container mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup_cpu: Option<CgroupCpuSummary>,
    /// Cgroup memory metrics (container mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup_memory: Option<CgroupMemorySummary>,
    /// Cgroup PIDs metrics (container mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup_pids: Option<CgroupPidsSummary>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CpuSummary {
    /// System CPU %.
    pub sys_pct: f64,
    /// User CPU %.
    pub usr_pct: f64,
    /// IRQ CPU %.
    pub irq_pct: f64,
    /// I/O wait %.
    pub iow_pct: f64,
    /// Idle %.
    pub idle_pct: f64,
    /// Steal %.
    pub steal_pct: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LoadSummary {
    pub avg1: f32,
    pub avg5: f32,
    pub avg15: f32,
    /// Total number of processes.
    pub nr_threads: u32,
    /// Number of running processes.
    pub nr_running: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemorySummary {
    /// Total memory in KB.
    pub total_kb: u64,
    /// Available memory in KB.
    pub available_kb: u64,
    /// Cached in KB.
    pub cached_kb: u64,
    /// Buffers in KB.
    pub buffers_kb: u64,
    /// Slab in KB.
    pub slab_kb: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SwapSummary {
    /// Total swap in KB.
    pub total_kb: u64,
    /// Free swap in KB.
    pub free_kb: u64,
    /// Used swap in KB.
    pub used_kb: u64,
    /// Dirty pages in KB.
    pub dirty_kb: u64,
    /// Writeback in KB.
    pub writeback_kb: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DiskSummary {
    pub name: String,
    /// Read throughput in bytes/s.
    pub read_bytes_s: f64,
    /// Write throughput in bytes/s.
    pub write_bytes_s: f64,
    /// Read IOPS.
    pub read_iops: f64,
    /// Write IOPS.
    pub write_iops: f64,
    /// Utilization percentage.
    pub util_pct: f64,
    /// Average read await (ms per read I/O).
    pub r_await_ms: f64,
    /// Average write await (ms per write I/O).
    pub w_await_ms: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NetworkSummary {
    pub name: String,
    /// RX bytes/s.
    pub rx_bytes_s: f64,
    /// TX bytes/s.
    pub tx_bytes_s: f64,
    /// RX packets/s.
    pub rx_packets_s: f64,
    /// TX packets/s.
    pub tx_packets_s: f64,
    /// Errors/s (RX + TX).
    pub errors_s: f64,
    /// Drops/s (RX + TX).
    pub drops_s: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PsiSummary {
    /// CPU pressure (some avg10).
    pub cpu_some_pct: f64,
    /// Memory pressure (some avg10).
    pub mem_some_pct: f64,
    /// I/O pressure (some avg10).
    pub io_some_pct: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VmstatSummary {
    /// Pages paged in per second.
    pub pgin_s: f64,
    /// Pages paged out per second.
    pub pgout_s: f64,
    /// Swap in per second.
    pub swin_s: f64,
    /// Swap out per second.
    pub swout_s: f64,
    /// Page faults per second.
    pub pgfault_s: f64,
    /// Context switches per second.
    pub ctxsw_s: f64,
}

// ============================================================
// Cgroup summary (container mode)
// ============================================================

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CgroupCpuSummary {
    /// CPU limit in cores (quota / period).
    pub limit_cores: f64,
    /// CPU usage percentage relative to limit.
    pub used_pct: f64,
    /// User CPU percentage of total usage.
    pub usr_pct: f64,
    /// System CPU percentage of total usage.
    pub sys_pct: f64,
    /// Throttled time in ms (delta).
    pub throttled_ms: f64,
    /// Throttle event count (delta).
    pub nr_throttled: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CgroupMemorySummary {
    /// Memory limit in bytes.
    pub limit_bytes: u64,
    /// Current memory usage in bytes.
    pub used_bytes: u64,
    /// Usage percentage of limit (includes file cache — evictable).
    pub used_pct: f64,
    /// Non-evictable memory percentage of limit (anon + slab).
    pub anon_pct: f64,
    /// Anonymous memory in bytes.
    pub anon_bytes: u64,
    /// File-backed (page cache) memory in bytes.
    pub file_bytes: u64,
    /// Slab memory in bytes.
    pub slab_bytes: u64,
    /// Cumulative OOM kill count.
    pub oom_kills: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CgroupPidsSummary {
    /// Current number of processes.
    pub current: u64,
    /// Maximum allowed processes.
    pub max: u64,
}

// ============================================================
// PostgreSQL summary
// ============================================================

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PgSummary {
    /// Transactions per second (commit + rollback).
    pub tps: Option<f64>,
    /// Buffer cache hit ratio (0..100).
    pub hit_ratio_pct: Option<f64>,
    /// Backend IO hit ratio (0..100): (rchar - read_bytes) / rchar.
    /// Based on /proc/[pid]/io for PG backend processes.
    pub backend_io_hit_pct: Option<f64>,
    /// Tuples returned+fetched per second.
    pub tuples_s: Option<f64>,
    /// Temp bytes per second.
    pub temp_bytes_s: Option<f64>,
    /// Deadlocks in the interval.
    pub deadlocks: Option<f64>,
    /// Background writer stats.
    pub bgwriter: Option<BgwriterSummary>,
    /// Total PostgreSQL log error count in this snapshot (ERROR+FATAL+PANIC).
    pub errors: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BgwriterSummary {
    /// Checkpoints per minute.
    pub checkpoints_per_min: f64,
    /// Checkpoint write time ms (in interval).
    pub checkpoint_write_time_ms: f64,
    /// Backend buffers written per second.
    pub buffers_backend_s: f64,
    /// Buffers cleaned per second.
    pub buffers_clean_s: f64,
    /// maxwritten_clean count.
    pub maxwritten_clean: f64,
    /// Buffers allocated per second.
    pub buffers_alloc_s: f64,
}

// ============================================================
// Tab rows
// ============================================================

/// OS process row (from /proc).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiProcessRow {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub cmdline: String,
    pub state: String,
    pub num_threads: u32,
    pub btime: u32,
    /// CPU percentage (0..100).
    pub cpu_pct: f64,
    /// User CPU ticks.
    pub utime: u64,
    /// System CPU ticks.
    pub stime: u64,
    /// Current CPU number.
    pub curcpu: i32,
    /// Run delay (nanoseconds).
    pub rundelay: u64,
    /// Nice value (-20..19).
    pub nice: i32,
    /// Scheduling priority.
    pub priority: i32,
    /// Real-time scheduling priority.
    pub rtprio: i32,
    /// Scheduling policy.
    pub policy: i32,
    /// I/O wait ticks.
    pub blkdelay: u64,
    /// Voluntary context switches (cumulative).
    pub nvcsw: u64,
    /// Involuntary context switches (cumulative).
    pub nivcsw: u64,
    /// Voluntary context switches/s.
    pub nvcsw_s: Option<f64>,
    /// Involuntary context switches/s.
    pub nivcsw_s: Option<f64>,
    /// Memory percentage (0..100).
    pub mem_pct: f64,
    /// Virtual memory (KB).
    pub vsize_kb: u64,
    /// Resident set size (KB).
    pub rsize_kb: u64,
    /// Proportional set size (KB).
    pub psize_kb: u64,
    /// Virtual memory growth (KB delta).
    pub vgrow_kb: i64,
    /// Resident memory growth (KB delta).
    pub rgrow_kb: i64,
    /// Swap usage (KB).
    pub vswap_kb: u64,
    /// Code segment (KB).
    pub vstext_kb: u64,
    /// Data segment (KB).
    pub vdata_kb: u64,
    /// Stack (KB).
    pub vstack_kb: u64,
    /// Shared libraries (KB).
    pub vslibs_kb: u64,
    /// Locked memory (KB).
    pub vlock_kb: u64,
    /// Minor page faults.
    pub minflt: u64,
    /// Major page faults.
    pub majflt: u64,
    /// Read bytes/s.
    pub read_bytes_s: Option<f64>,
    /// Write bytes/s.
    pub write_bytes_s: Option<f64>,
    /// Read ops/s.
    pub read_ops_s: Option<f64>,
    /// Write ops/s.
    pub write_ops_s: Option<f64>,
    /// Total read bytes (cumulative).
    pub total_read_bytes: u64,
    /// Total write bytes (cumulative).
    pub total_write_bytes: u64,
    /// Total read ops (cumulative).
    pub total_read_ops: u64,
    /// Total write ops (cumulative).
    pub total_write_ops: u64,
    /// Cancelled write bytes.
    pub cancelled_write_bytes: u64,
    /// Real user ID.
    pub uid: u32,
    /// Effective user ID.
    pub euid: u32,
    /// Real group ID.
    pub gid: u32,
    /// Effective group ID.
    pub egid: u32,
    /// Controlling terminal.
    pub tty: u16,
    /// Exit signal.
    pub exit_signal: i32,
    /// Associated PG query (if pid matches pg_stat_activity).
    pub pg_query: Option<String>,
    /// Associated PG backend type.
    pub pg_backend_type: Option<String>,
}

/// pg_stat_activity row.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PgActivityRow {
    pub pid: i32,
    pub database: String,
    pub user: String,
    pub application_name: String,
    pub client_addr: String,
    pub state: String,
    pub wait_event_type: String,
    pub wait_event: String,
    pub backend_type: String,
    pub query: String,
    pub query_id: i64,
    /// Query duration in seconds (now - query_start), None if no active query.
    pub query_duration_s: Option<i64>,
    /// Transaction duration in seconds (now - xact_start).
    pub xact_duration_s: Option<i64>,
    /// Backend duration in seconds (now - backend_start).
    pub backend_duration_s: Option<i64>,
    /// backend_start epoch.
    pub backend_start: i64,
    /// xact_start epoch.
    pub xact_start: i64,
    /// query_start epoch.
    pub query_start: i64,
    /// CPU% from OS process (if matched).
    pub cpu_pct: Option<f64>,
    /// Resident memory KB from OS process (if matched).
    pub rss_kb: Option<u64>,
    /// Read syscall bytes/s (rchar delta / dt) from /proc/[pid]/io.
    pub rchar_s: Option<f64>,
    /// Write syscall bytes/s (wchar delta / dt) from /proc/[pid]/io.
    pub wchar_s: Option<f64>,
    /// Physical read bytes/s (read_bytes delta / dt) from /proc/[pid]/io.
    pub read_bytes_s: Option<f64>,
    /// Physical write bytes/s (write_bytes delta / dt) from /proc/[pid]/io.
    pub write_bytes_s: Option<f64>,
    /// Read ops/s (syscr delta / dt) from /proc/[pid]/io.
    pub read_ops_s: Option<f64>,
    /// Write ops/s (syscw delta / dt) from /proc/[pid]/io.
    pub write_ops_s: Option<f64>,
    /// Mean exec time from pg_stat_statements (if query_id matched).
    pub stmt_mean_exec_time_ms: Option<f64>,
    /// Max exec time from pg_stat_statements.
    pub stmt_max_exec_time_ms: Option<f64>,
    /// Calls/s from pg_stat_statements.
    pub stmt_calls_s: Option<f64>,
    /// Buffer hit % from pg_stat_statements.
    pub stmt_hit_pct: Option<f64>,
}

/// pg_stat_statements row with pre-computed rates.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PgStatementsRow {
    pub queryid: i64,
    pub database: String,
    pub user: String,
    pub query: String,
    /// Cumulative call count.
    pub calls: i64,
    /// Cumulative total rows.
    pub rows: i64,
    /// Mean execution time (ms) — from PG stats.
    pub mean_exec_time_ms: f64,
    /// Min execution time (ms).
    pub min_exec_time_ms: f64,
    /// Max execution time (ms).
    pub max_exec_time_ms: f64,
    /// Stddev execution time (ms).
    pub stddev_exec_time_ms: f64,
    // --- rates (per second, computed from deltas) ---
    pub calls_s: Option<f64>,
    pub rows_s: Option<f64>,
    /// Execution time rate (ms/s).
    pub exec_time_ms_s: Option<f64>,
    pub shared_blks_read_s: Option<f64>,
    pub shared_blks_hit_s: Option<f64>,
    pub shared_blks_dirtied_s: Option<f64>,
    pub shared_blks_written_s: Option<f64>,
    pub local_blks_read_s: Option<f64>,
    pub local_blks_written_s: Option<f64>,
    pub temp_blks_read_s: Option<f64>,
    pub temp_blks_written_s: Option<f64>,
    /// Temp I/O rate in MB/s.
    pub temp_mb_s: Option<f64>,
    // --- computed fields ---
    /// rows / calls.
    pub rows_per_call: Option<f64>,
    /// shared_blks_hit / (hit + read) * 100.
    pub hit_pct: Option<f64>,
    /// Cumulative total plan time (ms).
    pub total_plan_time: f64,
    /// Cumulative WAL records generated.
    pub wal_records: i64,
    /// Cumulative WAL bytes generated.
    pub wal_bytes: i64,
    /// Cumulative total execution time (ms).
    pub total_exec_time: f64,
}

/// pg_stat_user_tables row with pre-computed rates.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PgTablesRow {
    pub relid: u32,
    pub database: String,
    pub schema: String,
    pub table: String,
    /// `schema.table` or just `table` if public.
    pub display_name: String,
    /// Current live tuple count (gauge).
    pub n_live_tup: i64,
    /// Current dead tuple count (gauge).
    pub n_dead_tup: i64,
    /// Table size in bytes.
    pub size_bytes: i64,
    /// Last autovacuum epoch (0 = never).
    pub last_autovacuum: i64,
    /// Last autoanalyze epoch (0 = never).
    pub last_autoanalyze: i64,
    // --- rates ---
    pub seq_scan_s: Option<f64>,
    pub seq_tup_read_s: Option<f64>,
    pub idx_scan_s: Option<f64>,
    pub idx_tup_fetch_s: Option<f64>,
    pub n_tup_ins_s: Option<f64>,
    pub n_tup_upd_s: Option<f64>,
    pub n_tup_del_s: Option<f64>,
    pub n_tup_hot_upd_s: Option<f64>,
    pub vacuum_count_s: Option<f64>,
    pub autovacuum_count_s: Option<f64>,
    pub heap_blks_read_s: Option<f64>,
    pub heap_blks_hit_s: Option<f64>,
    pub idx_blks_read_s: Option<f64>,
    pub idx_blks_hit_s: Option<f64>,
    // --- computed fields ---
    /// seq_tup_read_s + idx_tup_fetch_s.
    pub tot_tup_read_s: Option<f64>,
    /// heap_blks_read_s + idx_blks_read_s.
    pub disk_blks_read_s: Option<f64>,
    /// all hits / (all hits + all reads) * 100.
    pub io_hit_pct: Option<f64>,
    /// seq_scan / (seq_scan + idx_scan) * 100.
    pub seq_pct: Option<f64>,
    /// n_dead_tup / (live + dead) * 100.
    pub dead_pct: Option<f64>,
    /// hot_upd / upd * 100 (cumulative-based).
    pub hot_pct: Option<f64>,
    // --- additional rates ---
    pub analyze_count_s: Option<f64>,
    pub autoanalyze_count_s: Option<f64>,
    /// Last manual vacuum epoch (0 = never).
    pub last_vacuum: i64,
    /// Last manual analyze epoch (0 = never).
    pub last_analyze: i64,
    // --- TOAST I/O rates ---
    pub toast_blks_read_s: Option<f64>,
    pub toast_blks_hit_s: Option<f64>,
    pub tidx_blks_read_s: Option<f64>,
    pub tidx_blks_hit_s: Option<f64>,
}

/// pg_stat_user_indexes row with pre-computed rates.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PgIndexesRow {
    pub indexrelid: u32,
    /// Parent table relid.
    pub relid: u32,
    pub database: String,
    pub schema: String,
    pub table: String,
    pub index: String,
    pub display_table: String,
    /// Cumulative index scan count (gauge).
    pub idx_scan: i64,
    /// Index size in bytes.
    pub size_bytes: i64,
    // --- rates ---
    pub idx_scan_s: Option<f64>,
    pub idx_tup_read_s: Option<f64>,
    pub idx_tup_fetch_s: Option<f64>,
    pub idx_blks_read_s: Option<f64>,
    pub idx_blks_hit_s: Option<f64>,
    // --- computed fields ---
    /// idx_blks_hit / (hit + read) * 100.
    pub io_hit_pct: Option<f64>,
    /// Alias for idx_blks_read_s (schema consistency).
    pub disk_blks_read_s: Option<f64>,
}

/// PostgreSQL log event/error row.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PgEventsRow {
    /// Unique event identifier (hash-based for errors, sequential for events).
    pub event_id: u64,
    /// Event type: "error", "fatal", "panic", "checkpoint_starting",
    /// "checkpoint_complete", "autovacuum", "autoanalyze".
    pub event_type: String,
    /// Severity: "ERROR"/"FATAL"/"PANIC" for errors, "LOG" for events.
    pub severity: String,
    /// Number of occurrences (N for grouped errors, 1 for events).
    pub count: u32,
    /// Table name (for autovacuum/autoanalyze), empty for checkpoint/errors.
    pub table_name: String,
    /// Elapsed time in seconds (checkpoint total_time / vacuum elapsed).
    pub elapsed_s: f64,
    /// Extra numeric 1: buffers_written (checkpoint) / tuples_removed (vacuum).
    pub extra_num1: i64,
    /// Extra numeric 2: distance_kb (checkpoint) / pages_removed (vacuum).
    pub extra_num2: i64,
    /// Extra numeric 3: estimate_kb (checkpoint), 0 for others.
    pub extra_num3: i64,
    /// Buffer cache hits (autovacuum/autoanalyze) / sync files (checkpoint).
    pub buffer_hits: i64,
    /// Buffer cache misses (autovacuum/autoanalyze).
    pub buffer_misses: i64,
    /// Buffers dirtied (autovacuum/autoanalyze).
    pub buffer_dirtied: i64,
    /// Average read rate in MB/s (autovacuum/autoanalyze).
    pub avg_read_rate_mbs: f64,
    /// Average write rate in MB/s (autovacuum/autoanalyze).
    pub avg_write_rate_mbs: f64,
    /// CPU user time in seconds (autovacuum/autoanalyze).
    pub cpu_user_s: f64,
    /// CPU system time in seconds (autovacuum/autoanalyze).
    pub cpu_system_s: f64,
    /// WAL records generated (autovacuum).
    pub wal_records: i64,
    /// WAL full page images (autovacuum).
    pub wal_fpi: i64,
    /// WAL bytes written (autovacuum).
    pub wal_bytes: i64,
    /// Error pattern or event message.
    pub message: String,
    /// Concrete error sample (errors only), empty for events.
    pub sample: String,
    /// SQL statement that caused the error (from STATEMENT: line), empty if not available.
    pub statement: String,
}

/// pg_locks blocking tree row (flat with depth).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PgLocksRow {
    pub pid: i32,
    /// Depth in blocking tree (1 = root blocker).
    pub depth: i32,
    /// Root blocking PID.
    pub root_pid: i32,
    pub database: String,
    pub user: String,
    pub application_name: String,
    pub state: String,
    pub wait_event_type: String,
    pub wait_event: String,
    pub backend_type: String,
    pub lock_type: String,
    pub lock_mode: String,
    pub lock_target: String,
    pub lock_granted: bool,
    pub query: String,
    /// Transaction start epoch.
    pub xact_start: i64,
    /// Query start epoch.
    pub query_start: i64,
    /// State change epoch.
    pub state_change: i64,
}
