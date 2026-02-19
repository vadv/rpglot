use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use clap::Parser;
use serde::Serialize;

use rpglot_core::storage::ChunkReader;
use rpglot_core::storage::model::DataBlock;

// ── RPG3 chunk format constants (mirrored from rpglot-core::storage::chunk) ──

const CHUNK_MAGIC: &[u8; 4] = b"RPG3";
const CHUNK_HEADER_SIZE: usize = 48;
const INDEX_ENTRY_SIZE: usize = 28;

// ── WAL format constants (mirrored from rpglot-core::storage::manager) ───────

const WAL_FRAME_HEADER_SIZE: usize = 8;
const MAX_WAL_ENTRY_SIZE: u32 = 256 * 1024 * 1024;

// ── Heatmap format constants ─────────────────────────────────────────────────

const HEATMAP_MAGIC: &[u8; 4] = b"HM03";
const HEATMAP_ENTRY_SIZE: usize = 14;

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "rpglotd-dump", about = "Inspect rpglot storage files")]
struct Cli {
    /// Path to .zst, .heatmap, wal.log, or storage directory
    path: Option<PathBuf>,

    /// Show per-DataBlock size breakdown (requires decompression)
    #[arg(long)]
    blocks: bool,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

fn main() {
    let cli = Cli::parse();
    let path = cli.path.clone().unwrap_or_else(|| PathBuf::from("."));

    if path.is_dir() {
        dump_directory(&path, &cli);
    } else if has_ext(&path, "zst") {
        dump_chunk(&path, &cli);
    } else if has_ext(&path, "heatmap") {
        dump_heatmap(&path, &cli);
    } else if path.file_name().and_then(|f| f.to_str()) == Some("wal.log") {
        dump_wal(&path, &cli);
    } else {
        eprintln!("Unknown file type: {}", path.display());
        std::process::exit(1);
    }
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension().and_then(OsStr::to_str) == Some(ext)
}

// ── Formatting helpers ───────────────────────────────────────────────────────

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn fmt_ts(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn fmt_time_only(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn pct(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64 * 100.0
    }
}

// ── DataBlock info ───────────────────────────────────────────────────────────

fn block_info(block: &DataBlock) -> (&'static str, usize) {
    match block {
        DataBlock::Processes(v) => ("Processes", v.len()),
        DataBlock::PgStatActivity(v) => ("PgStatActivity", v.len()),
        DataBlock::PgStatStatements(v) => ("PgStatStatements", v.len()),
        DataBlock::PgStatDatabase(v) => ("PgStatDatabase", v.len()),
        DataBlock::PgStatUserTables(v) => ("PgStatUserTables", v.len()),
        DataBlock::PgStatUserIndexes(v) => ("PgStatUserIndexes", v.len()),
        DataBlock::PgLockTree(v) => ("PgLockTree", v.len()),
        DataBlock::PgStatBgwriter(_) => ("PgStatBgwriter", 1),
        DataBlock::PgStatProgressVacuum(v) => ("PgStatProgressVacuum", v.len()),
        DataBlock::PgLogErrors(v) => ("PgLogErrors", v.len()),
        DataBlock::PgLogEvents(_) => ("PgLogEvents", 1),
        DataBlock::PgLogDetailedEvents(v) => ("PgLogDetailedEvents", v.len()),
        DataBlock::PgSettings(v) => ("PgSettings", v.len()),
        DataBlock::SystemCpu(v) => ("SystemCpu", v.len()),
        DataBlock::SystemLoad(_) => ("SystemLoad", 1),
        DataBlock::SystemMem(_) => ("SystemMem", 1),
        DataBlock::SystemNet(v) => ("SystemNet", v.len()),
        DataBlock::SystemDisk(v) => ("SystemDisk", v.len()),
        DataBlock::SystemPsi(v) => ("SystemPsi", v.len()),
        DataBlock::SystemVmstat(_) => ("SystemVmstat", 1),
        DataBlock::SystemFile(_) => ("SystemFile", 1),
        DataBlock::SystemInterrupts(v) => ("SystemInterrupts", v.len()),
        DataBlock::SystemSoftirqs(v) => ("SystemSoftirqs", v.len()),
        DataBlock::SystemStat(_) => ("SystemStat", 1),
        DataBlock::SystemNetSnmp(_) => ("SystemNetSnmp", 1),
        DataBlock::Cgroup(_) => ("Cgroup", 1),
        DataBlock::ReplicationStatus(_) => ("ReplicationStatus", 1),
    }
}

/// Accumulator for per-block-type stats across multiple snapshots.
#[derive(Default)]
struct BlockStats {
    count: u64,
    total_items: u64,
    total_postcard_bytes: u64,
}

fn collect_block_stats(
    snapshots: impl Iterator<Item = rpglot_core::storage::Snapshot>,
) -> (BTreeMap<&'static str, BlockStats>, u64) {
    let mut stats: BTreeMap<&'static str, BlockStats> = BTreeMap::new();
    let mut snap_count = 0u64;
    for snap in snapshots {
        snap_count += 1;
        for block in &snap.blocks {
            let (name, items) = block_info(block);
            let postcard_size = postcard::to_allocvec(block).map(|v| v.len()).unwrap_or(0);
            let entry = stats.entry(name).or_default();
            entry.count += 1;
            entry.total_items += items as u64;
            entry.total_postcard_bytes += postcard_size as u64;
        }
    }
    (stats, snap_count)
}

fn print_block_stats(stats: &BTreeMap<&'static str, BlockStats>, snap_count: u64) {
    if snap_count == 0 {
        return;
    }
    let total_bytes: u64 = stats.values().map(|s| s.total_postcard_bytes).sum();
    println!("\nDataBlock breakdown (avg per snapshot):");
    println!(
        "  {:<28} {:>6} {:>10} {:>6}",
        "Block", "Items", "Postcard", "Share"
    );
    println!("  {}", "─".repeat(56));
    for (name, s) in stats {
        let avg_items = s.total_items as f64 / snap_count as f64;
        let avg_bytes = s.total_postcard_bytes as f64 / snap_count as f64;
        let share = pct(s.total_postcard_bytes, total_bytes);
        println!(
            "  {:<28} {:>6.0} {:>9.0}B {:>5.1}%",
            name, avg_items, avg_bytes, share
        );
    }
    let avg_total = total_bytes as f64 / snap_count as f64;
    println!("  {}", "─".repeat(56));
    println!(
        "  {:<28} {:>6} {:>9.0}B {:>5.1}%",
        "TOTAL", "", avg_total, 100.0
    );
}

// ── JSON output types ────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChunkJson {
    file: String,
    file_size: u64,
    format: String,
    snapshot_count: usize,
    sections: ChunkSectionsJson,
    time_range: Option<TimeRangeJson>,
    compressed: StatsJson,
    uncompressed: StatsJson,
    ratio_avg: f64,
    interner_strings: Option<usize>,
    interner_compressed_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocks: Option<Vec<BlockJson>>,
}

#[derive(Serialize)]
struct ChunkSectionsJson {
    header_and_index: u64,
    dictionary: u64,
    snapshot_frames: u64,
    interner_frame: u64,
}

#[derive(Serialize)]
struct TimeRangeJson {
    first: String,
    last: String,
}

#[derive(Serialize)]
struct StatsJson {
    avg: u64,
    min: u64,
    max: u64,
}

#[derive(Serialize)]
struct BlockJson {
    name: String,
    avg_items: f64,
    avg_postcard_bytes: f64,
    share_pct: f64,
}

#[derive(Serialize)]
struct WalJson {
    file: String,
    file_size: u64,
    format: String,
    entries: usize,
    frame_stats: StatsJson,
    time_range: Option<TimeRangeJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocks: Option<Vec<BlockJson>>,
}

#[derive(Serialize)]
struct HeatmapJson {
    file: String,
    file_size: u64,
    format: String,
    entries: usize,
}

#[derive(Serialize)]
struct DirectoryJson {
    path: String,
    chunks: FileGroupJson,
    heatmaps: FileGroupJson,
    wal: Option<FileGroupJson>,
    total_size: u64,
    time_range: Option<TimeRangeJson>,
    snapshot_count_chunks: u64,
    snapshot_count_wal: u64,
}

#[derive(Serialize)]
struct FileGroupJson {
    count: usize,
    total_size: u64,
}

fn blocks_to_json(stats: &BTreeMap<&'static str, BlockStats>, snap_count: u64) -> Vec<BlockJson> {
    let total_bytes: u64 = stats.values().map(|s| s.total_postcard_bytes).sum();
    stats
        .iter()
        .map(|(name, s)| BlockJson {
            name: name.to_string(),
            avg_items: s.total_items as f64 / snap_count.max(1) as f64,
            avg_postcard_bytes: s.total_postcard_bytes as f64 / snap_count.max(1) as f64,
            share_pct: pct(s.total_postcard_bytes, total_bytes),
        })
        .collect()
}

// ── Chunk header parsing (no decompression) ──────────────────────────────────

#[allow(dead_code)]
struct ChunkHeader {
    snapshot_count: usize,
    interner_offset: u64,
    interner_compressed_len: u64,
    dict_offset: u64,
    dict_len: u64,
}

#[allow(dead_code)]
struct IndexEntry {
    offset: u64,
    compressed_len: u64,
    timestamp: i64,
    uncompressed_len: u32,
}

fn parse_chunk_header(data: &[u8]) -> io::Result<ChunkHeader> {
    if data.len() < CHUNK_HEADER_SIZE {
        return Err(io::Error::other("file too small for RPG3 header"));
    }
    if &data[0..4] != CHUNK_MAGIC {
        return Err(io::Error::other(format!(
            "invalid magic: expected RPG3, got {:?}",
            &data[0..4]
        )));
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != 3 {
        return Err(io::Error::other(format!(
            "unsupported chunk version: {version}"
        )));
    }
    Ok(ChunkHeader {
        snapshot_count: u16::from_le_bytes([data[6], data[7]]) as usize,
        interner_offset: u64::from_le_bytes(data[8..16].try_into().unwrap()),
        interner_compressed_len: u64::from_le_bytes(data[16..24].try_into().unwrap()),
        dict_offset: u64::from_le_bytes(data[24..32].try_into().unwrap()),
        dict_len: u64::from_le_bytes(data[32..40].try_into().unwrap()),
    })
}

fn parse_index(data: &[u8], count: usize) -> Vec<IndexEntry> {
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let base = CHUNK_HEADER_SIZE + i * INDEX_ENTRY_SIZE;
        entries.push(IndexEntry {
            offset: u64::from_le_bytes(data[base..base + 8].try_into().unwrap()),
            compressed_len: u64::from_le_bytes(data[base + 8..base + 16].try_into().unwrap()),
            timestamp: i64::from_le_bytes(data[base + 16..base + 24].try_into().unwrap()),
            uncompressed_len: u32::from_le_bytes(data[base + 24..base + 28].try_into().unwrap()),
        });
    }
    entries
}

// ── dump_chunk ───────────────────────────────────────────────────────────────

fn dump_chunk(path: &Path, cli: &Cli) {
    let data = fs::read(path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {e}", path.display());
        std::process::exit(1);
    });
    let file_size = data.len() as u64;

