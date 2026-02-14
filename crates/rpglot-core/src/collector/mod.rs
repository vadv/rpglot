//! System metrics collector for Linux.
//!
//! This module provides infrastructure for collecting system and process metrics
//! from the Linux `/proc` filesystem, with support for mocking for testing on macOS.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         Collector                           │
//! │  ┌─────────────────────┐   ┌─────────────────────────────┐  │
//! │  │  ProcessCollector   │   │     SystemCollector         │  │
//! │  │  - /proc/[pid]/*    │   │  - /proc/meminfo            │  │
//! │  │  - StringInterner   │   │  - /proc/stat               │  │
//! │  └──────────┬──────────┘   │  - /proc/loadavg            │  │
//! │             │              └──────────────┬──────────────┘  │
//! │             └──────────────┬──────────────┘                 │
//! │                            │                                │
//! │                     ┌──────▼──────┐                         │
//! │                     │  FileSystem │ (trait)                 │
//! │                     └──────┬──────┘                         │
//! └────────────────────────────┼────────────────────────────────┘
//!                              │
//!              ┌───────────────┼───────────────┐
//!              │               │               │
//!       ┌──────▼──────┐ ┌──────▼──────┐ ┌──────▼──────┐
//!       │   RealFs    │ │   MockFs    │ │  Scenarios  │
//!       │ (Linux)     │ │ (Testing)   │ │ (Fixtures)  │
//!       └─────────────┘ └─────────────┘ └─────────────┘
//! ```
//!
//! # Usage
//!
//! ## Production (Linux)
//!
//! ```ignore
//! use rpglot_core::collector::{Collector, RealFs};
//!
//! let fs = RealFs::new();
//! let mut collector = Collector::new(fs, "/proc");
//! let snapshot = collector.collect_snapshot().unwrap();
//! ```
//!
//! ## Testing (with MockFs)
//!
//! ```
//! use rpglot_core::collector::{Collector, MockFs};
//!
//! let fs = MockFs::typical_system();
//! let mut collector = Collector::new(fs, "/proc");
//! let snapshot = collector.collect_snapshot().unwrap();
//! assert!(!snapshot.blocks.is_empty());
//! ```

pub mod cgroup;
#[allow(clippy::module_inception)]
mod collector;
pub mod mock;
mod pg_collector;
pub mod procfs;
pub mod traits;

// Re-exports for public API (will be used by consumers of this library)
#[allow(unused_imports)]
pub use cgroup::CgroupCollector;
#[allow(unused_imports)]
pub use collector::{Collector, CollectorTiming};
#[allow(unused_imports)]
pub use mock::MockFs;
#[allow(unused_imports)]
pub use pg_collector::{PgCollectError, PostgresCollector};
#[allow(unused_imports)]
pub use procfs::CollectError;
#[allow(unused_imports)]
pub use procfs::UserResolver;
#[allow(unused_imports)]
pub use traits::{FileSystem, RealFs};
