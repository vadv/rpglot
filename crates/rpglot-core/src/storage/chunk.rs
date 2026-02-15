//! Chunk storage format with per-snapshot zstd frames and O(1) random access.
//!
//! File layout:
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │ HEADER (32 bytes, uncompressed)                         │
//! │   magic: [u8; 4]              = b"RPG2"                 │
//! │   version: u16                = 2                       │
//! │   snapshot_count: u16                                   │
//! │   interner_offset: u64        (byte offset in file)     │
//! │   interner_compressed_len: u64                          │
//! │   _reserved: [u8; 4]          = [0; 4]                  │
//! ├─────────────────────────────────────────────────────────┤
//! │ INDEX TABLE (snapshot_count × 24 bytes, uncompressed)   │
//! │   Per snapshot:                                         │
//! │     offset: u64   (byte position in file)               │
//! │     compressed_len: u64                                 │
//! │     timestamp: i64                                      │
//! ├─────────────────────────────────────────────────────────┤
//! │ SNAPSHOT FRAMES (variable, each an independent zstd)    │
//! │   zstd(bincode(Snapshot_0))                             │
//! │   zstd(bincode(Snapshot_1))                             │
//! │   ...                                                   │
//! ├─────────────────────────────────────────────────────────┤
//! │ INTERNER FRAME (one zstd frame)                         │
//! │   zstd(bincode(StringInterner))                         │
//! └─────────────────────────────────────────────────────────┘
//! ```

use crate::storage::interner::StringInterner;
use crate::storage::model::Snapshot;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

const MAGIC: [u8; 4] = *b"RPG2";
const VERSION: u16 = 2;
const HEADER_SIZE: usize = 32;
const INDEX_ENTRY_SIZE: usize = 24; // offset: u64 + compressed_len: u64 + timestamp: i64

/// Reader for new-format chunk files with per-snapshot random access.
pub struct ChunkReader {
    snapshot_count: usize,
    /// (byte_offset, compressed_len, timestamp) for each snapshot frame.
    index: Vec<(u64, u64, i64)>,
    interner_offset: u64,
    interner_compressed_len: u64,
    /// Raw file data — kept in memory for reading individual frames.
    /// This is the file mmap alternative: read whole file once, then
    /// seek into it. File sizes are typically 2-4 MB.
    data: Vec<u8>,
}

impl ChunkReader {
    /// Opens a chunk file and reads only the header + index (no snapshot decompression).
    pub fn open(path: &Path) -> io::Result<Self> {
        let data = std::fs::read(path)?;

        if data.len() < HEADER_SIZE {
            return Err(io::Error::other("file too small for header"));
        }

        // Parse header
        let magic = &data[0..4];
        if magic != MAGIC {
            return Err(io::Error::other(format!(
                "invalid magic: expected RPG2, got {:?}",
                magic
            )));
        }

        let version = u16::from_le_bytes([data[4], data[5]]);
        if version != VERSION {
            return Err(io::Error::other(format!(
                "unsupported version: {}",
                version
            )));
        }

        let snapshot_count = u16::from_le_bytes([data[6], data[7]]) as usize;
        let interner_offset = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let interner_compressed_len = u64::from_le_bytes(data[16..24].try_into().unwrap());
        // bytes 24..28 = reserved

        let index_size = snapshot_count * INDEX_ENTRY_SIZE;
        let expected_min = HEADER_SIZE + index_size;
        if data.len() < expected_min {
            return Err(io::Error::other("file too small for index"));
        }

        // Parse index table
        let mut index = Vec::with_capacity(snapshot_count);
        for i in 0..snapshot_count {
            let base = HEADER_SIZE + i * INDEX_ENTRY_SIZE;
            let offset = u64::from_le_bytes(data[base..base + 8].try_into().unwrap());
            let compressed_len = u64::from_le_bytes(data[base + 8..base + 16].try_into().unwrap());
            let timestamp = i64::from_le_bytes(data[base + 16..base + 24].try_into().unwrap());
            index.push((offset, compressed_len, timestamp));
        }

        Ok(Self {
            snapshot_count,
            index,
            interner_offset,
            interner_compressed_len,
            data,
        })
    }

