//! Snapshot and delta structures for efficient storage.
//!
//! These structures define how metrics data is organized for storage.
//! The storage system uses delta-encoding: only the first snapshot in a chunk
//! is stored fully, subsequent snapshots store only changes (deltas).

use serde::{Deserialize, Serialize};

use super::cgroup::CgroupInfo;
use super::postgres::{PgStatActivityInfo, PgStatStatementsInfo};
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

/// Delta representation of a DataBlock for efficient storage.
///
/// Instead of storing full data on every snapshot, we store only
/// the changes (updates and removals) compared to the previous snapshot.
/// This significantly reduces storage size when most data remains unchanged.
///
/// For collection types (Processes, Networks, etc.), we track:
/// - `updates`: new or modified entries
/// - `removals`: IDs/hashes of removed entries
///
/// For singleton types (SystemLoad, SystemMem, etc.), we store the full value
/// since there's only one instance and changes are typically expected.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum DataBlockDiff {
    /// Process changes.
    /// `removals` contains PIDs of terminated processes.
    Processes {
        updates: Vec<ProcessInfo>,
        removals: Vec<u32>,
    },

    /// PostgreSQL activity changes.
    /// `removals` contains PIDs of disconnected backends.
    PgStatActivity {
        updates: Vec<PgStatActivityInfo>,
        removals: Vec<i32>,
    },

    /// PostgreSQL statements changes.
    /// `removals` contains queryids of evicted statements.
    PgStatStatements {
        updates: Vec<PgStatStatementsInfo>,
        removals: Vec<i64>,
    },

    /// CPU statistics changes (per-core).
    /// `removals` contains cpu_ids of removed CPUs (rare, hot-plug).
    SystemCpu {
        updates: Vec<SystemCpuInfo>,
        removals: Vec<i16>,
    },

    /// Full load average (always changes).
    SystemLoad(SystemLoadInfo),

    /// Full memory stats (always changes).
    SystemMem(SystemMemInfo),

    /// Network interface changes.
    /// `removals` contains name_hashes of removed interfaces.
    SystemNet {
        updates: Vec<SystemNetInfo>,
        removals: Vec<u64>,
    },

    /// Disk device changes.
    /// `removals` contains device_hashes of removed devices.
    SystemDisk {
        updates: Vec<SystemDiskInfo>,
        removals: Vec<u64>,
    },

    /// Full PSI data (always reported for all resources).
    SystemPsi(Vec<SystemPsiInfo>),

    /// Full vmstat counters.
    SystemVmstat(SystemVmstatInfo),

    /// Full file descriptor stats.
    SystemFile(SystemFileInfo),

    /// Interrupt counter changes.
    /// `removals` contains irq_hashes of removed IRQs.
    SystemInterrupts {
        updates: Vec<SystemInterruptInfo>,
        removals: Vec<u64>,
    },

    /// Softirq counter changes.
    /// `removals` contains name_hashes of removed softirqs.
    SystemSoftirqs {
        updates: Vec<SystemSoftirqInfo>,
        removals: Vec<u64>,
    },

    /// Full system stats.
    SystemStat(SystemStatInfo),

    /// Full network SNMP stats.
    SystemNetSnmp(SystemNetSnmpInfo),

    /// Full cgroup metrics (container only).
    Cgroup(CgroupInfo),
}

/// A point-in-time capture of all collected metrics.
///
/// Snapshot represents a complete picture of the system state at a given moment.
/// It contains multiple DataBlocks, each representing a different category
/// of metrics (processes, system stats, database stats, etc.).
///
/// Snapshots are taken periodically (e.g., every 10 seconds) and stored
/// using delta-encoding for efficiency.
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

/// Represents either a full snapshot or incremental changes.
///
/// Delta is the fundamental unit of storage. Within a chunk:
/// - The first entry is always `Delta::Full` (complete baseline)
/// - Subsequent entries are `Delta::Diff` (only changes)
///
/// This approach dramatically reduces storage size because:
/// - Most processes don't change between snapshots
/// - System counters often have small deltas
/// - String interning eliminates duplicate string storage
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Delta {
    /// Complete snapshot data (used as baseline).
    /// Stored at the beginning of each chunk for random access.
    Full(Snapshot),

    /// Incremental changes from previous snapshot.
    /// Contains only modified/removed data blocks.
    Diff {
        /// Timestamp of this snapshot.
        timestamp: i64,
        /// Changed data blocks since previous snapshot.
        blocks: Vec<DataBlockDiff>,
    },
}

impl Delta {
    /// Returns the timestamp of this delta.
    #[allow(dead_code)]
    pub fn timestamp(&self) -> i64 {
        match self {
            Delta::Full(s) => s.timestamp,
            Delta::Diff { timestamp, .. } => *timestamp,
        }
    }
}