    let header = parse_chunk_header(&data).unwrap_or_else(|e| {
        eprintln!("Error parsing chunk header: {e}");
        std::process::exit(1);
    });

    let min_data = CHUNK_HEADER_SIZE + header.snapshot_count * INDEX_ENTRY_SIZE;
    if data.len() < min_data {
        eprintln!("File truncated: need at least {min_data} bytes for header+index");
        std::process::exit(1);
    }

    let index = parse_index(&data, header.snapshot_count);

    // Section sizes
    let header_index_size = (CHUNK_HEADER_SIZE + header.snapshot_count * INDEX_ENTRY_SIZE) as u64;
    let snapshot_frames_size: u64 = index.iter().map(|e| e.compressed_len).sum();

    // Compression stats from index
    let compressed: Vec<u64> = index.iter().map(|e| e.compressed_len).collect();
    let uncompressed: Vec<u64> = index.iter().map(|e| e.uncompressed_len as u64).collect();
    let timestamps: Vec<i64> = index.iter().map(|e| e.timestamp).collect();

    // Block stats (optional, requires decompression)
    let block_data = if cli.blocks {
        let reader = ChunkReader::open(path).unwrap_or_else(|e| {
            eprintln!("Error opening chunk for block analysis: {e}");
            std::process::exit(1);
        });
        let interner = reader.read_interner().unwrap_or_else(|e| {
            eprintln!("Error reading interner: {e}");
            std::process::exit(1);
        });
        let snapshots = (0..reader.snapshot_count()).map(|i| {
            reader.read_snapshot(i).unwrap_or_else(|e| {
                eprintln!("Error reading snapshot {i}: {e}");
                std::process::exit(1);
            })
        });
        let (stats, snap_count) = collect_block_stats(snapshots);
        Some((stats, snap_count, interner.len()))
    } else {
        None
    };

