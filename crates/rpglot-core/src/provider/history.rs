//! History data provider with lazy snapshot loading.
//!
//! Only chunk metadata (timestamps, counts, file paths) is kept in RAM permanently.
//! Snapshot data and per-chunk StringInterners are loaded on demand from disk.
//! A single-chunk cache avoids repeated decompression when multiple snapshots
//! are requested from the same chunk (common during reconvert_current).
//! The cache is explicitly dropped after each batch via `drop_cache()`.

use std::any::Any;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::storage::model::Snapshot;
use crate::storage::{Chunk, ChunkMetadata, StorageManager, StringInterner};

use super::{ProviderError, SnapshotProvider};

// ============================================================
// SingleChunkCache — cache for one decompressed chunk
// ============================================================

/// Cache holding a single decompressed chunk to avoid repeated decompression
/// when multiple snapshots are requested from the same chunk (e.g. during
/// `reconvert_current` which reads current + prev + PGS lookback).
struct SingleChunkCache {
    chunk_idx: usize,
    chunk: Chunk,
}

impl SingleChunkCache {
    /// Get or load the chunk at `chunk_idx`. Returns a reference to the cached Chunk.
    /// If a different chunk was cached, it is dropped first.
    fn get_or_load(
        cache: &mut Option<Self>,
        chunk_idx: usize,
        meta: &ChunkMeta,
    ) -> Result<(), ProviderError> {
        if let Some(c) = cache.as_ref()
            && c.chunk_idx == chunk_idx
        {
            return Ok(());
        }
        // Drop old cache entry (if any) before loading new one
        *cache = None;

        if !meta.available {
            return Err(ProviderError::Io(format!(
                "Chunk file no longer available: {}",
                meta.path.display()
            )));
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
        // `data` (compressed bytes) dropped here

        *cache = Some(Self { chunk_idx, chunk });
        Ok(())
    }
}

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

    fn load_snapshot_with_interner(&self, idx: usize) -> Option<(Snapshot, StringInterner)> {
        match &self.source {
            WalSource::File { path, entries } => {
                let entry = entries.get(idx)?;
                StorageManager::load_wal_snapshot_with_interner(
                    path,
                    entry.byte_offset,
                    entry.byte_length,
                )
                .ok()
            }
            WalSource::InMemory { snapshots } => snapshots
                .get(idx)
                .map(|s| (s.clone(), StringInterner::new())),
        }
    }
}

// ============================================================
// HistoryProvider — lazy loading, no cache
// ============================================================

/// Provider for historical data from storage files.
///
/// Only metadata (timestamps, chunk paths) is kept in RAM permanently.
/// Snapshot data and per-chunk interners are loaded on demand from disk —
/// no cache, minimal memory footprint.
pub struct HistoryProvider {
    /// Metadata for each chunk file, sorted by timestamp.
    chunks: Vec<ChunkMeta>,
    /// WAL (unflushed) snapshots — lazy loaded from file.
    wal: Option<WalIndex>,
    /// Current cursor position (global, 0-based).
    cursor: usize,
    /// Total number of snapshots across all chunks + WAL.
    total_snapshots: usize,
    /// Sorted list of all snapshot timestamps for binary search.
    timestamps: Vec<i64>,
    /// Internal snapshot buffer (for SnapshotProvider::current() → &Snapshot).
    current_buffer: Option<Snapshot>,
    /// Per-chunk/WAL interner for the currently buffered snapshot.
    current_interner: Option<StringInterner>,
    /// Single-chunk cache: keeps last decompressed chunk to avoid repeated
    /// decompression when multiple snapshots are requested from the same chunk.
    chunk_cache: Option<SingleChunkCache>,

    last_error: Option<ProviderError>,
}

