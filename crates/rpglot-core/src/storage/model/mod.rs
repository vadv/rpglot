//! Data models for the storage system.
//!
//! This module contains all data structures used for storing metrics:
//!
//! - [`process`]: Per-process metrics from `/proc/[pid]/`
//! - [`postgres`]: PostgreSQL database metrics from system views
//! - [`system`]: System-wide metrics from `/proc/` filesystem
//! - [`snapshot`]: Storage structures (Snapshot, Delta, DataBlock)
//!
//! # Architecture
//!
//! The storage system uses a hierarchical approach:
//!
//! ```text
//! Chunk (compressed file on disk)
//!   └── Delta[]
//!         ├── Full(Snapshot)     <- first in chunk
//!         └── Diff { blocks[] }  <- subsequent entries
//!               └── DataBlockDiff
//!                     ├── updates: Vec<T>
//!                     └── removals: Vec<ID>
//! ```
//!
//! This delta-encoding approach significantly reduces storage size
//! by only storing changes between snapshots.

mod cgroup;
mod postgres;
mod process;
mod snapshot;
mod system;

// Re-export all public types for convenient access
pub use cgroup::{CgroupCpuInfo, CgroupInfo, CgroupIoInfo, CgroupMemoryInfo, CgroupPidsInfo};
pub use postgres::{
    PgLockTreeNode, PgStatActivityInfo, PgStatBgwriterInfo, PgStatDatabaseInfo,
    PgStatStatementsInfo, PgStatUserIndexesInfo, PgStatUserTablesInfo,
};
#[allow(unused_imports)]
pub use process::{ProcessCpuInfo, ProcessDskInfo, ProcessInfo, ProcessMemInfo};
pub use snapshot::{DataBlock, DataBlockDiff, Delta, Snapshot};
pub use system::{
    SystemCpuInfo, SystemDiskInfo, SystemFileInfo, SystemInterruptInfo, SystemLoadInfo,
    SystemMemInfo, SystemNetInfo, SystemNetSnmpInfo, SystemPsiInfo, SystemSoftirqInfo,
    SystemStatInfo, SystemVmstatInfo,
};
