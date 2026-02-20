//! Chunk storage format with per-snapshot zstd frames, dictionary compression,
//! and O(1) random access.
//!
//! A trained zstd dictionary captures common patterns across snapshots within
//! a chunk, restoring cross-snapshot redundancy that was lost when moving from
//! monolithic compression to per-snapshot frames.
//!
//! File layout:
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │ HEADER (48 bytes, uncompressed)                         │
//! │   magic: [u8; 4]              = b"RPG5"                 │
//! │   version: u16                = 4                       │
//! │   snapshot_count: u16                                   │
//! │   interner_offset: u64        (byte offset in file)     │
//! │   interner_compressed_len: u64                          │
//! │   dict_offset: u64            (byte offset in file)     │
//! │   dict_len: u64               (raw dict size in bytes)  │
//! │   _reserved: [u8; 4]          = [0; 4]                  │
//! ├─────────────────────────────────────────────────────────┤
//! │ INDEX TABLE (snapshot_count × 28 bytes, uncompressed)   │
//! │   Per snapshot:                                         │
//! │     offset: u64           (byte position in file)       │
//! │     compressed_len: u64                                 │
//! │     timestamp: i64                                      │
//! │     uncompressed_len: u32 (for decompress capacity)     │
//! ├─────────────────────────────────────────────────────────┤
//! │ DICTIONARY (raw bytes, NOT zstd compressed)             │
//! │   zstd trained dictionary (~64-112 KB)                  │
//! ├─────────────────────────────────────────────────────────┤
//! │ SNAPSHOT FRAMES (each compressed WITH dictionary)       │
//! │   zstd_dict(postcard(Snapshot_0))                        │
//! │   zstd_dict(postcard(Snapshot_1))                       │
//! │   ...                                                   │
//! ├─────────────────────────────────────────────────────────┤
//! │ INTERNER FRAME (one zstd frame, WITHOUT dictionary)     │
//! │   zstd(postcard(StringInterner))                        │
//! └─────────────────────────────────────────────────────────┘
//! ```

use crate::storage::interner::StringInterner;
use crate::storage::model::Snapshot;
use std::fs;
use std::io::{self, Read as _, Seek, SeekFrom, Write};
use std::path::Path;
use tracing::warn;

const MAGIC: [u8; 4] = *b"RPG6";
const VERSION: u16 = 6;
const HEADER_SIZE: usize = 48;
const INDEX_ENTRY_SIZE: usize = 28; // offset: u64 + compressed_len: u64 + timestamp: i64 + uncompressed_len: u32
pub(crate) const DICT_MAX_SIZE: usize = 112 * 1024; // 112 KB

/// Lightweight metadata from a chunk's header + index table.
/// Reading this requires only ~10 KB of I/O (48-byte header + N × 28-byte index),
/// compared to 10–50 MB for the full `ChunkReader::open()`.
pub struct ChunkMetadata {
    pub snapshot_count: usize,
    pub timestamps: Vec<i64>,
}

/// Read only the chunk header + index table from disk.
///
/// Returns snapshot count and per-snapshot timestamps without loading the
/// dictionary, snapshot data, or interner. This is ~1000× cheaper than
/// `ChunkReader::open()` which reads the entire file via `fs::read()`.
pub fn read_chunk_metadata(path: &Path) -> io::Result<ChunkMetadata> {
    let mut file = fs::File::open(path)?;

    // Read header (48 bytes)
    let mut header = [0u8; HEADER_SIZE];
    file.read_exact(&mut header)?;

    let magic = &header[0..4];
    if magic != MAGIC {
        return Err(io::Error::other(format!(
            "invalid magic: expected RPG6, got {:?}",
            magic
        )));
    }

    let version = u16::from_le_bytes([header[4], header[5]]);
    if version != VERSION {
        return Err(io::Error::other(format!(
            "unsupported version: {}",
            version
        )));
    }

    let snapshot_count = u16::from_le_bytes([header[6], header[7]]) as usize;

    // Read index table (snapshot_count × 28 bytes)
    let index_size = snapshot_count * INDEX_ENTRY_SIZE;
    let mut index_buf = vec![0u8; index_size];
    file.read_exact(&mut index_buf)?;

    // Extract only timestamps from the index entries
    let mut timestamps = Vec::with_capacity(snapshot_count);
    for i in 0..snapshot_count {
        let base = i * INDEX_ENTRY_SIZE;
        let timestamp = i64::from_le_bytes(index_buf[base + 16..base + 24].try_into().unwrap());
        timestamps.push(timestamp);
    }

    Ok(ChunkMetadata {
        snapshot_count,
        timestamps,
    })
}

