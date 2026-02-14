# Storage Architecture

This document describes the storage subsystem for rpglot, designed to efficiently store system metrics snapshots with SIGKILL resilience.

## Overview

The storage system implements a **Chunk-based Log with Delta-encoding** approach, optimized for:
- Minimal storage size (Zstd compression + delta encoding)
- SIGKILL resilience (WAL + atomic writes)
- String deduplication (XXH3 hashing)
- Easy navigation between snapshots

## Directory Structure

```
src/storage/
├── mod.rs              # Module exports
├── storage.md          # This documentation
├── interner.rs         # String deduplication via XXH3 hashing
├── chunk.rs            # Chunk compression/decompression (Zstd)
├── manager.rs          # StorageManager: WAL, chunks, delta computation
└── model/
    ├── mod.rs          # Model re-exports
    ├── process.rs      # Per-process metrics from /proc/[pid]/
    ├── system.rs       # System-wide metrics from /proc/*
    ├── postgres.rs     # PostgreSQL metrics (pg_stat_activity, etc.)
    └── snapshot.rs     # Snapshot, Delta, DataBlock definitions
```

## Core Components

### 1. StringInterner (`interner.rs`)

Deduplicates long strings (process names, command lines, device names) by storing them once and referencing by XXH3 hash.

```rust
let mut interner = StringInterner::new();
let hash = interner.intern("/usr/bin/very-long-command --with-many-args");
// hash is a u64, stored instead of the full string
```

### 2. Snapshot (`model/snapshot.rs`)

A single point-in-time capture of system state, containing multiple `DataBlock` types:

| DataBlock Type | Source | Description |
|---------------|--------|-------------|
| `Processes` | `/proc/[pid]/` | Per-process metrics (memory, CPU, disk I/O) |
| `SystemCpu` | `/proc/stat` | Per-CPU counters (user, system, idle, etc.) |
| `SystemLoad` | `/proc/loadavg` | Load averages and running processes |
| `SystemMem` | `/proc/meminfo` | Memory statistics |
| `SystemNet` | `/proc/net/dev` | Per-interface network counters |
| `SystemDisk` | `/proc/diskstats` | Per-device I/O counters |
| `SystemPsi` | `/proc/pressure/*` | Pressure Stall Information |
| `SystemVmstat` | `/proc/vmstat` | VM statistics (faults, swap, etc.) |
| `SystemFile` | `/proc/sys/fs/*` | File descriptor statistics |
| `SystemInterrupts` | `/proc/interrupts` | Hardware interrupt counters |
| `SystemSoftirqs` | `/proc/softirqs` | Software interrupt counters |
| `SystemStat` | `/proc/stat` | Context switches, processes created |
| `SystemNetSnmp` | `/proc/net/snmp` | TCP/UDP protocol statistics |
| `PgStatActivity` | `pg_stat_activity` | PostgreSQL active connections |
| `PgStatStatements` | `pg_stat_statements` | PostgreSQL query statistics |
| `Cgroup` | `/sys/fs/cgroup/*` | Container resource limits and usage (cgroup v2) |

### 3. Delta Encoding (`model/snapshot.rs`)

Between full snapshots, only changes are stored:

```rust
enum Delta {
    Full(Snapshot),           // Complete snapshot (first in chunk)
    Diff {
        timestamp: i64,
        blocks: Vec<DataBlockDiff>,  // Only changed/new/removed items
    },
}
```

For list-based data (processes, network interfaces), diff tracks:
- `updates`: New or changed items
- `removals`: IDs of removed items

### 4. Chunk (`chunk.rs`)

Groups multiple deltas and compresses them with Zstd:

```rust
struct Chunk {
    interner: StringInterner,  // String table for this chunk
    deltas: Vec<Delta>,        // First is Full, rest are Diff
}
```

Typical chunk size: up to 360 snapshots (~1 hour at 10-second intervals). Chunks are flushed on hour boundaries or when size limit is reached.

### 5. StorageManager (`manager.rs`)

Coordinates all operations with a **WAL-only architecture** — no chunks are kept in memory:

```
┌─────────────────────────────────────────────────────────────┐
│                     StorageManager                          │
├─────────────────────────────────────────────────────────────┤
│  add_snapshot(snapshot)                                     │
│    1. Write to WAL (sync to disk)                          │
│    2. Increment wal_entries_count                          │
│    3. If hour changed or size limit → flush_chunk()        │
├─────────────────────────────────────────────────────────────┤
│  flush_chunk()                                              │
│    1. Read all snapshots from WAL                          │
│    2. Compute deltas on-the-fly                            │
│    3. Build chunk with filtered interner                   │
│    4. Compress with Zstd                                   │
│    5. Write to temp file (.tmp)                            │
│    6. Rename to final file (.zst) ← atomic                 │
│    7. Truncate WAL, reset entry count                      │
├─────────────────────────────────────────────────────────────┤
│  load_all_snapshots()                                       │
│    1. Load all .zst chunk files                            │
│    2. Reconstruct snapshots from deltas                    │
│    3. Load unflushed snapshots from WAL                    │
│    4. Sort by timestamp, deduplicate                       │
│    5. Return all snapshots (chronological order)           │
├─────────────────────────────────────────────────────────────┤
│  recover_from_wal()                                         │
│    Count valid entries in WAL, truncate corruption         │
└─────────────────────────────────────────────────────────────┘
```

