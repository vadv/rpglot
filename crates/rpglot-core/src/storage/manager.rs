use crate::storage::interner::StringInterner;
use crate::storage::model::{DataBlock, Snapshot};
use chrono::{DateTime, NaiveDate, Timelike, Utc};
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Configuration for automatic data rotation.
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// Maximum total size of all data files in bytes. Default: 1GB.
    pub max_total_size: u64,
    /// Maximum retention period in days. Default: 7 days.
    pub max_retention_days: u32,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            max_total_size: 1_073_741_824, // 1GB
            max_retention_days: 7,
        }
    }
}

impl RotationConfig {
    /// Creates a new RotationConfig with custom values.
    pub fn new(max_total_size: u64, max_retention_days: u32) -> Self {
        Self {
            max_total_size,
            max_retention_days,
        }
    }
}

/// WAL entry containing a snapshot and its string interner.
/// Each WAL entry is self-contained for recovery purposes.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct WalEntry {
    pub(crate) snapshot: Snapshot,
    pub(crate) interner: StringInterner,
}

pub struct StorageManager {
    base_path: PathBuf,
    chunk_size_limit: usize,
    wal_file: File,
    /// Number of entries currently in WAL (for size limit checking)
    wal_entries_count: usize,
    /// Current hour (0-23) for hourly file segmentation
    current_hour: Option<u32>,
    /// Current date for hourly file segmentation
    current_date: Option<NaiveDate>,
}

impl StorageManager {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        let base_path = base_path.into();
        std::fs::create_dir_all(&base_path).unwrap();

