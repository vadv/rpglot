//! History data provider with lazy snapshot loading.
//!
//! Only chunk metadata (timestamps, counts, file paths) and the merged StringInterner
//! are kept in RAM permanently. Snapshot data is loaded on demand from .zst chunk files
//! through an LRU cache, keeping memory usage bounded regardless of history size.

use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use tracing::warn;

use crate::storage::model::Snapshot;
use crate::storage::{Chunk, StorageManager, StringInterner};

use super::{ProviderError, SnapshotProvider};

// ============================================================
// ChunkMeta — metadata about one chunk file on disk
// ============================================================

struct ChunkMeta {
    path: PathBuf,
    snapshot_count: usize,
    /// Global offset of the first snapshot in this chunk.
    global_offset: usize,
    /// false if the file was deleted from disk (rotation).
    available: bool,
}

// ============================================================
// WAL — lazy loading from file or in-memory (tests)
// ============================================================

/// Metadata about one WAL entry (byte position in the WAL file).
struct WalEntryMeta {
    byte_offset: u64,
    byte_length: u64,
}

/// WAL snapshot source — lazy file-based or in-memory (for tests).
enum WalSource {
    /// Lazy: load individual snapshots from WAL file by byte offset.
    File {
        path: PathBuf,
        entries: Vec<WalEntryMeta>,
    },
    /// In-memory: for tests that don't use the filesystem.
    InMemory { snapshots: Vec<Snapshot> },
}

struct WalIndex {
    source: WalSource,
    global_offset: usize,
}

impl WalIndex {
    fn len(&self) -> usize {
        match &self.source {
            WalSource::File { entries, .. } => entries.len(),
            WalSource::InMemory { snapshots } => snapshots.len(),
        }
    }

    fn load_snapshot(&self, idx: usize) -> Option<Snapshot> {
        match &self.source {
            WalSource::File { path, entries } => {
                let entry = entries.get(idx)?;
                StorageManager::load_wal_snapshot_at(path, entry.byte_offset, entry.byte_length)
                    .ok()
            }
            WalSource::InMemory { snapshots } => snapshots.get(idx).cloned(),
        }
    }
}

// ============================================================
// ChunkCache — LRU cache of reconstructed snapshots
// ============================================================

struct CachedChunk {
    snapshots: Vec<Snapshot>,
    last_accessed: Instant,
}

struct ChunkCache {
    cache: HashMap<usize, CachedChunk>, // chunk_index → cached data
    max_chunks: usize,
}

impl ChunkCache {
    fn new(max_chunks: usize) -> Self {
        Self {
            cache: HashMap::new(),
            max_chunks,
        }
    }

    /// Get a snapshot from the cache, loading the chunk from disk if needed.
    fn get_snapshot(
        &mut self,
        chunk_idx: usize,
        offset_in_chunk: usize,
        meta: &ChunkMeta,
    ) -> Result<&Snapshot, ProviderError> {
        // Load chunk if not cached
        if !self.cache.contains_key(&chunk_idx) {
            if !meta.available {
                return Err(ProviderError::Io(format!(
                    "Chunk file no longer available: {}",
                    meta.path.display()
                )));
            }
            self.load_chunk(chunk_idx, meta)?;
        }

        let cached = self.cache.get_mut(&chunk_idx).unwrap();
        cached.last_accessed = Instant::now();

        cached.snapshots.get(offset_in_chunk).ok_or_else(|| {
            ProviderError::Io(format!(
                "Offset {} out of range for chunk {} (has {} snapshots)",
                offset_in_chunk,
                meta.path.display(),
                cached.snapshots.len()
            ))
        })
    }

    fn load_chunk(&mut self, chunk_idx: usize, meta: &ChunkMeta) -> Result<(), ProviderError> {
        // Evict if at capacity
        while self.cache.len() >= self.max_chunks {
            self.evict_oldest();
        }

        let data = std::fs::read(&meta.path).map_err(|e| {
            ProviderError::Io(format!(
                "Failed to read chunk {}: {}",
                meta.path.display(),
                e
            ))
        })?;

        let chunk = Chunk::decompress(&data).map_err(|e| {
            ProviderError::Io(format!(
                "Failed to decompress chunk {}: {}",
                meta.path.display(),
                e
            ))
        })?;

        let snapshots = StorageManager::reconstruct_snapshots_from_chunk(&chunk).map_err(|e| {
            ProviderError::Io(format!(
                "Failed to reconstruct chunk {}: {}",
                meta.path.display(),
                e
            ))
        })?;

        self.cache.insert(
            chunk_idx,
            CachedChunk {
                snapshots,
                last_accessed: Instant::now(),
            },
        );

        Ok(())
    }