/// Reader for chunk files with per-snapshot random access and dictionary decompression.
pub struct ChunkReader {
    snapshot_count: usize,
    /// (byte_offset, compressed_len, timestamp, uncompressed_len) for each snapshot frame.
    index: Vec<(u64, u64, i64, u32)>,
    interner_offset: u64,
    interner_compressed_len: u64,
    /// Prepared decoder dictionary for fast repeated decompression.
    decoder_dict: zstd::dict::DecoderDictionary<'static>,
    /// Raw file data kept in memory for reading individual frames.
    data: Vec<u8>,
}

impl ChunkReader {
    /// Opens a chunk file: reads header + index + dictionary (no snapshot decompression).
    pub fn open(path: &Path) -> io::Result<Self> {
        let data = fs::read(path)?;

        if data.len() < HEADER_SIZE {
            return Err(io::Error::other("file too small for header"));
        }

        // Parse header
        let magic = &data[0..4];
        if magic != MAGIC {
            return Err(io::Error::other(format!(
                "invalid magic: expected RPG6, got {:?}",
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
        let dict_offset = u64::from_le_bytes(data[24..32].try_into().unwrap());
        let dict_len = u64::from_le_bytes(data[32..40].try_into().unwrap());
        // bytes 40..44 = reserved

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
            let uncompressed_len =
                u32::from_le_bytes(data[base + 24..base + 28].try_into().unwrap());
            index.push((offset, compressed_len, timestamp, uncompressed_len));
        }

        // Load dictionary
        let dict_start = dict_offset as usize;
        let dict_end = dict_start + dict_len as usize;
        if dict_end > data.len() {
            return Err(io::Error::other("dictionary extends past end of file"));
        }
        let decoder_dict = zstd::dict::DecoderDictionary::copy(&data[dict_start..dict_end]);

        Ok(Self {
            snapshot_count,
            index,
            interner_offset,
            interner_compressed_len,
            decoder_dict,
            data,
        })
    }

    /// Returns the number of snapshots in this chunk.
    pub fn snapshot_count(&self) -> usize {
        self.snapshot_count
    }

    /// Returns timestamps of all snapshots from the index table (no decompression).
    pub fn timestamps(&self) -> Vec<i64> {
        self.index.iter().map(|(_, _, ts, _)| *ts).collect()
    }

    /// Reads and decompresses a single snapshot at the given index using the dictionary.
    pub fn read_snapshot(&self, idx: usize) -> io::Result<Snapshot> {
        if idx >= self.snapshot_count {
            return Err(io::Error::other(format!(
                "snapshot index {} out of range (count={})",
                idx, self.snapshot_count
            )));
        }

        let (offset, compressed_len, _timestamp, uncompressed_len) = self.index[idx];
        let start = offset as usize;
        let end = start + compressed_len as usize;

        if end > self.data.len() {
            return Err(io::Error::other("snapshot frame extends past end of file"));
        }

        let mut decompressor =
            zstd::bulk::Decompressor::with_prepared_dictionary(&self.decoder_dict)?;
        let decompressed =
            decompressor.decompress(&self.data[start..end], uncompressed_len as usize)?;
        let snapshot: Snapshot = postcard::from_bytes(&decompressed).map_err(|e| {
            warn!(
                idx,
                compressed_len = compressed_len,
                uncompressed_len,
                decompressed_len = decompressed.len(),
                error = %e,
                "chunk: snapshot deserialization failed"
            );
            io::Error::other(e)
        })?;

        Ok(snapshot)
    }

    /// Reads and decompresses the interner frame (no dictionary — different data structure).
    pub fn read_interner(&self) -> io::Result<StringInterner> {
        let start = self.interner_offset as usize;
        let end = start + self.interner_compressed_len as usize;

        if end > self.data.len() {
            return Err(io::Error::other("interner frame extends past end of file"));
        }

        let decompressed = zstd::decode_all(&self.data[start..end])?;
        let interner: StringInterner = postcard::from_bytes(&decompressed).map_err(|e| {
            warn!(
                decompressed_len = decompressed.len(),
                error = %e,
                "chunk: interner deserialization failed"
            );
            io::Error::other(e)
        })?;

        Ok(interner)
    }
}

/// Internal: writes a chunk file given a pre-trained dictionary and a callback
/// that provides serialized snapshot data one at a time.
///
/// `get_raw_snapshot(index)` must return `(serialized_bytes, timestamp)`.
/// The file is written atomically via a `.tmp` intermediate file.
fn write_chunk_inner<F>(
    path: &Path,
    snapshot_count: usize,
    dictionary: &[u8],
    mut get_raw_snapshot: F,
    interner: &StringInterner,
) -> io::Result<()>
where
    F: FnMut(usize) -> io::Result<(Vec<u8>, i64)>,
{
    if snapshot_count == 0 {
        return Err(io::Error::other("cannot write empty chunk"));
    }
    if snapshot_count > u16::MAX as usize {
        return Err(io::Error::other("too many snapshots for chunk format"));
    }

    let tmp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp_path)?;

    let count = snapshot_count as u16;

    // Write placeholder header (will be updated later)
    file.write_all(&[0u8; HEADER_SIZE])?;

    // Write placeholder index (will be updated later)
    let index_placeholder = vec![0u8; snapshot_count * INDEX_ENTRY_SIZE];
    file.write_all(&index_placeholder)?;

    // Write dictionary (raw bytes, not zstd compressed)
    let dict_offset = file.stream_position()?;
    let dict_len = dictionary.len() as u64;
    file.write_all(dictionary)?;

    // Compress and write each snapshot WITH dictionary
    let mut compressor = zstd::bulk::Compressor::with_dictionary(3, dictionary)?;
    let mut index_entries: Vec<(u64, u64, i64, u32)> = Vec::with_capacity(snapshot_count);

    for i in 0..snapshot_count {
        let (raw, timestamp) = get_raw_snapshot(i)?;
        let offset = file.stream_position()?;
        let compressed = compressor.compress(&raw)?;
        file.write_all(&compressed)?;
        index_entries.push((offset, compressed.len() as u64, timestamp, raw.len() as u32));
    }

    // Write interner frame (without dictionary)
    let interner_offset = file.stream_position()?;
    let raw_interner = postcard::to_allocvec(interner).map_err(io::Error::other)?;
    let compressed_interner = zstd::encode_all(&raw_interner[..], 3)?;
    let interner_compressed_len = compressed_interner.len() as u64;
    file.write_all(&compressed_interner)?;

    // Seek back and write real header
    file.seek(SeekFrom::Start(0))?;

    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&MAGIC);
    header[4..6].copy_from_slice(&VERSION.to_le_bytes());
    header[6..8].copy_from_slice(&count.to_le_bytes());
    header[8..16].copy_from_slice(&interner_offset.to_le_bytes());
    header[16..24].copy_from_slice(&interner_compressed_len.to_le_bytes());
    header[24..32].copy_from_slice(&dict_offset.to_le_bytes());
    header[32..40].copy_from_slice(&dict_len.to_le_bytes());
    file.write_all(&header)?;

    // Write real index
    for (offset, compressed_len, timestamp, uncompressed_len) in &index_entries {
        file.write_all(&offset.to_le_bytes())?;
        file.write_all(&compressed_len.to_le_bytes())?;
        file.write_all(&timestamp.to_le_bytes())?;
        file.write_all(&uncompressed_len.to_le_bytes())?;
    }

    file.sync_all()?;
    drop(file);

    // Atomic rename
    fs::rename(tmp_path, path)?;

    Ok(())
}

/// Writes snapshots and interner in the chunk format with dictionary compression.
///
/// A zstd dictionary is trained on all snapshots, then each snapshot is compressed
/// with that dictionary for O(1) random access with cross-snapshot redundancy.
/// The interner is compressed without the dictionary.
///
/// The file is written atomically via a `.tmp` intermediate file.
pub fn write_chunk(
    path: &Path,
    snapshots: &[Snapshot],
    interner: &StringInterner,
) -> io::Result<()> {
    // Serialize all snapshots to postcard
    let raw_snapshots: Vec<Vec<u8>> = snapshots
        .iter()
        .map(|s| postcard::to_allocvec(s).map_err(io::Error::other))
        .collect::<Result<_, _>>()?;

    // Train dictionary on all serialized snapshots.
    // Fall back to empty dictionary (= regular zstd) if training fails
    // (e.g. too few or too small samples for meaningful dictionary).
    let dictionary = zstd::dict::from_samples(&raw_snapshots, DICT_MAX_SIZE).unwrap_or_default();

    write_chunk_inner(
        path,
        snapshots.len(),
        &dictionary,
        |i| Ok((raw_snapshots[i].clone(), snapshots[i].timestamp)),
        interner,
    )
}

/// Writes a chunk file using a pre-trained dictionary and a callback that loads
/// snapshots one at a time. This avoids holding all snapshots in memory simultaneously.
///
/// The `load_snapshot` callback receives the snapshot index (0-based) and must return
/// the snapshot at that position. Snapshots are serialized, compressed with the provided
/// dictionary, and written sequentially. The file is written atomically via a `.tmp`
/// intermediate file.
pub fn write_chunk_with_trained_dict<F>(
    path: &Path,
    snapshot_count: usize,
    dictionary: &[u8],
    mut load_snapshot: F,
    interner: &StringInterner,
) -> io::Result<()>
where
    F: FnMut(usize) -> io::Result<Snapshot>,
{
    write_chunk_inner(
        path,
        snapshot_count,
        dictionary,
        |i| {
            let snapshot = load_snapshot(i)?;
            let raw = postcard::to_allocvec(&snapshot).map_err(io::Error::other)?;
            Ok((raw, snapshot.timestamp))
        },
        interner,
    )
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

    #[test]
    fn test_write_chunk_and_trained_dict_produce_identical_data() {
        let dir = tempdir().unwrap();
        let path_a = dir.path().join("a.zst");
        let path_b = dir.path().join("b.zst");
        let snapshots = create_test_snapshots(10);
        let interner = StringInterner::new();

        // Path A: write_chunk (serializes all + trains dict internally)
        write_chunk(&path_a, &snapshots, &interner).unwrap();

        // Path B: write_chunk_with_trained_dict (pre-train dict, callback)
        let raw_snapshots: Vec<Vec<u8>> = snapshots
            .iter()
            .map(|s| postcard::to_allocvec(s).unwrap())
            .collect();
        let dictionary =
            zstd::dict::from_samples(&raw_snapshots, DICT_MAX_SIZE).unwrap_or_default();
        write_chunk_with_trained_dict(
            &path_b,
            snapshots.len(),
            &dictionary,
            |i| Ok(snapshots[i].clone()),
            &interner,
        )
        .unwrap();

        // Both files should produce identical readable data
        let reader_a = ChunkReader::open(&path_a).unwrap();
        let reader_b = ChunkReader::open(&path_b).unwrap();

        assert_eq!(reader_a.snapshot_count(), reader_b.snapshot_count());
        assert_eq!(reader_a.timestamps(), reader_b.timestamps());

        for i in 0..reader_a.snapshot_count() {
            let snap_a = reader_a.read_snapshot(i).unwrap();
            let snap_b = reader_b.read_snapshot(i).unwrap();
            assert_eq!(snap_a, snap_b, "snapshot {} differs", i);
        }
    }

    #[test]
    fn test_dictionary_compression_reduces_size() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.zst");

        // Create many similar snapshots (simulating real workload)
        let snapshots: Vec<Snapshot> = (0..50)
            .map(|i| Snapshot {
                timestamp: 1000 + i as i64 * 10,
                blocks: vec![DataBlock::Processes(
                    (0..100)
                        .map(|p| ProcessInfo {
                            pid: p as u32,
                            ppid: 1,
                            uid: 1000,
                            euid: 1000,
                            name_hash: 42,
                            cmdline_hash: 43,
                            num_threads: 4,
                            ..ProcessInfo::default()
                        })
                        .collect(),
                )],
            })
            .collect();
        let interner = StringInterner::new();

        write_chunk(&path, &snapshots, &interner).unwrap();

        let file_size = std::fs::metadata(&path).unwrap().len();
        let total_raw: usize = snapshots
            .iter()
            .map(|s| postcard::to_allocvec(s).unwrap().len())
            .sum();

        // Dictionary compression should achieve at least 3x ratio on similar data
        assert!(
            file_size < total_raw as u64 / 3,
            "file_size={file_size}, total_raw={total_raw}, ratio={}",
            total_raw as f64 / file_size as f64
        );

        // Verify all snapshots read back correctly
        let reader = ChunkReader::open(&path).unwrap();
        assert_eq!(reader.snapshot_count(), 50);
        for i in 0..50 {
            let snap = reader.read_snapshot(i).unwrap();
            assert_eq!(snap.timestamp, 1000 + i as i64 * 10);
        }
    }
}
