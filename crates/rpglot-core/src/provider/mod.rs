//! Provider abstraction for snapshot data sources.
//!
//! This module defines the `SnapshotProvider` trait that allows TUI to work
//! with different data sources (live collection or historical data) through
//! a unified interface.

mod history;
mod live;

pub use history::HistoryProvider;
pub use live::LiveProvider;

use std::any::Any;

use crate::collector::{CollectorTiming, UserResolver};
use crate::storage::StringInterner;
use crate::storage::model::Snapshot;

/// Error types that can occur during snapshot operations.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ProviderError {
    /// I/O error while reading/writing data.
    Io(String),
    /// Error during data collection.
    Collection(String),
    /// Error parsing stored data.
    Parse(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Io(msg) => write!(f, "I/O error: {}", msg),
            ProviderError::Collection(msg) => write!(f, "Collection error: {}", msg),
            ProviderError::Parse(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Abstraction for snapshot data sources.
///
/// This trait allows TUI to work with different data sources:
/// - `LiveProvider`: Real-time data collection from the system
/// - `HistoryProvider`: Historical data from storage files
///
/// The trait is object-safe and designed to be used with `Box<dyn SnapshotProvider>`.
pub trait SnapshotProvider {
    /// Returns the current snapshot, if available.
    ///
    /// Returns `None` if no snapshot has been loaded yet.
    fn current(&self) -> Option<&Snapshot>;

    /// Advances to the next snapshot.
    ///
    /// - In live mode: collects a new snapshot from the system
    /// - In history mode: moves cursor forward in time
    ///
    /// Returns `None` if no more data is available (end of history)
    /// or if collection failed (check `last_error()` for details).
    fn advance(&mut self) -> Option<&Snapshot>;

    /// Moves to the previous snapshot.
    ///
    /// - In live mode: not supported, returns `None`
    /// - In history mode: moves cursor backward in time
    ///
    /// Returns `None` if at the beginning of history or if rewind is not supported.
    fn rewind(&mut self) -> Option<&Snapshot>;

    /// Returns `true` if this provider supports rewinding (going back in time).
    ///
    /// Live providers return `false`, history providers return `true`.
    fn can_rewind(&self) -> bool;

    /// Returns `true` if this provider is collecting live data.
    fn is_live(&self) -> bool;

    /// Returns the last error that occurred, if any.
    ///
    /// Useful for diagnostics when `advance()` or `rewind()` returns `None`.
    fn last_error(&self) -> Option<&ProviderError>;

    /// Returns self as Any for downcasting.
    fn as_any(&self) -> Option<&dyn Any> {
        None
    }

    /// Returns self as Any for mutable downcasting.
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        None
    }

    /// Returns the string interner for resolving name/cmdline hashes.
    ///
    /// Live providers return the interner from the collector.
    /// History providers may return None if interner data is not available.
    fn interner(&self) -> Option<&StringInterner> {
        None
    }

    /// Returns the user resolver for UID -> username mapping.
    ///
    /// Live providers return the resolver from the collector.
    /// History providers may return None if resolver data is not available.
    fn user_resolver(&self) -> Option<&UserResolver> {
        None
    }

    /// Returns the last PostgreSQL collector error, if any.
    ///
    /// This is used to display error messages in PGA tab when
    /// PostgreSQL data is not available.
    fn pg_last_error(&self) -> Option<&str> {
        None
    }

    /// Returns timing information from the last snapshot collection.
    ///
    /// Only available for live providers. History providers return None.
    fn collector_timing(&self) -> Option<&CollectorTiming> {
        None
    }
}