    fn evict_oldest(&mut self) {
        if let Some((&oldest_key, _)) = self.cache.iter().min_by_key(|(_, v)| v.last_accessed) {
            self.cache.remove(&oldest_key);
        }
    }
}

// ============================================================
// HistoryProvider — lazy loading with LRU chunk cache
// ============================================================

/// Provider for historical data from storage files.
///
/// Only metadata and the merged StringInterner are kept in RAM permanently.
/// Snapshot data is loaded on demand from .zst chunk files through an LRU cache.
pub struct HistoryProvider {
    /// Metadata for each chunk file, sorted by timestamp.
    chunks: Vec<ChunkMeta>,
    /// WAL (unflushed) snapshots — lazy loaded from file.
    wal: Option<WalIndex>,
    /// Merged interner from all chunks + WAL.
    interner: StringInterner,
    /// LRU cache of reconstructed chunk snapshots.
    cache: ChunkCache,
    /// Current cursor position (global, 0-based).
    cursor: usize,
    /// Total number of snapshots across all chunks + WAL.
    total_snapshots: usize,
    /// Sorted list of all snapshot timestamps for binary search.
    timestamps: Vec<i64>,
    /// Internal snapshot buffer (for SnapshotProvider::current() → &Snapshot).
    current_buffer: Option<Snapshot>,

    last_error: Option<ProviderError>,
}

impl HistoryProvider {
    /// Creates a new history provider by scanning chunk files at the given path.
    ///
    /// Only metadata and the merged interner are loaded into RAM.
    /// Snapshot data is loaded lazily on demand.
    pub fn from_path(storage_path: impl AsRef<Path>) -> Result<Self, ProviderError> {
        let (chunks, wal, interner, total, timestamps) = Self::build_index(storage_path.as_ref())?;

        if total == 0 {
            return Err(ProviderError::Io(
                "No snapshots found in storage".to_string(),
            ));
        }

        let mut provider = Self {
            chunks,
            wal,
            interner,
            cache: ChunkCache::new(2),
            cursor: 0,
            total_snapshots: total,
            timestamps,
            current_buffer: None,
            last_error: None,
        };

        // Load first snapshot into buffer
        provider.load_into_buffer(0);

        Ok(provider)
    }

    /// Creates a new history provider starting from the specified timestamp.
    pub fn from_path_since(
        storage_path: impl AsRef<Path>,
        since_timestamp: i64,
    ) -> Result<Self, ProviderError> {
        let mut provider = Self::from_path(storage_path)?;

        // Find the first snapshot with timestamp >= since_timestamp
        let start_idx = provider
            .timestamps
            .partition_point(|&ts| ts < since_timestamp);

        if start_idx < provider.total_snapshots {
            provider.cursor = start_idx;
            provider.load_into_buffer(start_idx);
        }

        Ok(provider)
    }

    /// Creates a history provider from a pre-loaded vector of snapshots.
    ///
    /// Useful for testing. All snapshots are stored as WAL data (in-memory).
    pub fn from_snapshots(snapshots: Vec<Snapshot>) -> Result<Self, ProviderError> {
        if snapshots.is_empty() {
            return Err(ProviderError::Io(
                "Cannot create provider with empty snapshots".to_string(),
            ));
        }

        let total = snapshots.len();
        let timestamps: Vec<i64> = snapshots.iter().map(|s| s.timestamp).collect();
        let first_snapshot = snapshots[0].clone();

        let wal = WalIndex {
            source: WalSource::InMemory { snapshots },
            global_offset: 0,
        };

        Ok(Self {
            chunks: Vec::new(),
            wal: Some(wal),
            interner: StringInterner::new(),
            cache: ChunkCache::new(2),
            cursor: 0,
            total_snapshots: total,
            timestamps,
            current_buffer: Some(first_snapshot),
            last_error: None,
        })
    }