        // Cleanup old .tmp files
        if let Ok(entries) = std::fs::read_dir(&base_path) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "tmp") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        let wal_path = base_path.join("wal.log");
        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&wal_path)
            .unwrap();

        let mut manager = Self {
            base_path,
            chunk_size_limit: 360, // ~1 hour at 10-second intervals
            wal_file,
            wal_entries_count: 0,
            current_hour: None,
            current_date: None,
        };

        manager.recover_from_wal();
        manager
    }

    /// Recovers WAL state on startup.
    /// Counts valid entries and truncates any corrupted data at the end.
    fn recover_from_wal(&mut self) {
        let wal_path = self.base_path.join("wal.log");

        // Migration: remove old strings.bin if exists (no longer needed)
        let strings_path = self.base_path.join("strings.bin");
        if strings_path.exists() {
            let _ = std::fs::remove_file(&strings_path);
        }

        let data = match std::fs::read(&wal_path) {
            Ok(d) if !d.is_empty() => d,
            _ => return,
        };

        let mut cursor = std::io::Cursor::new(&data);
        let mut valid_end_position = 0u64;
        let mut recovered_count = 0usize;

        // Count valid WAL entries and find valid end position
        while let Ok(_entry) = bincode::deserialize_from::<_, WalEntry>(&mut cursor) {
            valid_end_position = cursor.position();
            recovered_count += 1;
        }

        self.wal_entries_count = recovered_count;

        // Check if there's garbage after valid records (corruption detected)
        let file_size = data.len() as u64;
        if valid_end_position < file_size && valid_end_position > 0 {
            let garbage_bytes = file_size - valid_end_position;
            warn!(
                "WAL corruption detected: {} garbage bytes after {} valid records. Truncating WAL.",
                garbage_bytes, recovered_count
            );

            // Truncate WAL to remove corrupted data
            if let Err(e) = OpenOptions::new()
                .write(true)
                .open(&wal_path)
                .and_then(|f| f.set_len(valid_end_position))
            {
                warn!("Failed to truncate WAL: {}", e);
            }
        }
    }

    /// Collects all string hashes used in a single snapshot.
    fn collect_snapshot_hashes(snapshot: &Snapshot) -> HashSet<u64> {
        let mut hashes = HashSet::new();
        for block in &snapshot.blocks {
            match block {
                DataBlock::Processes(procs) => {
                    for p in procs {
                        hashes.insert(p.name_hash);
                        hashes.insert(p.cmdline_hash);
                        hashes.insert(p.cpu.wchan_hash);
                    }
                }
                DataBlock::SystemNet(nets) => {
                    for n in nets {
                        hashes.insert(n.name_hash);
                    }
                }
                DataBlock::SystemDisk(disks) => {
                    for d in disks {
                        hashes.insert(d.device_hash);
                    }
                }
                DataBlock::SystemInterrupts(intrs) => {
                    for i in intrs {
                        hashes.insert(i.irq_hash);
                    }
                }
                DataBlock::SystemSoftirqs(softirqs) => {
                    for s in softirqs {
                        hashes.insert(s.name_hash);
                    }
                }
                DataBlock::PgStatActivity(activities) => {
                    for a in activities {
                        hashes.insert(a.datname_hash);
                        hashes.insert(a.usename_hash);
                        hashes.insert(a.application_name_hash);
                        hashes.insert(a.state_hash);
                        hashes.insert(a.query_hash);
                        hashes.insert(a.wait_event_type_hash);
                        hashes.insert(a.wait_event_hash);
                        hashes.insert(a.backend_type_hash);
                    }
                }
                DataBlock::PgStatStatements(stmts) => {
                    for s in stmts {
                        hashes.insert(s.query_hash);
                    }
                }
                DataBlock::PgStatDatabase(dbs) => {
                    for d in dbs {
                        hashes.insert(d.datname_hash);
                    }
                }
                DataBlock::PgStatUserTables(tables) => {
                    for t in tables {
                        hashes.insert(t.schemaname_hash);
                        hashes.insert(t.relname_hash);
                    }
                }
                DataBlock::PgStatUserIndexes(indexes) => {
                    for i in indexes {
                        hashes.insert(i.schemaname_hash);
                        hashes.insert(i.relname_hash);
                        hashes.insert(i.indexrelname_hash);
                    }
                }
                DataBlock::PgLockTree(nodes) => {
                    for n in nodes {
                        hashes.insert(n.datname_hash);
                        hashes.insert(n.usename_hash);
                        hashes.insert(n.state_hash);
                        hashes.insert(n.wait_event_type_hash);
                        hashes.insert(n.wait_event_hash);
                        hashes.insert(n.query_hash);
                        hashes.insert(n.application_name_hash);
                        hashes.insert(n.backend_type_hash);
                        hashes.insert(n.lock_type_hash);
                        hashes.insert(n.lock_mode_hash);
                        hashes.insert(n.lock_target_hash);
                    }
                }
                _ => {}
            }
        }
        hashes
    }

    /// Adds a snapshot to storage with hourly segmentation.
    /// Returns true if a chunk was flushed (hour boundary crossed or size limit reached).
    pub fn add_snapshot(&mut self, snapshot: Snapshot, interner: &StringInterner) -> bool {
        // Check if hour changed and flush if needed
        let now = Utc::now();
        let current_hour = now.hour();
        let current_date = now.date_naive();
        let mut flushed = false;

        if let (Some(prev_hour), Some(prev_date)) = (self.current_hour, self.current_date)
            && (prev_hour != current_hour || prev_date != current_date)
            && self.wal_entries_count > 0
        {
            // Hour changed, flush the current chunk
            let _ = self.flush_chunk_with_time(prev_date, prev_hour);
            flushed = true;
        }

        // Update current hour/date tracking
        self.current_hour = Some(current_hour);
        self.current_date = Some(current_date);

        // Create minimal interner for this WAL entry (only hashes used in this snapshot)
        let used_hashes = Self::collect_snapshot_hashes(&snapshot);
        let wal_interner = interner.filter(&used_hashes);

        // Write to WAL for SIGKILL resilience
        let wal_entry = WalEntry {
            snapshot,
            interner: wal_interner,
        };
        let encoded = bincode::serialize(&wal_entry).unwrap();
        self.wal_file.write_all(&encoded).unwrap();
        self.wal_file.sync_all().unwrap();
        self.wal_entries_count += 1;

        // Check if size limit reached
        if self.wal_entries_count >= self.chunk_size_limit {
            let _ = self.flush_chunk();
            flushed = true;
        }

        flushed
    }

    /// Adds a snapshot with a specific timestamp (for testing or replay).
    /// Returns true if a chunk was flushed.
    pub fn add_snapshot_at(
        &mut self,
        snapshot: Snapshot,
        time: DateTime<Utc>,
        interner: &StringInterner,
    ) -> bool {
        let hour = time.hour();
        let date = time.date_naive();
        let mut flushed = false;

        if let (Some(prev_hour), Some(prev_date)) = (self.current_hour, self.current_date)
            && (prev_hour != hour || prev_date != date)
            && self.wal_entries_count > 0
        {
            let _ = self.flush_chunk_with_time(prev_date, prev_hour);
            flushed = true;
        }

        self.current_hour = Some(hour);
        self.current_date = Some(date);

        // Create minimal interner for this WAL entry
        let used_hashes = Self::collect_snapshot_hashes(&snapshot);
        let wal_interner = interner.filter(&used_hashes);

        // Write to WAL
        let wal_entry = WalEntry {
            snapshot,
            interner: wal_interner,
        };
        let encoded = bincode::serialize(&wal_entry).unwrap();
        self.wal_file.write_all(&encoded).unwrap();
        self.wal_file.sync_all().unwrap();
        self.wal_entries_count += 1;

        // Check if size limit reached
        if self.wal_entries_count >= self.chunk_size_limit {
            let _ = self.flush_chunk_with_time(date, hour);
            flushed = true;
        }

        flushed
    }

    /// Flushes the current chunk using current time for filename.
    pub fn flush_chunk(&mut self) -> std::io::Result<()> {
        let now = Utc::now();
        let date = self.current_date.unwrap_or_else(|| now.date_naive());
        let hour = self.current_hour.unwrap_or_else(|| now.hour());
        self.flush_chunk_with_time(date, hour)
    }

    /// Flushes WAL to a compressed chunk file in the new per-snapshot zstd frame format.
    /// Each snapshot is stored as an independent zstd frame for O(1) random access.
    /// File naming format: rpglot_YYYY-MM-DD_HH.zst
    fn flush_chunk_with_time(&mut self, date: NaiveDate, hour: u32) -> std::io::Result<()> {
        if self.wal_entries_count == 0 {
            return Err(std::io::Error::other("Empty WAL"));
        }

        // Read all snapshots from WAL
        let (snapshots, interner) = self.load_wal_snapshots_with_interner()?;
        if snapshots.is_empty() {
            return Err(std::io::Error::other("No snapshots in WAL"));
        }

        // Build optimized interner with only hashes used across all snapshots
        let mut used_hashes = HashSet::new();
        for snapshot in &snapshots {
            let h = Self::collect_snapshot_hashes(snapshot);
            used_hashes.extend(h);
        }
        let filtered_interner = interner.filter(&used_hashes);

        let filename = format!("rpglot_{}_{:02}.zst", date.format("%Y-%m-%d"), hour);
        let final_path = self.base_path.join(&filename);

        // If file already exists, append timestamp to make it unique
        let final_path = if final_path.exists() {
            let filename = format!(
                "rpglot_{}_{:02}_{}.zst",
                date.format("%Y-%m-%d"),
                hour,
                Utc::now().timestamp_nanos_opt().unwrap_or(0)
            );
            self.base_path.join(filename)
        } else {
            final_path
        };

        // Write chunk in new format (atomic via .tmp rename)
        crate::storage::chunk::write_chunk(&final_path, &snapshots, &filtered_interner)?;

        // Write .metrics sidecar for timeline heatmap
        let metrics = crate::storage::metrics::build_metrics_from_snapshots(&snapshots);
        let mpath = crate::storage::metrics::metrics_path(&final_path);
        if let Err(e) = crate::storage::metrics::write_metrics(&mpath, &metrics) {
            tracing::warn!(error = %e, "failed to write chunk metrics");
        }

        // Truncate WAL
        self.wal_file.set_len(0)?;
        self.wal_file.sync_all()?;

        // Reset WAL entry count
        self.wal_entries_count = 0;

        Ok(())
    }

    /// Loads unflushed snapshots and their interners from WAL file.
    pub fn load_wal_snapshots(&self) -> std::io::Result<(Vec<Snapshot>, StringInterner)> {
        self.load_wal_snapshots_with_interner()
    }

    /// Loads unflushed snapshots and their interners from WAL file (internal).
    fn load_wal_snapshots_with_interner(&self) -> std::io::Result<(Vec<Snapshot>, StringInterner)> {
        let wal_path = self.base_path.join("wal.log");
        let mut snapshots = Vec::new();
        let mut merged_interner = StringInterner::new();

        if let Ok(data) = std::fs::read(&wal_path)
            && !data.is_empty()
        {
            let mut cursor = std::io::Cursor::new(&data);
            while let Ok(entry) = bincode::deserialize_from::<_, WalEntry>(&mut cursor) {
                merged_interner.merge(&entry.interner);
                snapshots.push(entry.snapshot);
            }
        }

        Ok((snapshots, merged_interner))
    }

    /// Scans WAL file and returns entry metadata (byte_offset, byte_length, timestamp)
    /// for each entry. Snapshots are deserialized to extract timestamps but immediately
    /// dropped — peak RAM = one entry at a time. Interners are NOT merged; each WAL
    /// entry's interner is loaded on demand via `load_wal_snapshot_with_interner`.
    pub fn scan_wal_metadata(wal_path: &Path) -> std::io::Result<Vec<(u64, u64, i64)>> {
        use std::io::{BufReader, Seek};
        let file = match std::fs::File::open(wal_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(e) => return Err(e),
        };
        let file_len = file.metadata()?.len();
        if file_len == 0 {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let mut reader = BufReader::new(file);

        while reader.stream_position()? < file_len {
            let start = reader.stream_position()?;
            match bincode::deserialize_from::<_, WalEntry>(&mut reader) {
                Ok(entry) => {
                    let end = reader.stream_position()?;
                    let ts = entry.snapshot.timestamp;
                    entries.push((start, end - start, ts));
                    // entry (snapshot + interner) dropped here — RAM freed
                }
                Err(_) => break,
            }
        }

        Ok(entries)
    }

    /// Loads a single snapshot from WAL at the given byte range.
    /// Reads the WAL file, extracts the entry at [offset..offset+length], deserializes it.
    pub fn load_wal_snapshot_at(
        wal_path: &Path,
        offset: u64,
        length: u64,
    ) -> std::io::Result<Snapshot> {
        let (snapshot, _interner) =
            Self::load_wal_snapshot_with_interner(wal_path, offset, length)?;
        Ok(snapshot)
    }

    /// Loads a single snapshot + its interner from WAL at the given byte range.
    /// Each WAL entry contains a self-contained interner sufficient to resolve
    /// all string hashes in that entry's snapshot.
    /// Uses seek+read to avoid loading the entire WAL file into memory.
    pub fn load_wal_snapshot_with_interner(
        wal_path: &Path,
        offset: u64,
        length: u64,
    ) -> std::io::Result<(Snapshot, StringInterner)> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(wal_path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; length as usize];
        file.read_exact(&mut buf)?;
        let entry: WalEntry = bincode::deserialize(&buf).map_err(std::io::Error::other)?;
        Ok((entry.snapshot, entry.interner))
    }

    /// Loads all chunks from the storage directory and returns reconstructed snapshots
    /// along with a merged StringInterner containing all strings from all chunks.
    ///
    /// Also loads unflushed snapshots from WAL and their interners.
    /// Snapshots are returned in chronological order (oldest first).
    ///
    /// Reads v2 format (per-snapshot zstd frames). Legacy format files are skipped.
    pub fn load_all_snapshots_with_interner(
        &self,
    ) -> std::io::Result<(Vec<Snapshot>, StringInterner)> {
        let mut chunk_paths: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "zst") {
                chunk_paths.push(path);
            }
        }
        chunk_paths.sort();

        let mut snapshots = Vec::new();
        let mut merged_interner = StringInterner::new();

        for path in &chunk_paths {
            let reader = crate::storage::chunk::ChunkReader::open(path)?;
            let interner = reader.read_interner()?;
            merged_interner.merge(&interner);
            for i in 0..reader.snapshot_count() {
                snapshots.push(reader.read_snapshot(i)?);
            }
        }

        // Load unflushed snapshots and interners from WAL
        let (wal_snapshots, wal_interner) = self.load_wal_snapshots_with_interner()?;
        merged_interner.merge(&wal_interner);
        snapshots.extend(wal_snapshots);

        // Sort and deduplicate
        snapshots.sort_by_key(|s| s.timestamp);
        snapshots.dedup_by_key(|s| s.timestamp);

        Ok((snapshots, merged_interner))
    }

    /// Returns the number of snapshots in the WAL (unflushed).
    pub fn current_chunk_size(&self) -> usize {
        self.wal_entries_count
    }

    /// Rotates data files according to the given configuration.
    ///
    /// Removes files based on two criteria:
    /// 1. Files older than `max_retention_days`
    /// 2. Oldest files if total size exceeds `max_total_size`
    pub fn rotate(&self, config: &RotationConfig) -> std::io::Result<RotationResult> {
        let mut result = RotationResult::default();

        // Collect all .zst files with their metadata
        let mut files: Vec<FileInfo> = Vec::new();

        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "zst") {
                let metadata = entry.metadata()?;
                let size = metadata.len();

                // Extract date from filename (rpglot_YYYY-MM-DD_HH.zst)
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                let date = Self::parse_date_from_filename(filename);

                files.push(FileInfo { path, size, date });
            }
        }

        // Sort by date (oldest first)
        files.sort_by_key(|f| f.date);

        let now = Utc::now().date_naive();
        let retention_limit = now - chrono::Duration::days(config.max_retention_days as i64);

        // Remove files older than retention limit
        let mut remaining_files: Vec<FileInfo> = Vec::new();
        for file in files {
            if let Some(file_date) = file.date
                && file_date < retention_limit
            {
                std::fs::remove_file(&file.path)?;
                let _ = std::fs::remove_file(crate::storage::metrics::metrics_path(&file.path));
                result.files_removed_by_age += 1;
                result.bytes_freed += file.size;
                continue;
            }
            remaining_files.push(file);
        }

        // Calculate total size of remaining files
        let mut total_size: u64 = remaining_files.iter().map(|f| f.size).sum();

        // Remove oldest files if total size exceeds limit
        while total_size > config.max_total_size && !remaining_files.is_empty() {
            let file = remaining_files.remove(0);
            std::fs::remove_file(&file.path)?;
            let _ = std::fs::remove_file(crate::storage::metrics::metrics_path(&file.path));
            result.files_removed_by_size += 1;
            result.bytes_freed += file.size;
            total_size -= file.size;
        }

        result.total_size_after = total_size;
        result.files_remaining = remaining_files.len();

        Ok(result)
    }

    /// Parses date from filename format: rpglot_YYYY-MM-DD_HH.zst or chunk_*.zst
    fn parse_date_from_filename(filename: &str) -> Option<NaiveDate> {
        // Try new format: rpglot_YYYY-MM-DD_HH.zst
        if filename.starts_with("rpglot_") {
            let parts: Vec<&str> = filename
                .strip_prefix("rpglot_")?
                .strip_suffix(".zst")?
                .split('_')
                .collect();

            if !parts.is_empty() {
                return NaiveDate::parse_from_str(parts[0], "%Y-%m-%d").ok();
            }
        }

        // For old format (chunk_*), use file creation time via metadata
        // This is handled separately in rotate() if needed
        None
    }

    /// Returns the base path of the storage.
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
}

