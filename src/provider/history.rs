//! History data provider for viewing stored snapshots.

use std::any::Any;
use std::path::Path;

use crate::storage::StorageManager;
use crate::storage::StringInterner;
use crate::storage::model::Snapshot;

use super::{ProviderError, SnapshotProvider};

/// Provider for historical data from storage files.
///
/// Loads snapshots from stored chunk files and provides navigation
/// through the history (forward and backward in time).
#[allow(dead_code)]
pub struct HistoryProvider {
    snapshots: Vec<Snapshot>,
    cursor: usize,
    last_error: Option<ProviderError>,
    interner: StringInterner,
}

#[allow(dead_code)]
impl HistoryProvider {
    /// Creates a new history provider by loading snapshots from the given storage path.
    ///
    /// # Arguments
    /// * `storage_path` - Path to the directory containing chunk files
    ///
    /// # Returns
    /// A new `HistoryProvider` or an error if loading fails.
    pub fn from_path(storage_path: impl AsRef<Path>) -> Result<Self, ProviderError> {
        let manager = StorageManager::new(storage_path.as_ref());
        let (snapshots, interner): (Vec<Snapshot>, StringInterner) = manager
            .load_all_snapshots_with_interner()
            .map_err(|e| ProviderError::Io(e.to_string()))?;

        if snapshots.is_empty() {
            return Err(ProviderError::Io(
                "No snapshots found in storage".to_string(),
            ));
        }

        Ok(Self {
            snapshots,
            cursor: 0,
            last_error: None,
            interner,
        })
    }

    /// Creates a new history provider starting from the specified timestamp.
    ///
    /// # Arguments
    /// * `storage_path` - Path to the directory containing chunk files
    /// * `since_timestamp` - Unix timestamp to start from (snapshots before this are skipped)
    ///
    /// # Returns
    /// A new `HistoryProvider` positioned at the first snapshot >= since_timestamp,
    /// or an error if no matching snapshots found.
    pub fn from_path_since(
        storage_path: impl AsRef<Path>,
        since_timestamp: i64,
    ) -> Result<Self, ProviderError> {
        let manager = StorageManager::new(storage_path.as_ref());
        let (all_snapshots, interner): (Vec<Snapshot>, StringInterner) = manager
            .load_all_snapshots_with_interner()
            .map_err(|e| ProviderError::Io(e.to_string()))?;

        if all_snapshots.is_empty() {
            return Err(ProviderError::Io(
                "No snapshots found in storage".to_string(),
            ));
        }

        // Find the first snapshot with timestamp >= since_timestamp
        let start_idx = all_snapshots
            .iter()
            .position(|s| s.timestamp >= since_timestamp)
            .unwrap_or(0);

        // Keep all snapshots from start_idx onwards
        let snapshots: Vec<_> = all_snapshots.into_iter().skip(start_idx).collect();

        if snapshots.is_empty() {
            return Err(ProviderError::Io(format!(
                "No snapshots found after timestamp {}",
                since_timestamp
            )));
        }

        Ok(Self {
            snapshots,
            cursor: 0,
            last_error: None,
            interner,
        })
    }

    /// Creates a history provider from a pre-loaded vector of snapshots.
    ///
    /// Useful for testing or when snapshots are loaded from a different source.
    pub fn from_snapshots(snapshots: Vec<Snapshot>) -> Result<Self, ProviderError> {
        if snapshots.is_empty() {
            return Err(ProviderError::Io(
                "Cannot create provider with empty snapshots".to_string(),
            ));
        }

        Ok(Self {
            snapshots,
            cursor: 0,
            last_error: None,
            interner: StringInterner::new(),
        })
    }

    /// Returns the total number of snapshots available.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns true if there are no snapshots.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Returns the current cursor position (0-indexed).
    pub fn position(&self) -> usize {
        self.cursor
    }

    /// Returns position info as (current, total) for UI display.
    /// Position is 1-indexed for user-friendly display.
    pub fn position_info(&self) -> (usize, usize) {
        (self.cursor + 1, self.snapshots.len())
    }

    /// Jumps to a specific position in the history.
    ///
    /// Returns the snapshot at that position, or `None` if the position is out of bounds.
    pub fn jump_to(&mut self, position: usize) -> Option<&Snapshot> {
        if position < self.snapshots.len() {
            self.cursor = position;
            Some(&self.snapshots[self.cursor])
        } else {
            None
        }
    }

    /// Returns snapshot at the specified position without changing the cursor.
    pub fn snapshot_at(&self, position: usize) -> Option<&Snapshot> {
        self.snapshots.get(position)
    }

    /// Jumps to the latest snapshot with timestamp <= `target_ts`.
    ///
    /// If `target_ts` is earlier than the first snapshot, jumps to the first snapshot.
    /// If `target_ts` is later than the last snapshot, jumps to the last snapshot.
    pub fn jump_to_timestamp_floor(&mut self, target_ts: i64) -> Option<&Snapshot> {
        if self.snapshots.is_empty() {
            return None;
        }

        // `partition_point` returns the index of the first element for which the predicate is false.
        // With `<= target_ts`, it effectively gives the count of elements <= target_ts.
        let idx = self
            .snapshots
            .partition_point(|s| s.timestamp <= target_ts)
            .saturating_sub(1);

        self.cursor = idx;
        self.snapshots.get(self.cursor)
    }
}