    /// Build the chunk index by scanning .zst files and WAL.
    ///
    /// For each chunk file: decompress, extract metadata (timestamps, count),
    /// merge interner, then drop the chunk data. Peak RAM = one chunk at a time.
    fn build_index(
        storage_path: &Path,
    ) -> Result<
        (
            Vec<ChunkMeta>,
            Option<WalIndex>,
            StringInterner,
            usize,
            Vec<i64>,
        ),
        ProviderError,
    > {
        let mut chunk_paths: Vec<PathBuf> = Vec::new();

        // Collect .zst file paths
        let entries = std::fs::read_dir(storage_path)
            .map_err(|e| ProviderError::Io(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| ProviderError::Io(format!("Failed to read entry: {}", e)))?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "zst") {
                chunk_paths.push(path);
            }
        }

        // Sort by filename (chronological order: rpglot_YYYY-MM-DD_HH.zst)
        chunk_paths.sort();

        let mut chunks: Vec<ChunkMeta> = Vec::with_capacity(chunk_paths.len());
        let mut merged_interner = StringInterner::new();
        let mut all_timestamps: Vec<i64> = Vec::new();
        let mut global_offset: usize = 0;

        // Scan each chunk file: load → extract metadata → merge interner → drop
        for path in chunk_paths {
            let data = std::fs::read(&path).map_err(|e| {
                ProviderError::Io(format!("Failed to read chunk {}: {}", path.display(), e))
            })?;

            let chunk = Chunk::decompress(&data).map_err(|e| {
                ProviderError::Io(format!(
                    "Failed to decompress chunk {}: {}",
                    path.display(),
                    e
                ))
            })?;

            // Extract metadata from deltas without reconstructing snapshots
            let snapshot_count = chunk.deltas.len();
            if snapshot_count == 0 {
                continue;
            }

            // Collect all timestamps from this chunk
            for delta in &chunk.deltas {
                all_timestamps.push(delta.timestamp());
            }

            // Merge interner
            merged_interner.merge(&chunk.interner);

            chunks.push(ChunkMeta {
                path,
                snapshot_count,
                global_offset,
                available: true,
            });

            global_offset += snapshot_count;
            // chunk is dropped here — only metadata + interner strings survive
        }

        // Scan WAL metadata lazily — snapshots are NOT kept in memory
        let wal = {
            let wal_path = storage_path.join("wal.log");
            let (wal_entries, wal_interner) = StorageManager::scan_wal_metadata(&wal_path)
                .map_err(|e| ProviderError::Io(format!("Failed to scan WAL: {}", e)))?;

            merged_interner.merge(&wal_interner);

            if wal_entries.is_empty() {
                None
            } else {
                let entries: Vec<WalEntryMeta> = wal_entries
                    .into_iter()
                    .map(|(offset, length, ts)| {
                        all_timestamps.push(ts);
                        WalEntryMeta {
                            byte_offset: offset,
                            byte_length: length,
                        }
                    })
                    .collect();
                let count = entries.len();
                let wal_index = WalIndex {
                    source: WalSource::File {
                        path: wal_path,
                        entries,
                    },
                    global_offset,
                };
                global_offset += count;
                Some(wal_index)
            }
        };

        let total = global_offset;

        // Sort and dedup timestamps
        all_timestamps.sort();
        all_timestamps.dedup();