/// Information about a data file for rotation.
struct FileInfo {
    path: PathBuf,
    size: u64,
    date: Option<NaiveDate>,
}

/// Result of a rotation operation.
#[derive(Debug, Default)]
pub struct RotationResult {
    /// Number of files removed due to age (older than max_retention_days).
    pub files_removed_by_age: usize,
    /// Number of files removed due to size limit.
    pub files_removed_by_size: usize,
    /// Total bytes freed by rotation.
    pub bytes_freed: u64,
    /// Total size of remaining files after rotation.
    pub total_size_after: u64,
    /// Number of files remaining after rotation.
    pub files_remaining: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::chunk::ChunkReader;
    use crate::storage::model::ProcessInfo;
    use tempfile::tempdir;

    #[test]
    fn test_storage_manager_v2_round_trip() {
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 2;

        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 1,
                cmdline_hash: 1,
                ..ProcessInfo::default()
            }])],
        };

        let s2 = Snapshot {
            timestamp: 110,
            blocks: s1.blocks.clone(),
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());

        // At this point it should have flushed to a v2 chunk file
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "zst"))
            .collect();
        assert!(!entries.is_empty());

        // Read back via ChunkReader (v2 format)
        let chunk_path = entries[0].path();
        let reader = ChunkReader::open(&chunk_path).unwrap();
        assert_eq!(reader.snapshot_count(), 2);

        let loaded_s1 = reader.read_snapshot(0).unwrap();
        assert_eq!(loaded_s1.timestamp, 100);
        if let DataBlock::Processes(procs) = &loaded_s1.blocks[0] {
            assert_eq!(procs.len(), 1);
            assert_eq!(procs[0].pid, 1);
        } else {
            panic!("Expected DataBlock::Processes");
        }

        let loaded_s2 = reader.read_snapshot(1).unwrap();
        assert_eq!(loaded_s2.timestamp, 110);
    }

    #[test]
    fn test_storage_manager_wal_recovery() {
        let dir = tempdir().unwrap();
        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 1,
                cmdline_hash: 1,
                ..ProcessInfo::default()
            }])],
        };

        {
            let mut manager = StorageManager::new(dir.path());
            manager.add_snapshot(s1.clone(), &StringInterner::new());
            // Drop manager without flushing chunk to disk (simulated crash)
        }

        // New manager should recover WAL entry count
        let manager = StorageManager::new(dir.path());
        assert_eq!(manager.current_chunk_size(), 1);

        // Verify snapshot can be loaded from WAL
        let (snapshots, _) = manager.load_all_snapshots_with_interner().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].timestamp, 100);
    }

    #[test]
    fn test_storage_manager_v2_pg_round_trip() {
        use crate::storage::model::PgStatActivityInfo;

        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 2;

        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::PgStatActivity(vec![PgStatActivityInfo {
                pid: 1234,
                query_hash: 555,
                ..PgStatActivityInfo::default()
            }])],
        };

        let s2 = Snapshot {
            timestamp: 110,
            blocks: vec![DataBlock::PgStatActivity(vec![PgStatActivityInfo {
                pid: 1234,
                query_hash: 666,
                ..PgStatActivityInfo::default()
            }])],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "zst"))
            .collect();
        assert!(!entries.is_empty());

        let reader = ChunkReader::open(&entries[0].path()).unwrap();
        assert_eq!(reader.snapshot_count(), 2);

        // First snapshot has query_hash=555
        let loaded_s1 = reader.read_snapshot(0).unwrap();
        if let DataBlock::PgStatActivity(acts) = &loaded_s1.blocks[0] {
            assert_eq!(acts[0].query_hash, 555);
        } else {
            panic!("Expected PgStatActivity");
        }

        // Second snapshot has query_hash=666
        let loaded_s2 = reader.read_snapshot(1).unwrap();
        if let DataBlock::PgStatActivity(acts) = &loaded_s2.blocks[0] {
            assert_eq!(acts[0].query_hash, 666);
        } else {
            panic!("Expected PgStatActivity");
        }
    }

    #[test]
    fn test_storage_manager_v2_system_round_trip() {
        use crate::storage::model::{SystemCpuInfo, SystemLoadInfo};

        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 2;

        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![
                DataBlock::SystemCpu(vec![SystemCpuInfo {
                    cpu_id: -1,
                    user: 100,
                    ..SystemCpuInfo::default()
                }]),
                DataBlock::SystemLoad(SystemLoadInfo {
                    lavg1: 0.1,
                    ..SystemLoadInfo::default()
                }),
            ],
        };

        let s2 = Snapshot {
            timestamp: 110,
            blocks: vec![
                DataBlock::SystemCpu(vec![SystemCpuInfo {
                    cpu_id: -1,
                    user: 110,
                    ..SystemCpuInfo::default()
                }]),
                DataBlock::SystemLoad(SystemLoadInfo {
                    lavg1: 0.2,
                    ..SystemLoadInfo::default()
                }),
            ],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "zst"))
            .collect();
        assert!(!entries.is_empty());

        let reader = ChunkReader::open(&entries[0].path()).unwrap();
        assert_eq!(reader.snapshot_count(), 2);

        // Second snapshot has updated values
        let loaded_s2 = reader.read_snapshot(1).unwrap();
        if let DataBlock::SystemCpu(cpus) = &loaded_s2.blocks[0] {
            assert_eq!(cpus[0].user, 110);
        } else {
            panic!("Expected SystemCpu");
        }
        if let DataBlock::SystemLoad(load) = &loaded_s2.blocks[1] {
            assert_eq!(load.lavg1, 0.2);
        } else {
            panic!("Expected SystemLoad");
        }
    }

    #[test]
    fn test_rotation_by_days() {
        let dir = tempdir().unwrap();
        let manager = StorageManager::new(dir.path());

        // Create test files with dates
        let now = Utc::now().date_naive();
        let old_date = now - chrono::Duration::days(10);
        let recent_date = now - chrono::Duration::days(3);

        // Create old file (should be deleted)
        let old_file = dir
            .path()
            .join(format!("rpglot_{}_12.zst", old_date.format("%Y-%m-%d")));
        std::fs::write(&old_file, b"old data").unwrap();

        // Create recent file (should be kept)
        let recent_file = dir
            .path()
            .join(format!("rpglot_{}_12.zst", recent_date.format("%Y-%m-%d")));
        std::fs::write(&recent_file, b"recent data").unwrap();

        let config = RotationConfig::new(1_000_000_000, 7); // 1GB, 7 days
        let result = manager.rotate(&config).unwrap();

        assert_eq!(result.files_removed_by_age, 1);
        assert_eq!(result.files_remaining, 1);
        assert!(!old_file.exists());
        assert!(recent_file.exists());
    }

    #[test]
    fn test_rotation_by_size() {
        let dir = tempdir().unwrap();
        let manager = StorageManager::new(dir.path());

        let now = Utc::now().date_naive();
        let day1 = now - chrono::Duration::days(3);
        let day2 = now - chrono::Duration::days(2);
        let day3 = now - chrono::Duration::days(1);

        // Create files totaling more than the limit (different dates for predictable sort order)
        let file1 = dir
            .path()
            .join(format!("rpglot_{}_10.zst", day1.format("%Y-%m-%d")));
        let file2 = dir
            .path()
            .join(format!("rpglot_{}_10.zst", day2.format("%Y-%m-%d")));
        let file3 = dir
            .path()
            .join(format!("rpglot_{}_10.zst", day3.format("%Y-%m-%d")));

        std::fs::write(&file1, vec![0u8; 500]).unwrap(); // 500 bytes, oldest
        std::fs::write(&file2, vec![0u8; 500]).unwrap(); // 500 bytes
        std::fs::write(&file3, vec![0u8; 500]).unwrap(); // 500 bytes, newest

        // Set max size to 1000 bytes (should keep only 2 files)
        let config = RotationConfig::new(1000, 365);
        let result = manager.rotate(&config).unwrap();

        assert_eq!(result.files_removed_by_size, 1);
        assert_eq!(result.files_remaining, 2);
        assert!(!file1.exists()); // oldest removed
        assert!(file2.exists());
        assert!(file3.exists());
    }

    #[test]
    fn test_hourly_file_naming() {
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 100; // Large limit to prevent auto-flush by count

        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 1,
                cmdline_hash: 1,
                ..ProcessInfo::default()
            }])],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.flush_chunk().unwrap();

        // Check that file was created with the new naming format
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("rpglot_") && name.ends_with(".zst")
            })
            .collect();

        assert_eq!(entries.len(), 1);
        let filename = entries[0].file_name().to_string_lossy().to_string();
        assert!(filename.starts_with("rpglot_"));
        assert!(filename.contains("_")); // Contains date and hour separator
    }

    #[test]
    fn test_parse_date_from_filename() {
        // Test new format
        let date = StorageManager::parse_date_from_filename("rpglot_2026-02-07_17.zst");
        assert!(date.is_some());
        assert_eq!(date.unwrap().to_string(), "2026-02-07");

        // Test with collision suffix
        let date = StorageManager::parse_date_from_filename("rpglot_2026-02-07_17_123456789.zst");
        assert!(date.is_some());
        assert_eq!(date.unwrap().to_string(), "2026-02-07");

        // Test old format (returns None)
        let date = StorageManager::parse_date_from_filename("chunk_1234567890.zst");
        assert!(date.is_none());
    }

    #[test]
    fn test_rotation_config_default() {
        let config = RotationConfig::default();
        assert_eq!(config.max_total_size, 1_073_741_824); // 1GB
        assert_eq!(config.max_retention_days, 7);
    }

    #[test]
    fn test_load_all_snapshots_includes_wal() {
        use crate::storage::model::SystemLoadInfo;
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 100; // Large limit to prevent auto-flush

        // Add snapshots that will stay in WAL (not flushed)
        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 1.0,
                ..SystemLoadInfo::default()
            })],
        };
        let s2 = Snapshot {
            timestamp: 200,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 2.0,
                ..SystemLoadInfo::default()
            })],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());

        // DO NOT flush - snapshots should be in WAL only
        // Verify WAL file exists and has content
        let wal_path = dir.path().join("wal.log");
        assert!(wal_path.exists());
        let wal_size = std::fs::metadata(&wal_path).unwrap().len();
        assert!(wal_size > 0);

        // Create a new manager to read (simulates rpglot -r)
        let reader = StorageManager::new(dir.path());
        let (snapshots, _) = reader.load_all_snapshots_with_interner().unwrap();

        // Should read 2 snapshots from WAL
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].timestamp, 100);
        assert_eq!(snapshots[1].timestamp, 200);
    }

    #[test]
    fn test_load_all_snapshots_combines_chunks_and_wal() {
        use crate::storage::model::SystemLoadInfo;
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 2; // Small limit to trigger flush

        // Add 2 snapshots (will be flushed to chunk)
        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 1.0,
                ..SystemLoadInfo::default()
            })],
        };
        let s2 = Snapshot {
            timestamp: 200,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 2.0,
                ..SystemLoadInfo::default()
            })],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());
        // After 2 snapshots, chunk_size_limit=2 triggers flush

        // Add one more snapshot (stays in WAL)
        let s3 = Snapshot {
            timestamp: 300,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 3.0,
                ..SystemLoadInfo::default()
            })],
        };
        manager.add_snapshot(s3, &StringInterner::new());

        // Verify: should have 1 chunk file + WAL with 1 snapshot
        let chunk_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "zst"))
            .collect();
        assert_eq!(chunk_files.len(), 1);

        // Create a new manager to read
        let reader = StorageManager::new(dir.path());
        let (snapshots, _) = reader.load_all_snapshots_with_interner().unwrap();

        // Should read all 3 snapshots (2 from chunk + 1 from WAL)
        assert_eq!(snapshots.len(), 3);
        assert_eq!(snapshots[0].timestamp, 100);
        assert_eq!(snapshots[1].timestamp, 200);
        assert_eq!(snapshots[2].timestamp, 300);
    }
}