    /// Returns the number of snapshots in this chunk.
    pub fn snapshot_count(&self) -> usize {
        self.snapshot_count
    }

    /// Returns timestamps of all snapshots from the index table (no decompression).
    pub fn timestamps(&self) -> Vec<i64> {
        self.index.iter().map(|(_, _, ts)| *ts).collect()
    }

    /// Reads and decompresses a single snapshot at the given index.
    /// Peak RAM: ~30 KB (one compressed frame + one decompressed snapshot).
    pub fn read_snapshot(&self, idx: usize) -> io::Result<Snapshot> {
        if idx >= self.snapshot_count {
            return Err(io::Error::other(format!(
                "snapshot index {} out of range (count={})",
                idx, self.snapshot_count
            )));
        }

        let (offset, compressed_len, _timestamp) = self.index[idx];
        let start = offset as usize;
        let end = start + compressed_len as usize;

        if end > self.data.len() {
            return Err(io::Error::other("snapshot frame extends past end of file"));
        }

        let decompressed = zstd::decode_all(&self.data[start..end])?;
        let snapshot: Snapshot = bincode::deserialize(&decompressed).map_err(io::Error::other)?;

        Ok(snapshot)
    }

    /// Reads and decompresses the interner frame.
    pub fn read_interner(&self) -> io::Result<StringInterner> {
        let start = self.interner_offset as usize;
        let end = start + self.interner_compressed_len as usize;

        if end > self.data.len() {
            return Err(io::Error::other("interner frame extends past end of file"));
        }

        let decompressed = zstd::decode_all(&self.data[start..end])?;
        let interner: StringInterner =
            bincode::deserialize(&decompressed).map_err(io::Error::other)?;

        Ok(interner)
    }
}