    if cli.json {
        let blocks_json = block_data
            .as_ref()
            .map(|(stats, snap_count, _)| blocks_to_json(stats, *snap_count));
        let interner_strings = block_data.as_ref().map(|(_, _, len)| *len);

        let json = ChunkJson {
            file: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            file_size,
            format: "RPG3 v3".into(),
            snapshot_count: header.snapshot_count,
            sections: ChunkSectionsJson {
                header_and_index: header_index_size,
                dictionary: header.dict_len,
                snapshot_frames: snapshot_frames_size,
                interner_frame: header.interner_compressed_len,
            },
            time_range: if !timestamps.is_empty() {
                Some(TimeRangeJson {
                    first: fmt_ts(timestamps[0]),
                    last: fmt_ts(*timestamps.last().unwrap()),
                })
            } else {
                None
            },
            compressed: stats_json(&compressed),
            uncompressed: stats_json(&uncompressed),
            ratio_avg: avg_ratio(&compressed, &uncompressed),
            interner_strings,
            interner_compressed_bytes: header.interner_compressed_len,
            blocks: blocks_json,
        };
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        println!("File: {} ({})", fname, human_bytes(file_size));
        println!("Format: RPG3 v3, {} snapshots", header.snapshot_count);

        println!("\nSections:");
        println!(
            "  Header + Index     {} + {} = {} ({:.1}%)",
            CHUNK_HEADER_SIZE,
            header.snapshot_count * INDEX_ENTRY_SIZE,
            header_index_size,
            pct(header_index_size, file_size)
        );
        println!(
            "  Dictionary         {} ({:.1}%)",
            human_bytes(header.dict_len),
            pct(header.dict_len, file_size)
        );
        println!(
            "  Snapshot frames    {} ({:.1}%)",
            human_bytes(snapshot_frames_size),
            pct(snapshot_frames_size, file_size)
        );
        println!(
            "  Interner frame     {} ({:.1}%)",
            human_bytes(header.interner_compressed_len),
            pct(header.interner_compressed_len, file_size)
        );

        if !timestamps.is_empty() {
            println!(
                "\nSnapshots: {}, time range {} \u{2013} {}",
                header.snapshot_count,
                fmt_time_only(timestamps[0]),
                fmt_time_only(*timestamps.last().unwrap())
            );
        }

        if !compressed.is_empty() {
            println!(
                "  Compressed:   avg {} B, min {} B, max {} B",
                avg_u64(&compressed),
                compressed.iter().min().unwrap(),
                compressed.iter().max().unwrap()
            );
            println!(
                "  Uncompressed: avg {} B, min {} B, max {} B",
                avg_u64(&uncompressed),
                uncompressed.iter().min().unwrap(),
                uncompressed.iter().max().unwrap()
            );
            println!(
                "  Ratio:        avg {:.1}x",
                avg_ratio(&compressed, &uncompressed)
            );
        }

        if let Some((ref stats, snap_count, interner_len)) = block_data {
            println!(
                "\nInterner: {} strings, compressed {}",
                interner_len,
                human_bytes(header.interner_compressed_len)
            );
            print_block_stats(stats, snap_count);
        }
    }
}

