//! Cgroup v2 metrics for container environments.
//!
//! These structures store resource limits and usage from Linux cgroup v2 filesystem.
//! Only collected when running inside a container (detected via `is_container()`).

use serde::{Deserialize, Serialize};

/// I/O cgroup metrics (per block device).
///
/// Source file:
/// - `/sys/fs/cgroup/io.stat` - per-device I/O counters for the cgroup
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct CgroupIoInfo {
    /// Block device major number.
    pub major: u32,
    /// Block device minor number.
    pub minor: u32,
    /// Bytes read.
    pub rbytes: u64,
    /// Bytes written.
    pub wbytes: u64,
    /// Read I/O operations.
    pub rios: u64,
    /// Write I/O operations.
    pub wios: u64,
}

/// CPU cgroup metrics.
///
/// Source files:
/// - `/sys/fs/cgroup/cpu.max` - quota and period
/// - `/sys/fs/cgroup/cpu.stat` - usage statistics
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct CgroupCpuInfo {
    /// CPU quota in microseconds per period (-1 = unlimited).
    /// From `cpu.max` first field.
    pub quota: i64,
    /// CPU period in microseconds.
    /// From `cpu.max` second field.
    pub period: u64,
    /// Total CPU usage in microseconds.
    /// From `cpu.stat` usage_usec.
    pub usage_usec: u64,
    /// User CPU usage in microseconds.
    /// From `cpu.stat` user_usec.
    pub user_usec: u64,
    /// System CPU usage in microseconds.
    /// From `cpu.stat` system_usec.
    pub system_usec: u64,
    /// Time throttled in microseconds.
    /// From `cpu.stat` throttled_usec.
    pub throttled_usec: u64,
    /// Number of throttling events.
    /// From `cpu.stat` nr_throttled.
    pub nr_throttled: u64,
}

/// Memory cgroup metrics.
///
/// Source files:
/// - `/sys/fs/cgroup/memory.max` - memory limit
/// - `/sys/fs/cgroup/memory.current` - current usage
/// - `/sys/fs/cgroup/memory.stat` - detailed statistics
/// - `/sys/fs/cgroup/memory.events` - OOM events
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct CgroupMemoryInfo {
    /// Memory limit in bytes (u64::MAX = unlimited).
    /// From `memory.max`.
    pub max: u64,
    /// Current memory usage in bytes.
    /// From `memory.current`.
    pub current: u64,
    /// Anonymous memory in bytes.
    /// From `memory.stat` anon.
    pub anon: u64,
    /// File-backed memory (page cache) in bytes.
    /// From `memory.stat` file.
    pub file: u64,
    /// Kernel memory in bytes.
    /// From `memory.stat` kernel.
    pub kernel: u64,
    /// Slab memory in bytes.
    /// From `memory.stat` slab.
    pub slab: u64,
    /// Number of OOM kills.
    /// From `memory.events` oom_kill.
    pub oom_kill: u64,
}

/// PIDs cgroup metrics.
///
/// Source files:
/// - `/sys/fs/cgroup/pids.current` - current process count
/// - `/sys/fs/cgroup/pids.max` - process limit
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct CgroupPidsInfo {
    /// Current number of processes.
    /// From `pids.current`.
    pub current: u64,
    /// Maximum allowed processes (u64::MAX = unlimited).
    /// From `pids.max`.
    pub max: u64,
}

/// Combined cgroup metrics for a container.
///
/// All fields are optional since some cgroup controllers may be disabled.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct CgroupInfo {
    /// CPU metrics (if cpu controller is available).
    pub cpu: Option<CgroupCpuInfo>,
    /// Memory metrics (if memory controller is available).
    pub memory: Option<CgroupMemoryInfo>,
    /// PIDs metrics (if pids controller is available).
    pub pids: Option<CgroupPidsInfo>,

    /// I/O metrics (if io controller is available).
    ///
    /// This is a list of devices present in `io.stat`.
    ///
    /// Note: `#[serde(default)]` keeps backward compatibility when loading
    /// older snapshots that were stored without this field.
    #[serde(default)]
    pub io: Vec<CgroupIoInfo>,
}
