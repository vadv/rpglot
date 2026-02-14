//! Mock filesystem implementations for testing.
//!
//! This module provides `MockFs` and pre-built scenarios for testing
//! collectors without requiring actual Linux `/proc` filesystem access.

mod filesystem;
mod scenarios;

pub use filesystem::MockFs;
