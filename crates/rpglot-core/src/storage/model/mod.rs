//! Data models for the storage system.
//!
//! This module contains all data structures used for storing metrics:
//!
//! - [`process`]: Per-process metrics from `/proc/[pid]/`
//! - [`postgres`]: PostgreSQL database metrics from system views
//! - [`system`]: System-wide metrics from `/proc/` filesystem
//! - [`snapshot`]: Storage structures (Snapshot, DataBlock)
//!
//! # Architecture
//!
//! Each snapshot is stored as an independent zstd frame within a chunk file,
//! enabling O(1) random access. String interning eliminates duplicate string storage.

mod cgroup;
mod postgres;
mod process;
mod snapshot;
mod system;

// Re-export all public types for convenient access
pub use cgroup::{CgroupCpuInfo, CgroupInfo, CgroupIoInfo, CgroupMemoryInfo, CgroupPidsInfo};
pub use postgres::{
    ErrorCategory, PgLockTreeNode, PgLogEntry, PgLogEventEntry, PgLogEventType, PgLogEventsInfo,
    PgLogSeverity, PgSettingEntry, PgStatActivityInfo, PgStatBgwriterInfo, PgStatDatabaseInfo,
    PgStatStatementsInfo, PgStatUserIndexesInfo, PgStatUserTablesInfo,
};
#[allow(unused_imports)]
pub use process::{ProcessCpuInfo, ProcessDskInfo, ProcessInfo, ProcessMemInfo};
pub use snapshot::{DataBlock, Snapshot};
pub use system::{
    SystemCpuInfo, SystemDiskInfo, SystemFileInfo, SystemInterruptInfo, SystemLoadInfo,
    SystemMemInfo, SystemNetInfo, SystemNetSnmpInfo, SystemPsiInfo, SystemSoftirqInfo,
    SystemStatInfo, SystemVmstatInfo,
};
