//! Snapshot structures for storage.
//!
//! These structures define how metrics data is organized for storage.
//! Each snapshot is stored as an independent zstd frame within a chunk file,
//! enabling O(1) random access to any snapshot.

use serde::{Deserialize, Serialize};

use super::cgroup::CgroupInfo;
use super::postgres::{
    PgLockTreeNode, PgStatActivityInfo, PgStatBgwriterInfo, PgStatDatabaseInfo,
    PgStatStatementsInfo, PgStatUserIndexesInfo, PgStatUserTablesInfo,
};
use super::process::ProcessInfo;
use super::system::{
    SystemCpuInfo, SystemDiskInfo, SystemFileInfo, SystemInterruptInfo, SystemLoadInfo,
    SystemMemInfo, SystemNetInfo, SystemNetSnmpInfo, SystemPsiInfo, SystemSoftirqInfo,
    SystemStatInfo, SystemVmstatInfo,
};

/// A block of data of a specific type within a snapshot.
///
/// DataBlock is a tagged union that can contain different types of metrics.
/// This allows storing heterogeneous data (processes, PostgreSQL stats,
/// system metrics) in a single snapshot while maintaining type safety.
///
/// Each variant corresponds to a specific data source or category.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum DataBlock {
    /// Per-process information.
    /// Source: `/proc/[pid]/*` files
    Processes(Vec<ProcessInfo>),

    /// PostgreSQL backend activity.
    /// Source: `pg_stat_activity` view
    PgStatActivity(Vec<PgStatActivityInfo>),

    /// PostgreSQL query statistics.
    /// Source: `pg_stat_statements` extension
    PgStatStatements(Vec<PgStatStatementsInfo>),

    /// PostgreSQL database-level statistics.
    /// Source: `pg_stat_database` view
    PgStatDatabase(Vec<PgStatDatabaseInfo>),

    /// PostgreSQL per-table statistics (per-database view).
    /// Source: `pg_stat_user_tables` view
    PgStatUserTables(Vec<PgStatUserTablesInfo>),

    /// PostgreSQL per-index statistics (per-database view).
    /// Source: `pg_stat_user_indexes` view
    PgStatUserIndexes(Vec<PgStatUserIndexesInfo>),

    /// PostgreSQL lock tree (blocking chains).
    /// Source: recursive CTE on `pg_locks` + `pg_stat_activity`
    PgLockTree(Vec<PgLockTreeNode>),

    /// PostgreSQL background writer / checkpointer statistics (singleton).
    /// Source: `pg_stat_bgwriter` (+ `pg_stat_checkpointer` on PG 17+)
    PgStatBgwriter(PgStatBgwriterInfo),

    /// CPU usage statistics (total and per-core).
    /// Source: `/proc/stat`
    SystemCpu(Vec<SystemCpuInfo>),

    /// System load averages.
    /// Source: `/proc/loadavg`
    SystemLoad(SystemLoadInfo),

    /// Memory usage statistics.
    /// Source: `/proc/meminfo`
    SystemMem(SystemMemInfo),

    /// Network interface statistics.
    /// Source: `/proc/net/dev`
    SystemNet(Vec<SystemNetInfo>),

    /// Block device I/O statistics.
    /// Source: `/proc/diskstats`
    SystemDisk(Vec<SystemDiskInfo>),

    /// Pressure Stall Information (CPU, memory, I/O pressure).
    /// Source: `/proc/pressure/{cpu,memory,io}`
    SystemPsi(Vec<SystemPsiInfo>),

    /// Virtual memory statistics.
    /// Source: `/proc/vmstat`
    SystemVmstat(SystemVmstatInfo),

    /// File descriptor and inode limits.
    /// Source: `/proc/sys/fs/{file-nr,inode-state}`
    SystemFile(SystemFileInfo),

    /// Hardware interrupt counters.
    /// Source: `/proc/interrupts`
    SystemInterrupts(Vec<SystemInterruptInfo>),

    /// Software interrupt counters.
    /// Source: `/proc/softirqs`
    SystemSoftirqs(Vec<SystemSoftirqInfo>),

    /// Global system statistics (context switches, forks).
    /// Source: `/proc/stat`
    SystemStat(SystemStatInfo),

    /// Network protocol statistics (TCP/UDP).
    /// Source: `/proc/net/snmp`
    SystemNetSnmp(SystemNetSnmpInfo),

    /// Cgroup v2 metrics (container resource limits and usage).
    /// Source: `/sys/fs/cgroup/*`
    Cgroup(CgroupInfo),
}

/// A point-in-time capture of all collected metrics.
///
/// Snapshot represents a complete picture of the system state at a given moment.
/// It contains multiple DataBlocks, each representing a different category
/// of metrics (processes, system stats, database stats, etc.).
///
/// Snapshots are taken periodically (e.g., every 10 seconds) and stored
/// as independent zstd frames within chunk files for efficient random access.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Snapshot {
    /// Unix timestamp (seconds since epoch) when this snapshot was taken.
    /// Used for navigation and time-based queries.
    pub timestamp: i64,

    /// Collection of data blocks in this snapshot.
    /// Each block represents a different category of metrics.
    /// Not all block types need to be present in every snapshot.
    pub blocks: Vec<DataBlock>,
}