fn avg_u64(v: &[u64]) -> u64 {
    if v.is_empty() {
        0
    } else {
        v.iter().sum::<u64>() / v.len() as u64
    }
}

fn avg_ratio(compressed: &[u64], uncompressed: &[u64]) -> f64 {
    let total_c: u64 = compressed.iter().sum();
    let total_u: u64 = uncompressed.iter().sum();
    if total_c == 0 {
        0.0
    } else {
        total_u as f64 / total_c as f64
    }
}

fn stats_json(values: &[u64]) -> StatsJson {
    StatsJson {
        avg: avg_u64(values),
        min: values.iter().copied().min().unwrap_or(0),
        max: values.iter().copied().max().unwrap_or(0),
    }
}

// ── dump_wal ─────────────────────────────────────────────────────────────────

fn dump_wal(path: &Path, cli: &Cli) {
    let data = fs::read(path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {e}", path.display());
        std::process::exit(1);
    });
    let file_size = data.len() as u64;

    if data.is_empty() {
        if cli.json {
            let json = WalJson {
                file: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                file_size: 0,
                format: "CRC32-framed WAL".into(),
                entries: 0,
                frame_stats: StatsJson {
                    avg: 0,
                    min: 0,
                    max: 0,
                },
                time_range: None,
                blocks: None,
            };
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        } else {
            println!("File: wal.log (empty)");
        }
        return;
    }

    // Parse WAL frames
    let mut pos = 0usize;
    let mut frame_sizes: Vec<u64> = Vec::new();
    let mut timestamps: Vec<i64> = Vec::new();
    let mut snapshots: Vec<rpglot_core::storage::Snapshot> = Vec::new();

    while pos + WAL_FRAME_HEADER_SIZE <= data.len() {
        let length = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        let crc = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap());

        if length > MAX_WAL_ENTRY_SIZE || pos + WAL_FRAME_HEADER_SIZE + length as usize > data.len()
        {
            break;
        }

        let payload =
            &data[pos + WAL_FRAME_HEADER_SIZE..pos + WAL_FRAME_HEADER_SIZE + length as usize];
        let actual_crc = crc32fast::hash(payload);
        if actual_crc != crc {
            break;
        }

        let frame_total = WAL_FRAME_HEADER_SIZE as u64 + length as u64;
        frame_sizes.push(frame_total);

        // Deserialize to get timestamp (and optionally snapshot for --blocks)
        if let Ok(entry) = postcard::from_bytes::<WalEntryView>(payload) {
            timestamps.push(entry.snapshot.timestamp);
            if cli.blocks {
                snapshots.push(entry.snapshot);
            }
        } else {
            break;
        }

        pos += WAL_FRAME_HEADER_SIZE + length as usize;
    }

    let block_data = if cli.blocks && !snapshots.is_empty() {
        let (stats, snap_count) = collect_block_stats(snapshots.into_iter());
        Some((stats, snap_count))
    } else {
        None
    };

    if cli.json {
        let json = WalJson {
            file: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            file_size,
            format: "CRC32-framed WAL".into(),
            entries: frame_sizes.len(),
            frame_stats: stats_json(&frame_sizes),
            time_range: if !timestamps.is_empty() {
                Some(TimeRangeJson {
                    first: fmt_ts(timestamps[0]),
                    last: fmt_ts(*timestamps.last().unwrap()),
                })
            } else {
                None
            },
            blocks: block_data
                .as_ref()
                .map(|(stats, snap_count)| blocks_to_json(stats, *snap_count)),
        };
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        println!("File: {} ({})", fname, human_bytes(file_size));
        println!("Format: CRC32-framed WAL, {} entries", frame_sizes.len());

        if !frame_sizes.is_empty() {
            println!(
                "\nEntries:\n  Frame sizes: avg {} B, min {} B, max {} B",
                avg_u64(&frame_sizes),
                frame_sizes.iter().min().unwrap(),
                frame_sizes.iter().max().unwrap()
            );
        }

        if !timestamps.is_empty() {
            println!(
                "  Time range: {} \u{2013} {}",
                fmt_time_only(timestamps[0]),
                fmt_time_only(*timestamps.last().unwrap())
            );
        }

        if let Some((ref stats, snap_count)) = block_data {
            print_block_stats(stats, snap_count);
        }
    }
}