        Ok((chunks, wal, merged_interner, total, all_timestamps))
    }

    /// Resolve a global position to (chunk_index, offset_in_chunk) or WAL index.
    fn resolve_position(&self, position: usize) -> Option<SnapshotLocation> {
        if position >= self.total_snapshots {
            return None;
        }

        // Check WAL first (fast path for latest snapshots)
        if let Some(ref wal) = self.wal {
            if position >= wal.global_offset {
                let wal_idx = position - wal.global_offset;
                if wal_idx < wal.len() {
                    return Some(SnapshotLocation::Wal(wal_idx));
                }
                return None;
            }
        }

        // Binary search in chunks by global_offset
        let chunk_idx = self
            .chunks
            .partition_point(|c| c.global_offset <= position)
            .saturating_sub(1);

        if chunk_idx < self.chunks.len() {
            let chunk = &self.chunks[chunk_idx];
            let offset_in_chunk = position - chunk.global_offset;
            if offset_in_chunk < chunk.snapshot_count {
                return Some(SnapshotLocation::Chunk {
                    chunk_idx,
                    offset_in_chunk,
                });
            }
        }

        None
    }

    /// Load snapshot at global position into the internal buffer.
    fn load_into_buffer(&mut self, position: usize) {
        let snapshot = match self.resolve_position(position) {
            Some(SnapshotLocation::Wal(wal_idx)) => {
                self.wal.as_ref().and_then(|w| w.load_snapshot(wal_idx))
            }
            Some(SnapshotLocation::Chunk {
                chunk_idx,
                offset_in_chunk,
            }) => {
                let meta = &self.chunks[chunk_idx];
                match self.cache.get_snapshot(chunk_idx, offset_in_chunk, meta) {
                    Ok(s) => Some(s.clone()),
                    Err(e) => {
                        // File may have been deleted (rotation)
                        warn!(error = %e, position, "failed to load snapshot");
                        // Mark chunk as unavailable
                        self.chunks[chunk_idx].available = false;
                        None
                    }
                }
            }
            None => None,
        };

        self.current_buffer = snapshot;
    }

    /// Get a cloned snapshot at the given position (for external use).
    fn snapshot_cloned(&mut self, position: usize) -> Option<Snapshot> {
        match self.resolve_position(position) {
            Some(SnapshotLocation::Wal(wal_idx)) => {
                self.wal.as_ref().and_then(|w| w.load_snapshot(wal_idx))
            }
            Some(SnapshotLocation::Chunk {
                chunk_idx,
                offset_in_chunk,
            }) => {
                let meta = &self.chunks[chunk_idx];
                match self.cache.get_snapshot(chunk_idx, offset_in_chunk, meta) {
                    Ok(s) => Some(s.clone()),
                    Err(e) => {
                        warn!(error = %e, position, "failed to load snapshot");
                        self.chunks[chunk_idx].available = false;
                        None
                    }
                }
            }
            None => None,
        }
    }

    // ========== Public API ==========

    /// Returns the total number of snapshots available.
    pub fn len(&self) -> usize {
        self.total_snapshots
    }

    /// Returns true if there are no snapshots.
    pub fn is_empty(&self) -> bool {
        self.total_snapshots == 0
    }

    /// Returns the current cursor position (0-indexed).
    pub fn position(&self) -> usize {
        self.cursor
    }

    /// Returns position info as (current, total) for UI display.
    /// Position is 1-indexed for user-friendly display.
    pub fn position_info(&self) -> (usize, usize) {
        (self.cursor + 1, self.total_snapshots)
    }

    /// Jumps to a specific position in the history.
    ///
    /// Returns the snapshot at that position, or `None` if the position is out of bounds.
    pub fn jump_to(&mut self, position: usize) -> Option<&Snapshot> {
        if position >= self.total_snapshots {
            return None;
        }
        self.cursor = position;
        self.load_into_buffer(position);
        self.current_buffer.as_ref()
    }

    /// Returns a cloned snapshot at the specified position without changing the cursor.
    ///
    /// Note: unlike the old API that returned `Option<&Snapshot>`, this returns
    /// an owned clone because the cache may evict data at any time.
    pub fn snapshot_at(&mut self, position: usize) -> Option<Snapshot> {
        self.snapshot_cloned(position)
    }

    /// Refreshes snapshot metadata from disk, discovering new chunk files and WAL entries.
    ///
    /// Returns the number of newly discovered snapshots.
    pub fn refresh(&mut self, storage_path: impl AsRef<Path>) -> Result<usize, ProviderError> {
        let storage_path = storage_path.as_ref();

        let old_total = self.total_snapshots;

        // Re-scan chunk files
        let mut chunk_paths: Vec<PathBuf> = Vec::new();
        let entries = std::fs::read_dir(storage_path)
            .map_err(|e| ProviderError::Io(format!("Failed to read directory: {}", e)))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "zst") {
                chunk_paths.push(path);
            }
        }
        chunk_paths.sort();

        // Find new chunk files (not in current index)
        let known_paths: std::collections::HashSet<PathBuf> =
            self.chunks.iter().map(|c| c.path.clone()).collect();

        let mut global_offset = self
            .chunks
            .last()
            .map(|c| c.global_offset + c.snapshot_count)
            .unwrap_or(0);

        let mut new_timestamps: Vec<i64> = Vec::new();

        for path in &chunk_paths {
            if known_paths.contains(path) {
                continue;
            }

            // New chunk file discovered
            let data = match std::fs::read(path) {
                Ok(d) => d,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "failed to read new chunk");
                    continue;
                }
            };

            let chunk = match Chunk::decompress(&data) {
                Ok(c) => c,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "failed to decompress new chunk");
                    continue;
                }
            };

            let snapshot_count = chunk.deltas.len();
            if snapshot_count == 0 {
                continue;
            }

            for delta in &chunk.deltas {
                new_timestamps.push(delta.timestamp());
            }

            self.interner.merge(&chunk.interner);

            self.chunks.push(ChunkMeta {
                path: path.clone(),
                snapshot_count,
                global_offset,
                available: true,
            });

            global_offset += snapshot_count;
        }

        // Also check for deleted chunks — mark as unavailable
        for chunk in &mut self.chunks {
            if chunk.available && !chunk.path.exists() {
                chunk.available = false;
                // Invalidate cache for this chunk
                // (chunk_idx is position in self.chunks — find it)
            }
        }

        // Reload WAL metadata lazily
        let wal_path = storage_path.join("wal.log");
        let (wal_entries, wal_interner) = StorageManager::scan_wal_metadata(&wal_path)
            .map_err(|e| ProviderError::Io(format!("Failed to scan WAL: {}", e)))?;

        self.interner.merge(&wal_interner);

        if wal_entries.is_empty() {
            self.wal = None;
        } else {
            let entries: Vec<WalEntryMeta> = wal_entries
                .into_iter()
                .map(|(offset, length, ts)| {
                    new_timestamps.push(ts);
                    WalEntryMeta {
                        byte_offset: offset,
                        byte_length: length,
                    }
                })
                .collect();
            let count = entries.len();
            self.wal = Some(WalIndex {
                source: WalSource::File {
                    path: wal_path,
                    entries,
                },
                global_offset,
            });
            global_offset += count;
        }

        self.total_snapshots = global_offset;

        // Update timestamps index
        if !new_timestamps.is_empty() {
            self.timestamps.extend(new_timestamps);
            self.timestamps.sort();
            self.timestamps.dedup();
        }

        Ok(self.total_snapshots - old_total)
    }

    /// Returns the sorted list of all snapshot timestamps.
    /// Useful for building date indices without loading snapshot data.
    pub fn timestamps(&self) -> &[i64] {
        &self.timestamps
    }

    /// Returns the timestamp range as (first, last).
    pub fn timestamp_range(&self) -> (i64, i64) {
        let first = self.timestamps.first().copied().unwrap_or(0);
        let last = self.timestamps.last().copied().unwrap_or(0);
        (first, last)
    }

    /// Jumps to the latest snapshot with timestamp <= `target_ts`.
    pub fn jump_to_timestamp_floor(&mut self, target_ts: i64) -> Option<&Snapshot> {
        if self.timestamps.is_empty() {
            return None;
        }

        let idx = self
            .timestamps
            .partition_point(|&ts| ts <= target_ts)
            .saturating_sub(1);

        // idx is an index into timestamps, which maps 1:1 to global positions
        // (since timestamps are sorted and deduped, same as snapshot ordering)
        let position = idx.min(self.total_snapshots.saturating_sub(1));

        self.cursor = position;
        self.load_into_buffer(position);
        self.current_buffer.as_ref()
    }
}

enum SnapshotLocation {
    Chunk {
        chunk_idx: usize,
        offset_in_chunk: usize,
    },
    Wal(usize),
}

impl SnapshotProvider for HistoryProvider {
    fn current(&self) -> Option<&Snapshot> {
        self.current_buffer.as_ref()
    }

    fn advance(&mut self) -> Option<&Snapshot> {
        self.last_error = None;

        if self.cursor + 1 < self.total_snapshots {
            self.cursor += 1;
            self.load_into_buffer(self.cursor);
        }
        // If at end, keep current buffer
        self.current_buffer.as_ref()
    }

    fn rewind(&mut self) -> Option<&Snapshot> {
        self.last_error = None;

        if self.cursor > 0 {
            self.cursor -= 1;
            self.load_into_buffer(self.cursor);
        }
        // If at start, keep current buffer
        self.current_buffer.as_ref()
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
