//! Cgroup v2 metrics collection.
//!
//! This module provides collection of container resource limits and usage
//! from the Linux cgroup v2 filesystem.

mod collector;
mod parser;

pub use collector::CgroupCollector;