/// Minimal deserialization of WalEntry (same layout as rpglot-core's WalEntry).
#[derive(serde::Deserialize)]
struct WalEntryView {
    snapshot: rpglot_core::storage::Snapshot,
    #[allow(dead_code)]
    interner: rpglot_core::storage::StringInterner,
}

// ── dump_heatmap ─────────────────────────────────────────────────────────────

fn dump_heatmap(path: &Path, cli: &Cli) {
    let data = fs::read(path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {e}", path.display());
        std::process::exit(1);
    });
    let file_size = data.len() as u64;

    if data.len() < 4 || &data[0..4] != HEATMAP_MAGIC {
        eprintln!("Invalid heatmap file: bad magic");
        std::process::exit(1);
    }

    let payload_len = data.len() - 4;
    let entries = if payload_len.is_multiple_of(HEATMAP_ENTRY_SIZE) {
        payload_len / HEATMAP_ENTRY_SIZE
    } else {
        eprintln!("Invalid heatmap file: payload size not aligned to entry size");
        std::process::exit(1);
    };

    if cli.json {
        let json = HeatmapJson {
            file: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            file_size,
            format: "HM03".into(),
            entries,
        };
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        println!("File: {} ({} bytes)", fname, file_size);
        println!(
            "Format: HM03, {} entries ({} B each)",
            entries, HEATMAP_ENTRY_SIZE
        );
    }
}