**Memory efficiency**: The WAL-only approach means StorageManager uses ~0 bytes for chunk storage. All snapshots are written directly to disk (WAL), and delta computation happens only during flush.

### 6. RotationConfig (`manager.rs`)

Configuration for automatic data rotation:

```rust
pub struct RotationConfig {
    pub max_total_size: u64,      // Default: 1GB (1_073_741_824)
    pub max_retention_days: u32,  // Default: 7 days
}
```

## File Format

### WAL File (`wal.log`)

Sequential bincode-serialized `WalEntry` records. Each entry is self-contained:

```rust
struct WalEntry {
    snapshot: Snapshot,
    interner: StringInterner,  // Only hashes used in this snapshot
}
```

**Key features:**
- Each WAL entry contains its own minimal interner (only strings used in that snapshot)
- WAL is self-contained for recovery — no external files needed
- Cleared after successful chunk flush

**WAL Corruption Handling:**
- On recovery, WAL is read until deserialization fails
- Corrupted/partial data at the end is truncated
- Valid records before corruption are recovered
- Warning is logged if corruption is detected

**Reading WAL:**
- `load_all_snapshots()` reads unflushed snapshots from WAL
- `load_wal_snapshots_with_interner()` also merges interners from all entries
- Used by `rpglot -r` to display latest data even before flush
- Snapshots are deduplicated by timestamp when combined with chunks

### Chunk Files (`rpglot_YYYY-MM-DD_HH.zst`)

Hourly segmented files with Zstd-compressed bincode-serialized `Chunk` structures.

**Naming convention:**
- `rpglot_2026-02-07_17.zst` — data for hour 17:00-17:59 on 2026-02-07
- Files are created when the hour changes (automatic flush)
- Legacy format `chunk_<timestamp>.zst` is also supported for reading

## Hourly File Segmentation

The storage system automatically segments data by hour:

1. **Hour boundary detection**: When `add_snapshot()` is called, it checks if the current hour differs from the previous snapshot's hour
2. **Automatic flush**: If hour changed, the current chunk is flushed before adding the new snapshot
3. **File naming**: Files are named `rpglot_YYYY-MM-DD_HH.zst` based on the hour they contain

This enables:
- Easy identification of data by time period
- Efficient rotation (delete files by date from filename)
- Independent chunks for each hour

## Data Rotation

The `rotate()` method removes old data files based on two criteria:

```
┌─────────────────────────────────────────────────────────────┐
│                     rotate(config)                          │
├─────────────────────────────────────────────────────────────┤
│  1. Collect all .zst files with dates from filenames       │
│  2. Sort by date (oldest first)                            │
│  3. Remove files older than max_retention_days             │
│  4. If total size > max_total_size:                        │
│     Remove oldest files until under limit                   │
│  5. Return RotationResult with statistics                  │
└─────────────────────────────────────────────────────────────┘
```

**RotationResult fields:**
| Field | Description |
|-------|-------------|
| `files_removed_by_age` | Files removed due to retention limit |
| `files_removed_by_size` | Files removed due to size limit |
| `bytes_freed` | Total bytes freed |
| `total_size_after` | Size of remaining files |
| `files_remaining` | Number of remaining files |

**rpglotd rotation behavior:**
- Initial rotation on daemon startup
- Periodic rotation when hour changes
- Configurable via `--max-size` and `--max-days` CLI arguments

## SIGKILL Resilience

1. **Every snapshot is written to WAL first** with `sync_all()`
   - WAL entry includes snapshot AND its string interner
   - No external files needed for recovery
2. **Chunk writes are atomic**: write to `.tmp`, then `rename()`
3. **WAL corruption handling**:
   - If SIGKILL occurs during write, partial record at end is detected
   - On startup, corrupted bytes are truncated from WAL
   - All valid records before corruption are recovered
4. **On startup**: 
   - Delete orphan `.tmp` files
   - Delete legacy `strings.bin` if exists (migration)
   - Replay WAL to recover unflushed snapshots

## Memory Management

The storage system uses a **WAL-only architecture** that eliminates in-memory chunk accumulation:

### WAL-Only Design

StorageManager does NOT keep chunks in memory:
- Snapshots are written directly to WAL on disk
- Delta computation happens only during flush (reading WAL back)
- This eliminates ~3-7 MB of memory that would be used for chunk storage

### Chunk Size Limit

The `chunk_size_limit` is set to **360 entries** (~1 hour at 10-second intervals). Chunks are also flushed on hour boundaries for time-based file organization.

