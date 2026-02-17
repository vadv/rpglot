//! Live data provider for real-time system monitoring.

use crate::collector::traits::FileSystem;
use crate::collector::{Collector, CollectorTiming, UserResolver};
use crate::storage::StorageManager;
use crate::storage::StringInterner;
use crate::storage::model::Snapshot;

use super::{ProviderError, SnapshotProvider};
use std::any::Any;

/// Provider for real-time system data collection.
///
/// Collects snapshots from the system using the `Collector` and optionally
/// writes them to storage for later analysis.
pub struct LiveProvider<F: FileSystem + Clone> {
    collector: Collector<F>,
    storage: Option<StorageManager>,
    current: Option<Snapshot>,
    last_error: Option<ProviderError>,
}

impl<F: FileSystem + Clone> LiveProvider<F> {
    /// Creates a new live provider.
    ///
    /// # Arguments
    /// * `collector` - The collector to use for gathering system metrics
    /// * `storage` - Optional storage manager for recording snapshots
    pub fn new(collector: Collector<F>, storage: Option<StorageManager>) -> Self {
        Self {
            collector,
            storage,
            current: None,
            last_error: None,
        }
    }
}

impl<F: FileSystem + Clone + 'static> SnapshotProvider for LiveProvider<F> {
    fn current(&self) -> Option<&Snapshot> {
        self.current.as_ref()
    }

    fn advance(&mut self) -> Option<&Snapshot> {
        self.last_error = None;

        match self.collector.collect_snapshot() {
            Ok(snapshot) => {
                // Optionally save to storage
                if let Some(storage) = &mut self.storage {
                    storage.add_snapshot(snapshot.clone(), self.collector.interner());
                }
                self.current = Some(snapshot);
                self.current.as_ref()
            }
            Err(e) => {
                self.last_error = Some(ProviderError::Collection(e.to_string()));
                None
            }
        }
    }

    fn rewind(&mut self) -> Option<&Snapshot> {
        // Live provider does not support rewinding
        None
    }

    fn can_rewind(&self) -> bool {
        false
    }

    fn is_live(&self) -> bool {
        true
    }

    fn last_error(&self) -> Option<&ProviderError> {
        self.last_error.as_ref()
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }

    fn interner(&self) -> Option<&StringInterner> {
        Some(self.collector.interner())
    }

    fn user_resolver(&self) -> Option<&UserResolver> {
        Some(self.collector.user_resolver())
    }

    fn pg_last_error(&self) -> Option<&str> {
        self.collector.pg_last_error()
    }

    fn collector_timing(&self) -> Option<&CollectorTiming> {
        self.collector.last_timing()
    }

    fn instance_info(&self) -> Option<(String, String)> {
        self.collector.instance_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::mock::MockFs;

    #[test]
    fn test_live_provider_advance() {
        let fs = MockFs::typical_system();
        let collector = Collector::new(fs, "/proc");
        let mut provider = LiveProvider::new(collector, None);

        // Initially no snapshot
        assert!(provider.current().is_none());

        // Advance should collect a snapshot
        let snapshot = provider.advance();
        assert!(snapshot.is_some());

        // Current should now return the snapshot
        assert!(provider.current().is_some());
    }

    #[test]
    fn test_live_provider_cannot_rewind() {
        let fs = MockFs::typical_system();
        let collector = Collector::new(fs, "/proc");
        let mut provider = LiveProvider::new(collector, None);

        assert!(!provider.can_rewind());
        assert!(provider.rewind().is_none());
    }

    #[test]
    fn test_live_provider_is_live() {
        let fs = MockFs::typical_system();
        let collector = Collector::new(fs, "/proc");
        let provider = LiveProvider::new(collector, None);

        assert!(provider.is_live());
    }
}