// ── dump_directory ───────────────────────────────────────────────────────────

fn dump_directory(path: &Path, cli: &Cli) {
    let entries = fs::read_dir(path).unwrap_or_else(|e| {
        eprintln!("Error reading directory {}: {e}", path.display());
        std::process::exit(1);
    });

    let mut chunk_files: Vec<PathBuf> = Vec::new();
    let mut heatmap_files: Vec<PathBuf> = Vec::new();
    let mut wal_file: Option<PathBuf> = None;

    for entry in entries.flatten() {
        let p = entry.path();
        if has_ext(&p, "zst") {
            chunk_files.push(p);
        } else if has_ext(&p, "heatmap") {
            heatmap_files.push(p);
        } else if p.file_name().and_then(|f| f.to_str()) == Some("wal.log") {
            wal_file = Some(p);
        }
    }

    chunk_files.sort();
    heatmap_files.sort();

    let chunk_total_size: u64 = chunk_files
        .iter()
        .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    let heatmap_total_size: u64 = heatmap_files
        .iter()
        .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    let wal_size: u64 = wal_file
        .as_ref()
        .and_then(|p| fs::metadata(p).ok().map(|m| m.len()))
        .unwrap_or(0);

    let total_size = chunk_total_size + heatmap_total_size + wal_size;

    // Get time range and snapshot count from chunk indexes (fast, no decompression)
    let mut all_first_ts: Option<i64> = None;
    let mut all_last_ts: Option<i64> = None;
    let mut total_snapshots_chunks: u64 = 0;
    let mut total_snapshots_wal: u64 = 0;

    for chunk_path in &chunk_files {
        if let Ok(data) = fs::read(chunk_path)
            && let Ok(header) = parse_chunk_header(&data)
        {
            let needed = CHUNK_HEADER_SIZE + header.snapshot_count * INDEX_ENTRY_SIZE;
            if data.len() >= needed {
                let index = parse_index(&data, header.snapshot_count);
                total_snapshots_chunks += header.snapshot_count as u64;
                if let Some(first) = index.first() {
                    all_first_ts =
                        Some(all_first_ts.map_or(first.timestamp, |t: i64| t.min(first.timestamp)));
                }
                if let Some(last) = index.last() {
                    all_last_ts =
                        Some(all_last_ts.map_or(last.timestamp, |t: i64| t.max(last.timestamp)));
                }
            }
        }
    }

    // WAL snapshot count (scan metadata without full decompression)
    if let Some(ref wal_path) = wal_file
        && let Ok(data) = fs::read(wal_path)
    {
        let mut pos = 0usize;
        while pos + WAL_FRAME_HEADER_SIZE <= data.len() {
            let length = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
            let crc = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap());
            if length > MAX_WAL_ENTRY_SIZE
                || pos + WAL_FRAME_HEADER_SIZE + length as usize > data.len()
            {
                break;
            }
            let payload =
                &data[pos + WAL_FRAME_HEADER_SIZE..pos + WAL_FRAME_HEADER_SIZE + length as usize];
            if crc32fast::hash(payload) != crc {
                break;
            }
            // Quick timestamp extraction: deserialize just to get timestamp
            if let Ok(entry) = postcard::from_bytes::<WalEntryView>(payload) {
                let ts = entry.snapshot.timestamp;
                all_first_ts = Some(all_first_ts.map_or(ts, |t: i64| t.min(ts)));
                all_last_ts = Some(all_last_ts.map_or(ts, |t: i64| t.max(ts)));
                total_snapshots_wal += 1;
            } else {
                break;
            }
            pos += WAL_FRAME_HEADER_SIZE + length as usize;
        }
    }

    if cli.json {
        let json = DirectoryJson {
            path: path.display().to_string(),
            chunks: FileGroupJson {
                count: chunk_files.len(),
                total_size: chunk_total_size,
            },
            heatmaps: FileGroupJson {
                count: heatmap_files.len(),
                total_size: heatmap_total_size,
            },
            wal: wal_file.as_ref().map(|_| FileGroupJson {
                count: 1,
                total_size: wal_size,
            }),
            total_size,
            time_range: match (all_first_ts, all_last_ts) {
                (Some(f), Some(l)) => Some(TimeRangeJson {
                    first: fmt_ts(f),
                    last: fmt_ts(l),
                }),
                _ => None,
            },
            snapshot_count_chunks: total_snapshots_chunks,
            snapshot_count_wal: total_snapshots_wal,
        };
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        println!("Storage: {}", path.display());
        println!(
            "  Chunks:   {} files, {}",
            chunk_files.len(),
            human_bytes(chunk_total_size)
        );
        println!(
            "  Heatmaps: {} files, {}",
            heatmap_files.len(),
            human_bytes(heatmap_total_size)
        );
        if wal_file.is_some() {
            println!("  WAL:      1 file, {}", human_bytes(wal_size));
        } else {
            println!("  WAL:      none");
        }
        println!("  Total:    {}", human_bytes(total_size));

        if let (Some(first), Some(last)) = (all_first_ts, all_last_ts) {
            println!("  Time range: {} \u{2013} {}", fmt_ts(first), fmt_ts(last));
        }
        println!(
            "  Snapshots: ~{} (chunks) + {} (WAL)",
            total_snapshots_chunks, total_snapshots_wal
        );
    }
}