impl SnapshotProvider for HistoryProvider {
    fn current(&self) -> Option<&Snapshot> {
        self.snapshots.get(self.cursor)
    }

    fn advance(&mut self) -> Option<&Snapshot> {
        self.last_error = None;

        if self.cursor + 1 < self.snapshots.len() {
            self.cursor += 1;
            Some(&self.snapshots[self.cursor])
        } else {
            // Already at the end
            self.snapshots.get(self.cursor)
        }
    }

    fn rewind(&mut self) -> Option<&Snapshot> {
        self.last_error = None;

        if self.cursor > 0 {
            self.cursor -= 1;
            Some(&self.snapshots[self.cursor])
        } else {
            // Already at the beginning
            self.snapshots.get(self.cursor)
        }
    }

    fn can_rewind(&self) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        false
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
        Some(&self.interner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::{DataBlock, ProcessInfo};

    fn create_test_snapshots() -> Vec<Snapshot> {
        vec![
            Snapshot {
                timestamp: 100,
                blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                    pid: 1,
                    name_hash: 111,
                    ..ProcessInfo::default()
                }])],
            },
            Snapshot {
                timestamp: 110,
                blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                    pid: 1,
                    name_hash: 111,
                    ..ProcessInfo::default()
                }])],
            },
            Snapshot {
                timestamp: 120,
                blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                    pid: 2,
                    name_hash: 222,
                    ..ProcessInfo::default()
                }])],
            },
        ]
    }

    #[test]
    fn test_history_provider_creation() {
        let snapshots = create_test_snapshots();
        let provider = HistoryProvider::from_snapshots(snapshots).unwrap();

        assert_eq!(provider.len(), 3);
        assert!(!provider.is_empty());
        assert_eq!(provider.position(), 0);
    }

    #[test]
    fn test_history_provider_empty_error() {
        let result = HistoryProvider::from_snapshots(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_history_provider_navigation() {
        let snapshots = create_test_snapshots();
        let mut provider = HistoryProvider::from_snapshots(snapshots).unwrap();

        // Start at position 0
        assert_eq!(provider.current().unwrap().timestamp, 100);

        // Advance to position 1
        let s = provider.advance().unwrap();
        assert_eq!(s.timestamp, 110);
        assert_eq!(provider.position(), 1);

        // Advance to position 2
        let s = provider.advance().unwrap();
        assert_eq!(s.timestamp, 120);
        assert_eq!(provider.position(), 2);

        // Advance at end stays at end
        let s = provider.advance().unwrap();
        assert_eq!(s.timestamp, 120);
        assert_eq!(provider.position(), 2);

        // Rewind to position 1
        let s = provider.rewind().unwrap();
        assert_eq!(s.timestamp, 110);
        assert_eq!(provider.position(), 1);

        // Rewind to position 0
        let s = provider.rewind().unwrap();
        assert_eq!(s.timestamp, 100);
        assert_eq!(provider.position(), 0);

        // Rewind at start stays at start
        let s = provider.rewind().unwrap();
        assert_eq!(s.timestamp, 100);
        assert_eq!(provider.position(), 0);
    }

    #[test]
    fn test_history_provider_jump_to() {
        let snapshots = create_test_snapshots();
        let mut provider = HistoryProvider::from_snapshots(snapshots).unwrap();

        // Jump to position 2
        let s = provider.jump_to(2).unwrap();
        assert_eq!(s.timestamp, 120);
        assert_eq!(provider.position(), 2);

        // Jump to invalid position
        assert!(provider.jump_to(10).is_none());
        // Cursor should not change
        assert_eq!(provider.position(), 2);
    }

    #[test]
    fn test_history_provider_jump_to_timestamp_floor() {
        let snapshots = create_test_snapshots();
        let mut provider = HistoryProvider::from_snapshots(snapshots).unwrap();

        // exact match
        let s = provider.jump_to_timestamp_floor(110).unwrap();
        assert_eq!(s.timestamp, 110);
        assert_eq!(provider.position(), 1);

        // between snapshots -> floor
        let s = provider.jump_to_timestamp_floor(115).unwrap();
        assert_eq!(s.timestamp, 110);
        assert_eq!(provider.position(), 1);

        // before the first -> clamp to first
        let s = provider.jump_to_timestamp_floor(1).unwrap();
        assert_eq!(s.timestamp, 100);
        assert_eq!(provider.position(), 0);

        // after the last -> clamp to last
        let s = provider.jump_to_timestamp_floor(999).unwrap();
        assert_eq!(s.timestamp, 120);
        assert_eq!(provider.position(), 2);
    }

    #[test]
    fn test_history_provider_traits() {
        let snapshots = create_test_snapshots();
        let provider = HistoryProvider::from_snapshots(snapshots).unwrap();

        assert!(provider.can_rewind());
        assert!(!provider.is_live());
        assert!(provider.last_error().is_none());
    }
}