/// Writes snapshots and interner in the new chunk format.
///
/// Each snapshot is stored as an independent zstd frame for O(1) random access.
/// The interner is stored as a separate zstd frame at the end.
///
/// The file is written atomically via a `.tmp` intermediate file.
pub fn write_chunk(
    path: &Path,
    snapshots: &[Snapshot],
    interner: &StringInterner,
) -> io::Result<()> {
    if snapshots.is_empty() {
        return Err(io::Error::other("cannot write empty chunk"));
    }
    if snapshots.len() > u16::MAX as usize {
        return Err(io::Error::other("too many snapshots for chunk format"));
    }

    let tmp_path = path.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp_path)?;

    let snapshot_count = snapshots.len() as u16;

    // Write placeholder header (will be updated later)
    let header_placeholder = [0u8; HEADER_SIZE];
    file.write_all(&header_placeholder)?;

    // Write placeholder index (will be updated later)
    let index_placeholder = vec![0u8; snapshot_count as usize * INDEX_ENTRY_SIZE];
    file.write_all(&index_placeholder)?;

    // Write snapshot frames, recording offsets and timestamps
    let mut index_entries: Vec<(u64, u64, i64)> = Vec::with_capacity(snapshot_count as usize);

    for snapshot in snapshots {
        let offset = file.stream_position()?;
        let raw = bincode::serialize(snapshot).map_err(io::Error::other)?;
        let compressed = zstd::encode_all(&raw[..], 3)?;
        file.write_all(&compressed)?;
        index_entries.push((offset, compressed.len() as u64, snapshot.timestamp));
    }

    // Write interner frame
    let interner_offset = file.stream_position()?;
    let raw_interner = bincode::serialize(interner).map_err(io::Error::other)?;
    let compressed_interner = zstd::encode_all(&raw_interner[..], 3)?;
    let interner_compressed_len = compressed_interner.len() as u64;
    file.write_all(&compressed_interner)?;

    // Seek back and write real header
    file.seek(SeekFrom::Start(0))?;

    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&MAGIC);
    header[4..6].copy_from_slice(&VERSION.to_le_bytes());
    header[6..8].copy_from_slice(&snapshot_count.to_le_bytes());
    header[8..16].copy_from_slice(&interner_offset.to_le_bytes());
    header[16..24].copy_from_slice(&interner_compressed_len.to_le_bytes());
    // bytes 24..28 = reserved (zeros)
    file.write_all(&header)?;

    // Write real index
    for (offset, compressed_len, timestamp) in &index_entries {
        file.write_all(&offset.to_le_bytes())?;
        file.write_all(&compressed_len.to_le_bytes())?;
        file.write_all(&timestamp.to_le_bytes())?;
    }

    file.sync_all()?;
    drop(file);

    // Atomic rename
    std::fs::rename(tmp_path, path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::{DataBlock, ProcessInfo};
    use tempfile::tempdir;

    fn create_test_snapshots(count: usize) -> Vec<Snapshot> {
        (0..count)
            .map(|i| Snapshot {
                timestamp: 100 + i as i64 * 10,
                blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                    pid: (i + 1) as u32,
                    name_hash: 111 + i as u64,
                    cmdline_hash: 222 + i as u64,
                    ..ProcessInfo::default()
                }])],
            })
            .collect()
    }

    #[test]
    fn test_write_and_read_single_snapshot() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zst");
        let snapshots = create_test_snapshots(1);
        let interner = StringInterner::new();

        write_chunk(&path, &snapshots, &interner).unwrap();

        let reader = ChunkReader::open(&path).unwrap();
        assert_eq!(reader.snapshot_count(), 1);

        let snap = reader.read_snapshot(0).unwrap();
        assert_eq!(snap.timestamp, 100);
        if let DataBlock::Processes(procs) = &snap.blocks[0] {
            assert_eq!(procs[0].pid, 1);
        } else {
            panic!("expected Processes block");
        }
    }

    #[test]
    fn test_write_and_read_multiple_snapshots() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zst");
        let snapshots = create_test_snapshots(10);
        let interner = StringInterner::new();

        write_chunk(&path, &snapshots, &interner).unwrap();

        let reader = ChunkReader::open(&path).unwrap();
        assert_eq!(reader.snapshot_count(), 10);

        // Read each snapshot and verify
        for i in 0..10 {
            let snap = reader.read_snapshot(i).unwrap();
            assert_eq!(snap.timestamp, 100 + i as i64 * 10);
            if let DataBlock::Processes(procs) = &snap.blocks[0] {
                assert_eq!(procs[0].pid, (i + 1) as u32);
            } else {
                panic!("expected Processes block");
            }
        }
    }

    #[test]
    fn test_random_access_order() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zst");
        let snapshots = create_test_snapshots(5);
        let interner = StringInterner::new();

        write_chunk(&path, &snapshots, &interner).unwrap();

        let reader = ChunkReader::open(&path).unwrap();

        // Read in reverse order
        let s4 = reader.read_snapshot(4).unwrap();
        assert_eq!(s4.timestamp, 140);
        let s0 = reader.read_snapshot(0).unwrap();
        assert_eq!(s0.timestamp, 100);
        let s2 = reader.read_snapshot(2).unwrap();
        assert_eq!(s2.timestamp, 120);
    }

    #[test]
    fn test_interner_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zst");
        let snapshots = create_test_snapshots(1);
        let mut interner = StringInterner::new();
        let h1 = interner.intern("hello");
        let h2 = interner.intern("world");

        write_chunk(&path, &snapshots, &interner).unwrap();

        let reader = ChunkReader::open(&path).unwrap();
        let loaded = reader.read_interner().unwrap();
        assert_eq!(loaded.resolve(h1), Some("hello"));
        assert_eq!(loaded.resolve(h2), Some("world"));
    }

    #[test]
    fn test_timestamps_from_index() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zst");
        let snapshots = create_test_snapshots(5);
        let interner = StringInterner::new();

        write_chunk(&path, &snapshots, &interner).unwrap();

        let reader = ChunkReader::open(&path).unwrap();
        let timestamps = reader.timestamps();
        assert_eq!(timestamps, vec![100, 110, 120, 130, 140]);
    }

    #[test]
    fn test_out_of_range_snapshot() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zst");
        let snapshots = create_test_snapshots(3);
        let interner = StringInterner::new();

        write_chunk(&path, &snapshots, &interner).unwrap();

        let reader = ChunkReader::open(&path).unwrap();
        assert!(reader.read_snapshot(3).is_err());
        assert!(reader.read_snapshot(100).is_err());
    }

    #[test]
    fn test_empty_snapshots_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zst");
        let interner = StringInterner::new();

        assert!(write_chunk(&path, &[], &interner).is_err());
    }
}