### String Interner Optimization

1. **Per-chunk filtering**: During flush, the interner is built from WAL entries and filtered to keep only strings actually used
2. **Collector interner clearing**: After each chunk flush, the daemon clears the collector's interner (via `Collector.clear_interner()`)
3. **WAL entry optimization**: Each WAL entry contains only strings used in that specific snapshot
4. **shrink_to_fit()**: On interner clear, `shrink_to_fit()` is called to release capacity

### jemalloc Memory Release

After each chunk flush, `release_memory_to_os()` is called to return unused pages to the OS via jemalloc's arena purge.

### PostgreSQL Statements Cache

The `pg_stat_statements` cache is limited to **MAX_CACHED_STATEMENTS = 1000** entries to prevent unbounded growth when there are many unique queries.

### Memory Monitoring

Every 60 snapshots (~10 minutes), the daemon logs memory statistics:
```
Memory stats: collector_interner=X strings, wal_entries=Y
```

This ensures:
- Near-zero memory growth from storage (only collector interner grows)
- Chunk files have minimal size (no unused strings)
- WAL entries are self-contained but not bloated
- RSS is returned to OS after memory-intensive operations

## Data Sources Summary

### Process Metrics (`/proc/[pid]/`)

| File | Fields |
|------|--------|
| `stat` | pid, ppid, tty, utime, stime, nice, priority, vsize, rss, starttime, processor, rt_priority, policy, blkdelay |
| `status` | uid, gid, VmData, VmStk, VmLib, VmSwap, VmLck, context switches |
| `io` | read_bytes, write_bytes, syscr, syscw, cancelled_write_bytes |
| `comm` | process name (interned) |
| `cmdline` | command line (interned) |
| `wchan` | wait channel (interned) |
| `schedstat` | run_delay |
| `smaps` | Pss (proportional set size) |
| `maps` | executable memory size |

**Process Start Time (`btime`):**
The `ProcessInfo.btime` field contains the process start time in seconds since epoch.
It is calculated as: `btime = boot_time + (starttime / CLK_TCK)` where:
- `boot_time` is from `/proc/stat` (btime field)
- `starttime` is from `/proc/[pid]/stat` field 22 (time in jiffies since boot)
- `CLK_TCK = 100` (standard Linux clock ticks per second)

### System Metrics

| Source | Key Fields |
|--------|------------|
| `/proc/stat` | per-CPU jiffies, context switches, processes, boot time |
| `/proc/loadavg` | load averages, running/total threads |
| `/proc/meminfo` | total, free, available, buffers, cached, swap, etc. |
| `/proc/vmstat` | pgfault, pgmajfault, pswpin, pswpout, oom_kill |
| `/proc/diskstats` | reads, writes, sectors, I/O time per device |
| `/proc/net/dev` | bytes, packets, errors, drops per interface |
| `/proc/net/snmp` | TCP/UDP protocol counters |
| `/proc/pressure/*` | some/full avg10/60/300, total stall time |
| `/proc/interrupts` | per-IRQ counters |
| `/proc/softirqs` | per-softirq counters |
| `/proc/sys/fs/*` | file-nr, inode-nr |

## Versioning

**Current version: V1**

When modifying data structures (`Snapshot`, `Delta`, `DataBlock`, or any model struct):
1. Consider backward compatibility implications
2. Update this document (`storage.md`) with changes
3. If breaking changes are needed, implement versioned deserialization

### Backward Compatibility Notes

When adding new fields to existing model structs, prefer using `#[serde(default)]` on the new
fields to allow loading older snapshots that were serialized without them.

Recent PostgreSQL model changes:
- `PgStatActivityInfo` gained `query_id` (defaults to `0` when absent)
- `PgStatStatementsInfo` was extended with additional timing/block/WAL fields; new fields use
  `#[serde(default)]` to keep older snapshot files readable
- `PgStatStatementsInfo` gained `datname_hash` and `usename_hash` (interned database/role names for TUI filtering); these fields also use `#[serde(default)]`
- `PgStatStatementsInfo` gained `collected_at` (Unix timestamp of actual data collection time); used by TUI to calculate accurate rates when collector caches pg_stat_statements (~30s)

## Usage Example

```rust
use storage::{StorageManager, Snapshot, StringInterner};
use storage::model::{DataBlock, ProcessInfo, SystemMemInfo};

let mut manager = StorageManager::new("data");
let mut interner = StringInterner::new();

let snapshot = Snapshot {
    timestamp: chrono::Utc::now().timestamp(),
    blocks: vec![
        DataBlock::Processes(vec![ProcessInfo {
            pid: 1,
            name_hash: interner.intern("systemd"),
            ..Default::default()
        }]),
        DataBlock::SystemMem(SystemMemInfo {
            total: 16 * 1024 * 1024,
            free: 8 * 1024 * 1024,
            ..Default::default()
        }),
    ],
};

manager.add_snapshot(snapshot);
manager.flush_chunk().unwrap();
```