impl HistoryProvider {
    /// Creates a new history provider by scanning chunk files at the given path.
    ///
    /// Only metadata (timestamps, chunk paths) is loaded into RAM.
    /// Snapshot data and interners are loaded lazily on demand.
    pub fn from_path(storage_path: impl AsRef<Path>) -> Result<Self, ProviderError> {
        let (chunks, wal, total, timestamps) = Self::build_index(storage_path.as_ref())?;

        if total == 0 {
            return Err(ProviderError::Io(
                "No snapshots found in storage".to_string(),
            ));
        }

        let mut provider = Self {
            chunks,
            wal,
            cursor: 0,
            total_snapshots: total,
            timestamps,
            current_buffer: None,
            current_interner: None,
            chunk_cache: None,
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
            cursor: 0,
            total_snapshots: total,
            timestamps,
            current_buffer: Some(first_snapshot),
            current_interner: Some(StringInterner::new()),
            chunk_cache: None,
            last_error: None,
        })
    }

    /// Build the chunk index by scanning .zst files and WAL.
    ///
    /// For each chunk file: decompress, extract metadata (timestamps, count),
    /// then drop the chunk data. Interners are NOT merged — each chunk's interner
    /// is loaded on demand together with its snapshots via the LRU cache.
    /// Peak RAM = one chunk at a time.
    #[allow(clippy::type_complexity)]
    fn build_index(
        storage_path: &Path,
    ) -> Result<(Vec<ChunkMeta>, Option<WalIndex>, usize, Vec<i64>), ProviderError> {
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
        let mut all_timestamps: Vec<i64> = Vec::new();
        let mut global_offset: usize = 0;

        // Scan each chunk file: prefer .meta sidecar, fallback to decompression
        for path in chunk_paths {
            let meta_path = ChunkMetadata::meta_path(&path);
            let (snapshot_count, chunk_timestamps) =
                if let Ok(meta) = ChunkMetadata::load(&meta_path) {
                    // Fast path: read compact .meta file (tens of bytes)
                    (meta.snapshot_count, meta.timestamps)
                } else {
                    // Fallback: decompress .zst (for old files without .meta)
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
                    let ts: Vec<i64> = chunk.deltas.iter().map(|d| d.timestamp()).collect();
                    let count = ts.len();
                    // Write .meta for future starts
                    let meta = ChunkMetadata {
                        snapshot_count: count,
                        timestamps: ts.clone(),
                    };
                    let _ = meta.save(&meta_path);
                    (count, ts)
                    // chunk dropped here — RAM freed
                };

            if snapshot_count == 0 {
                continue;
            }

            all_timestamps.extend_from_slice(&chunk_timestamps);

            chunks.push(ChunkMeta {
                path,
                snapshot_count,
                global_offset,
                available: true,
            });

            global_offset += snapshot_count;
        }

        // Scan WAL metadata lazily — snapshots and interners are NOT kept in memory
        let wal = {
            let wal_path = storage_path.join("wal.log");
            let wal_entries = StorageManager::scan_wal_metadata(&wal_path)
                .map_err(|e| ProviderError::Io(format!("Failed to scan WAL: {}", e)))?;

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

        Ok((chunks, wal, total, all_timestamps))
    }

    /// Resolve a global position to (chunk_index, offset_in_chunk) or WAL index.
    fn resolve_position(&self, position: usize) -> Option<SnapshotLocation> {
        if position >= self.total_snapshots {
            return None;
        }

        // Check WAL first (fast path for latest snapshots)
        if let Some(ref wal) = self.wal
            && position >= wal.global_offset
        {
            let wal_idx = position - wal.global_offset;
            if wal_idx < wal.len() {
                return Some(SnapshotLocation::Wal(wal_idx));
            }
            return None;
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

    /// Load a single snapshot + interner from a chunk, using the single-chunk cache.
    /// If the requested chunk is already cached, no disk I/O or decompression occurs.
    fn load_from_chunk_cached(
        chunk_cache: &mut Option<SingleChunkCache>,
        chunks: &mut [ChunkMeta],
        chunk_idx: usize,
        offset_in_chunk: usize,
    ) -> Result<(Snapshot, StringInterner), ProviderError> {
        SingleChunkCache::get_or_load(chunk_cache, chunk_idx, &chunks[chunk_idx])?;

        let cached = chunk_cache.as_ref().unwrap();

        let snapshot = StorageManager::reconstruct_snapshot_at(&cached.chunk, offset_in_chunk)
            .map_err(|e| {
                ProviderError::Io(format!(
                    "Failed to reconstruct snapshot at offset {} in {}: {}",
                    offset_in_chunk,
                    chunks[chunk_idx].path.display(),
                    e
                ))
            })?;

        let interner = cached.chunk.interner.clone();

        Ok((snapshot, interner))
    }

    /// Load snapshot at global position into the internal buffer,
    /// along with the per-chunk/WAL interner for string resolution.
    fn load_into_buffer(&mut self, position: usize) {
        let (snapshot, interner) = match self.resolve_position(position) {
            Some(SnapshotLocation::Wal(wal_idx)) => {
                match self
                    .wal
                    .as_ref()
                    .and_then(|w| w.load_snapshot_with_interner(wal_idx))
                {
                    Some((s, i)) => (Some(s), Some(i)),
                    None => (None, None),
                }
            }
            Some(SnapshotLocation::Chunk {
                chunk_idx,
                offset_in_chunk,
            }) => {
                match Self::load_from_chunk_cached(
                    &mut self.chunk_cache,
                    &mut self.chunks,
                    chunk_idx,
                    offset_in_chunk,
                ) {
                    Ok((s, i)) => (Some(s), Some(i)),
                    Err(e) => {
                        warn!(error = %e, position, "failed to load snapshot");
                        self.chunks[chunk_idx].available = false;
                        (None, None)
                    }
                }
            }
            None => (None, None),
        };

        self.current_buffer = snapshot;
        self.current_interner = interner;
    }

    /// Get a cloned snapshot at the given position (for external use).
    /// Uses single-chunk cache to avoid repeated decompression.
    fn snapshot_cloned(&mut self, position: usize) -> Option<Snapshot> {
        match self.resolve_position(position) {
            Some(SnapshotLocation::Wal(wal_idx)) => {
                self.wal.as_ref().and_then(|w| w.load_snapshot(wal_idx))
            }
            Some(SnapshotLocation::Chunk {
                chunk_idx,
                offset_in_chunk,
            }) => {
                match Self::load_from_chunk_cached(
                    &mut self.chunk_cache,
                    &mut self.chunks,
                    chunk_idx,
                    offset_in_chunk,
                ) {
                    Ok((s, _interner)) => Some(s),
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

    /// Drops the cached decompressed chunk, freeing its memory.
    /// Call this after a batch of snapshot reads (e.g. after reconvert_current)
    /// to release RAM back to the allocator.
    pub fn drop_cache(&mut self) {
        self.chunk_cache = None;
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

            // New chunk file discovered — prefer .meta sidecar
            let meta_path = ChunkMetadata::meta_path(path);
            let (snapshot_count, chunk_timestamps) = if let Ok(meta) =
                ChunkMetadata::load(&meta_path)
            {
                (meta.snapshot_count, meta.timestamps)
            } else {
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
                let ts: Vec<i64> = chunk.deltas.iter().map(|d| d.timestamp()).collect();
                let count = ts.len();
                let meta = ChunkMetadata {
                    snapshot_count: count,
                    timestamps: ts.clone(),
                };
                let _ = meta.save(&meta_path);
                (count, ts)
            };

            if snapshot_count == 0 {
                continue;
            }

            new_timestamps.extend_from_slice(&chunk_timestamps);

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
        let wal_entries = StorageManager::scan_wal_metadata(&wal_path)
            .map_err(|e| ProviderError::Io(format!("Failed to scan WAL: {}", e)))?;

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
        self.current_interner.as_ref()
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
