//! Lightweight per-snapshot heatmap data for timeline visualization.
//!
//! Each snapshot produces a 14-byte `HeatmapEntry` (active_sessions, host CPU%,
//! cgroup CPU%, cgroup memory%, errors by severity, checkpoint/autovacuum/slow counts).
//! These are stored in `.heatmap` sidecar files alongside `.zst` chunk files
//! and read without decompressing snapshots — enabling O(1) access to activity
//! data for arbitrary time ranges.
//!
//! ## File format
//!
//! 4-byte magic `b"HM04"` followed by 15-byte little-endian entries.

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::Serialize;
use xxhash_rust::xxh3::xxh3_64;

use crate::analysis::{PrevSample, compute_health_score};

use super::model::{
    CgroupCpuInfo, CgroupMemoryInfo, DataBlock, ErrorCategory, PgLogEventType, Snapshot,
    SystemCpuInfo,
};

/// Magic bytes identifying heatmap sidecar files (v4: 15 bytes per entry, +health_score).
const HEATMAP_MAGIC: &[u8; 4] = b"HM04";

/// Entry size in bytes.
const ENTRY_SIZE: usize = 15;

/// Local severity mapping for error categories in heatmap context.
/// Same logic as in pg_errors.rs and convert.rs (intentionally duplicated — 5 lines).
fn severity_for_category(cat: ErrorCategory) -> ErrorSeverityGroup {
    match cat {
        ErrorCategory::Lock | ErrorCategory::Constraint | ErrorCategory::Serialization => {
            ErrorSeverityGroup::Info
        }
        ErrorCategory::Resource | ErrorCategory::DataCorruption | ErrorCategory::System => {
            ErrorSeverityGroup::Critical
        }
        _ => ErrorSeverityGroup::Warning,
    }
}

enum ErrorSeverityGroup {
    Critical,
    Warning,
    Info,
}

/// Lightweight per-snapshot heatmap entry.
/// 15 bytes per entry.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct HeatmapEntry {
    /// Number of pg_stat_activity rows where state != "idle".
    pub active_sessions: u16,
    /// Host CPU utilization * 10 (0..1000 = 0.0%..100.0%).
    pub cpu_pct_x10: u16,
    /// Cgroup CPU utilization * 10 relative to cgroup limit (0..1000).
    pub cgroup_cpu_pct_x10: u16,
    /// Cgroup memory utilization * 10 (0..1000 = 0.0%..100.0%).
    pub cgroup_mem_pct_x10: u16,
    /// Critical errors: resource + data_corruption + system.
    pub errors_critical: u8,
    /// Warning errors: timeout + connection + auth + syntax + other.
    pub errors_warning: u8,
    /// Info errors: lock + constraint + serialization.
    pub errors_info: u8,
    /// Number of checkpoint events in this snapshot interval.
    pub checkpoint_count: u8,
    /// Number of autovacuum/autoanalyze events in this snapshot interval.
    pub autovacuum_count: u8,
    /// Number of slow query events in this snapshot interval.
    pub slow_query_count: u8,
    /// Health score (0..100, where 100 = perfectly healthy).
    pub health_score: u8,
}

/// A bucketed heatmap data point for frontend display.
#[derive(Serialize, Clone, Debug)]
pub struct HeatmapBucket {
    /// Bucket start timestamp (epoch seconds).
    pub ts: i64,
    /// Max active sessions in this bucket.
    pub active: u16,
    /// Max host CPU% * 10 in this bucket (0..1000).
    pub cpu: u16,
    /// Max cgroup CPU% * 10 in this bucket (0..1000).
    pub cgroup_cpu: u16,
    /// Max cgroup memory% * 10 in this bucket (0..1000).
    pub cgroup_mem: u16,
    /// Max critical error count in this bucket.
    pub errors_critical: u8,
    /// Max warning error count in this bucket.
    pub errors_warning: u8,
    /// Max info error count in this bucket.
    pub errors_info: u8,
    /// Total checkpoint events in this bucket.
    pub checkpoints: u8,
    /// Total autovacuum/autoanalyze events in this bucket.
    pub autovacuums: u8,
    /// Total slow query events in this bucket.
    pub slow_queries: u8,
    /// Min (worst) health score in this bucket (0..100).
    pub health: u8,
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/// Derives `.heatmap` path from `.zst` path: `"foo.zst"` -> `"foo.heatmap"`.
pub fn heatmap_path(chunk_path: &Path) -> PathBuf {
    chunk_path.with_extension("heatmap")
}

/// Writes heatmap entries to a `.heatmap` sidecar file.
/// Format: 4-byte magic `b"HM04"` + 15-byte little-endian entries.
pub fn write_heatmap(path: &Path, entries: &[HeatmapEntry]) -> io::Result<()> {
    let mut buf = Vec::with_capacity(4 + entries.len() * ENTRY_SIZE);
    buf.extend_from_slice(HEATMAP_MAGIC);
    for e in entries {
        buf.extend_from_slice(&e.active_sessions.to_le_bytes());
        buf.extend_from_slice(&e.cpu_pct_x10.to_le_bytes());
        buf.extend_from_slice(&e.cgroup_cpu_pct_x10.to_le_bytes());
        buf.extend_from_slice(&e.cgroup_mem_pct_x10.to_le_bytes());
        buf.push(e.errors_critical);
        buf.push(e.errors_warning);
        buf.push(e.errors_info);
        buf.push(e.checkpoint_count);
        buf.push(e.autovacuum_count);
        buf.push(e.slow_query_count);
        buf.push(e.health_score);
    }
    fs::write(path, buf)
}

/// Reads heatmap entries from a `.heatmap` sidecar file.
pub fn read_heatmap(path: &Path) -> io::Result<Vec<HeatmapEntry>> {
    let data = fs::read(path)?;

    if data.len() < 4 || data[0..4] != *HEATMAP_MAGIC {
        return Err(io::Error::other("invalid heatmap file magic"));
    }

    let payload = &data[4..];
    if payload.len() % ENTRY_SIZE != 0 {
        return Err(io::Error::other("invalid heatmap file size"));
    }
    let count = payload.len() / ENTRY_SIZE;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * ENTRY_SIZE;
        entries.push(HeatmapEntry {
            active_sessions: u16::from_le_bytes([payload[off], payload[off + 1]]),
            cpu_pct_x10: u16::from_le_bytes([payload[off + 2], payload[off + 3]]),
            cgroup_cpu_pct_x10: u16::from_le_bytes([payload[off + 4], payload[off + 5]]),
            cgroup_mem_pct_x10: u16::from_le_bytes([payload[off + 6], payload[off + 7]]),
            errors_critical: payload[off + 8],
            errors_warning: payload[off + 9],
            errors_info: payload[off + 10],
            checkpoint_count: payload[off + 11],
            autovacuum_count: payload[off + 12],
            slow_query_count: payload[off + 13],
            health_score: payload[off + 14],
        });
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Data extraction from raw snapshots
// ---------------------------------------------------------------------------

/// Precomputed xxh3 hash of "idle" for fast state comparison
/// without needing the StringInterner.
fn idle_hash() -> u64 {
    xxh3_64(b"idle")
}

/// Count checkpoint events in a snapshot.
/// Checks both `PgLogDetailedEvents` (preferred) and legacy `PgLogEvents`.
pub fn count_checkpoint_events(snapshot: &Snapshot) -> u8 {
    // Prefer detailed events (source-of-truth)
    for b in &snapshot.blocks {
        if let DataBlock::PgLogDetailedEvents(events) = b {
            let count = events
                .iter()
                .filter(|e| {
                    matches!(
                        e.event_type,
                        PgLogEventType::CheckpointStarting | PgLogEventType::CheckpointComplete
                    )
                })
                .count();
            return count.min(255) as u8;
        }
    }
    // Fallback to legacy counters
    snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgLogEvents(info) = b {
                Some(info.checkpoint_count.min(255) as u8)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

/// Count autovacuum/autoanalyze events in a snapshot.
/// Checks both `PgLogDetailedEvents` (preferred) and legacy `PgLogEvents`.
pub fn count_autovacuum_events(snapshot: &Snapshot) -> u8 {
    // Prefer detailed events (source-of-truth)
    for b in &snapshot.blocks {
        if let DataBlock::PgLogDetailedEvents(events) = b {
            let count = events
                .iter()
                .filter(|e| {
                    matches!(
                        e.event_type,
                        PgLogEventType::Autovacuum | PgLogEventType::Autoanalyze
                    )
                })
                .count();
            return count.min(255) as u8;
        }
    }
    // Fallback to legacy counters
    snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgLogEvents(info) = b {
                Some(info.autovacuum_count.min(255) as u8)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

/// Count slow query events in a snapshot.
/// Checks both `PgLogDetailedEvents` (preferred) and legacy `PgLogEvents`.
pub fn count_slow_query_events(snapshot: &Snapshot) -> u8 {
    // Prefer detailed events (source-of-truth)
    for b in &snapshot.blocks {
        if let DataBlock::PgLogDetailedEvents(events) = b {
            let count = events
                .iter()
                .filter(|e| matches!(e.event_type, PgLogEventType::SlowQuery))
                .count();
            return count.min(255) as u8;
        }
    }
    // Fallback to legacy counters
    snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgLogEvents(info) = b {
                Some(info.slow_query_count.min(255) as u8)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

/// Count PostgreSQL log errors by severity group: (critical, warning, info).
pub fn count_error_entries_by_severity(snapshot: &Snapshot) -> (u8, u8, u8) {
    let mut critical: u64 = 0;
    let mut warning: u64 = 0;
    let mut info: u64 = 0;

    for b in &snapshot.blocks {
        if let DataBlock::PgLogErrors(entries) = b {
            for e in entries {
                match severity_for_category(e.category) {
                    ErrorSeverityGroup::Critical => critical += e.count as u64,
                    ErrorSeverityGroup::Warning => warning += e.count as u64,
                    ErrorSeverityGroup::Info => info += e.count as u64,
                }
            }
        }
    }

    (
        critical.min(255) as u8,
        warning.min(255) as u8,
        info.min(255) as u8,
    )
}

/// Count non-idle PGA sessions in a snapshot.
pub fn count_active_sessions(snapshot: &Snapshot) -> u16 {
    let idle = idle_hash();
    let count: usize = snapshot
        .blocks
        .iter()
        .filter_map(|b| {
            if let DataBlock::PgStatActivity(rows) = b {
                Some(rows.iter().filter(|r| r.state_hash != idle).count())
            } else {
                None
            }
        })
        .sum();
    count.min(u16::MAX as usize) as u16
}

/// Extract aggregate SystemCpuInfo (cpu_id == -1) from snapshot.
fn extract_system_cpu(snapshot: &Snapshot) -> Option<&SystemCpuInfo> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::SystemCpu(cpus) = b {
            cpus.iter().find(|c| c.cpu_id == -1)
        } else {
            None
        }
    })
}

/// Extract CgroupCpuInfo from snapshot.
fn extract_cgroup_cpu(snapshot: &Snapshot) -> Option<&CgroupCpuInfo> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::Cgroup(cg) = b {
            cg.cpu.as_ref()
        } else {
            None
        }
    })
}

/// Extract CgroupMemoryInfo from snapshot.
fn extract_cgroup_memory(snapshot: &Snapshot) -> Option<&CgroupMemoryInfo> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::Cgroup(cg) = b {
            cg.memory.as_ref()
        } else {
            None
        }
    })
}

/// Compute CPU utilization percentage from delta between two SystemCpuInfo.
/// Returns cpu_pct * 10 (0..1000).
fn compute_cpu_pct(prev: &SystemCpuInfo, curr: &SystemCpuInfo) -> u16 {
    let prev_total = prev.user
        + prev.nice
        + prev.system
        + prev.idle
        + prev.iowait
        + prev.irq
        + prev.softirq
        + prev.steal;
    let curr_total = curr.user
        + curr.nice
        + curr.system
        + curr.idle
        + curr.iowait
        + curr.irq
        + curr.softirq
        + curr.steal;
    let total_delta = curr_total.saturating_sub(prev_total);
    if total_delta == 0 {
        return 0;
    }
    let idle_delta = curr.idle.saturating_sub(prev.idle);
    let busy_delta = total_delta - idle_delta;
    let pct_x10 = (busy_delta as f64 / total_delta as f64 * 1000.0) as u16;
    pct_x10.min(1000)
}

/// Compute cgroup CPU utilization percentage from delta between two snapshots.
/// Returns cgroup_cpu_pct * 10 (0..1000) relative to the cgroup limit.
fn compute_cgroup_cpu_pct(prev: &CgroupCpuInfo, curr: &CgroupCpuInfo, delta_time_secs: f64) -> u16 {
    if delta_time_secs <= 0.0 || curr.quota <= 0 || curr.period == 0 {
        return 0;
    }
    let limit_cores = curr.quota as f64 / curr.period as f64;
    if limit_cores <= 0.0 {
        return 0;
    }
    let d_usage = curr.usage_usec.saturating_sub(prev.usage_usec) as f64 / 1_000_000.0;
    let used_pct = d_usage / delta_time_secs / limit_cores * 100.0;
    let pct_x10 = (used_pct * 10.0) as u16;
    pct_x10.min(1000)
}

/// Compute cgroup memory utilization percentage (instant, no delta).
/// Returns cgroup_mem_pct * 10 (0..1000).
fn compute_cgroup_mem_pct(mem: &CgroupMemoryInfo) -> u16 {
    if mem.max == 0 || mem.max == u64::MAX {
        return 0;
    }
    let pct_x10 = (mem.current as f64 / mem.max as f64 * 1000.0) as u16;
    pct_x10.min(1000)
}

/// Build HeatmapEntry array from a sequence of snapshots.
/// Host CPU% and cgroup CPU% are computed as deltas between consecutive snapshots.
/// First snapshot gets cpu values = 0 (no previous data).
pub fn build_heatmap_from_snapshots(snapshots: &[Snapshot]) -> Vec<HeatmapEntry> {
    let mut entries = Vec::with_capacity(snapshots.len());
    let mut prev_cpu: Option<&SystemCpuInfo> = None;
    let mut prev_cgroup_cpu: Option<&CgroupCpuInfo> = None;
    let mut prev_timestamp: Option<i64> = None;
    let mut prev_sample: Option<PrevSample> = None;

    for snap in snapshots {
        let active = count_active_sessions(snap);

        // Host CPU%
        let cpu = match (prev_cpu, extract_system_cpu(snap)) {
            (Some(prev), Some(curr)) => compute_cpu_pct(prev, curr),
            _ => 0,
        };

        // Cgroup CPU% (needs wall-clock delta for usage_usec → %)
        let delta_time = prev_timestamp
            .map(|pt| (snap.timestamp - pt) as f64)
            .unwrap_or(0.0);
        let cgroup_cpu = match (prev_cgroup_cpu, extract_cgroup_cpu(snap)) {
            (Some(prev), Some(curr)) => compute_cgroup_cpu_pct(prev, curr, delta_time),
            _ => 0,
        };

        // Cgroup memory% (instant value, no delta)
        let cgroup_mem = extract_cgroup_memory(snap)
            .map(compute_cgroup_mem_pct)
            .unwrap_or(0);

        // PostgreSQL log error counts by severity
        let (errors_critical, errors_warning, errors_info) = count_error_entries_by_severity(snap);

        // Checkpoint / autovacuum / slow query events
        let checkpoints = count_checkpoint_events(snap);
        let autovacuums = count_autovacuum_events(snap);
        let slow_queries = count_slow_query_events(snap);

        // Health score
        let health_score = compute_health_score(snap, prev_sample.as_ref(), delta_time).0;

        entries.push(HeatmapEntry {
            active_sessions: active,
            cpu_pct_x10: cpu,
            cgroup_cpu_pct_x10: cgroup_cpu,
            cgroup_mem_pct_x10: cgroup_mem,
            errors_critical,
            errors_warning,
            errors_info,
            checkpoint_count: checkpoints,
            autovacuum_count: autovacuums,
            slow_query_count: slow_queries,
            health_score,
        });

        prev_cpu = extract_system_cpu(snap);
        prev_cgroup_cpu = extract_cgroup_cpu(snap);
        prev_timestamp = Some(snap.timestamp);
        prev_sample = Some(PrevSample::extract(snap));
    }
    entries
}

/// Build heatmap entries by reading snapshots one at a time from a ChunkReader.
///
/// Unlike `build_heatmap_from_snapshots` which needs all snapshots in memory,
/// this reads and decompresses one snapshot at a time, keeping only the minimal
/// prev_cpu/cgroup state for delta computation. Peak memory: one snapshot (~500 KB).
pub fn build_heatmap_streaming(reader: &super::chunk::ChunkReader) -> Option<Vec<HeatmapEntry>> {
    let count = reader.snapshot_count();
    let mut entries = Vec::with_capacity(count);
    let mut prev_cpu: Option<SystemCpuInfo> = None;
    let mut prev_cgroup_cpu: Option<CgroupCpuInfo> = None;
    let mut prev_timestamp: Option<i64> = None;
    let mut prev_sample: Option<PrevSample> = None;

    for i in 0..count {
        let snap = match reader.read_snapshot(i) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    snapshot_idx = i,
                    total = count,
                    error = %e,
                    "heatmap streaming: failed to read snapshot"
                );
                return None;
            }
        };

        let active = count_active_sessions(&snap);

        let cpu = match (prev_cpu.as_ref(), extract_system_cpu(&snap)) {
            (Some(prev), Some(curr)) => compute_cpu_pct(prev, curr),
            _ => 0,
        };

        let delta_time = prev_timestamp
            .map(|pt| (snap.timestamp - pt) as f64)
            .unwrap_or(0.0);
        let cgroup_cpu = match (prev_cgroup_cpu.as_ref(), extract_cgroup_cpu(&snap)) {
            (Some(prev), Some(curr)) => compute_cgroup_cpu_pct(prev, curr, delta_time),
            _ => 0,
        };

        let cgroup_mem = extract_cgroup_memory(&snap)
            .map(compute_cgroup_mem_pct)
            .unwrap_or(0);

        let (errors_critical, errors_warning, errors_info) = count_error_entries_by_severity(&snap);
        let checkpoints = count_checkpoint_events(&snap);
        let autovacuums = count_autovacuum_events(&snap);
        let slow_queries = count_slow_query_events(&snap);

        // Health score
        let health_score = compute_health_score(&snap, prev_sample.as_ref(), delta_time).0;

        entries.push(HeatmapEntry {
            active_sessions: active,
            cpu_pct_x10: cpu,
            cgroup_cpu_pct_x10: cgroup_cpu,
            cgroup_mem_pct_x10: cgroup_mem,
            errors_critical,
            errors_warning,
            errors_info,
            checkpoint_count: checkpoints,
            autovacuum_count: autovacuums,
            slow_query_count: slow_queries,
            health_score,
        });

        // Keep only small prev state — snapshot is dropped
        prev_cpu = extract_system_cpu(&snap).cloned();
        prev_cgroup_cpu = extract_cgroup_cpu(&snap).cloned();
        prev_timestamp = Some(snap.timestamp);
        prev_sample = Some(PrevSample::extract(&snap));
    }

    Some(entries)
}

// ---------------------------------------------------------------------------
// Bucketing for frontend display
// ---------------------------------------------------------------------------

/// Aggregate raw heatmap entries into a fixed number of buckets.
/// Each bucket = max of each field within that time range (sum for events).
pub fn bucket_heatmap(
    entries: &[(i64, HeatmapEntry)],
    start_ts: i64,
    end_ts: i64,
    num_buckets: usize,
) -> Vec<HeatmapBucket> {
    if entries.is_empty() || num_buckets == 0 || end_ts <= start_ts {
        return Vec::new();
    }

    let range = (end_ts - start_ts) as f64;
    let mut buckets: Vec<HeatmapBucket> = (0..num_buckets)
        .map(|i| {
            let bucket_ts = start_ts + (range * i as f64 / num_buckets as f64) as i64;
            HeatmapBucket {
                ts: bucket_ts,
                active: 0,
                cpu: 0,
                cgroup_cpu: 0,
                cgroup_mem: 0,
                errors_critical: 0,
                errors_warning: 0,
                errors_info: 0,
                checkpoints: 0,
                autovacuums: 0,
                slow_queries: 0,
                health: 100,
            }
        })
        .collect();

    for &(ts, ref entry) in entries {
        let idx = ((ts - start_ts) as f64 / range * num_buckets as f64) as usize;
        let idx = idx.min(num_buckets - 1);
        buckets[idx].active = buckets[idx].active.max(entry.active_sessions);
        buckets[idx].cpu = buckets[idx].cpu.max(entry.cpu_pct_x10);
        buckets[idx].cgroup_cpu = buckets[idx].cgroup_cpu.max(entry.cgroup_cpu_pct_x10);
        buckets[idx].cgroup_mem = buckets[idx].cgroup_mem.max(entry.cgroup_mem_pct_x10);
        buckets[idx].errors_critical = buckets[idx].errors_critical.max(entry.errors_critical);
        buckets[idx].errors_warning = buckets[idx].errors_warning.max(entry.errors_warning);
        buckets[idx].errors_info = buckets[idx].errors_info.max(entry.errors_info);
        buckets[idx].checkpoints = buckets[idx]
            .checkpoints
            .saturating_add(entry.checkpoint_count);
        buckets[idx].autovacuums = buckets[idx]
            .autovacuums
            .saturating_add(entry.autovacuum_count);
        buckets[idx].slow_queries = buckets[idx]
            .slow_queries
            .saturating_add(entry.slow_query_count);
        buckets[idx].health = buckets[idx].health.min(entry.health_score);
    }

    buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heatmap_roundtrip() {
        let entries = vec![
            HeatmapEntry {
                active_sessions: 5,
                cpu_pct_x10: 450,
                cgroup_cpu_pct_x10: 300,
                cgroup_mem_pct_x10: 750,
                errors_critical: 2,
                errors_warning: 5,
                errors_info: 10,
                checkpoint_count: 1,
                autovacuum_count: 2,
                slow_query_count: 4,
                health_score: 85,
            },
            HeatmapEntry {
                active_sessions: 0,
                cpu_pct_x10: 0,
                cgroup_cpu_pct_x10: 0,
                cgroup_mem_pct_x10: 0,
                errors_critical: 0,
                errors_warning: 0,
                errors_info: 0,
                checkpoint_count: 0,
                autovacuum_count: 0,
                slow_query_count: 0,
                health_score: 100,
            },
            HeatmapEntry {
                active_sessions: 100,
                cpu_pct_x10: 999,
                cgroup_cpu_pct_x10: 500,
                cgroup_mem_pct_x10: 950,
                errors_critical: 1,
                errors_warning: 20,
                errors_info: 200,
                checkpoint_count: 3,
                autovacuum_count: 7,
                slow_query_count: 16,
                health_score: 30,
            },
        ];
        let dir = std::env::temp_dir().join("rpglot_test_heatmap");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.heatmap");

        write_heatmap(&path, &entries).unwrap();
        let loaded = read_heatmap(&path).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].active_sessions, 5);
        assert_eq!(loaded[0].cpu_pct_x10, 450);
        assert_eq!(loaded[0].cgroup_cpu_pct_x10, 300);
        assert_eq!(loaded[0].cgroup_mem_pct_x10, 750);
        assert_eq!(loaded[0].errors_critical, 2);
        assert_eq!(loaded[0].errors_warning, 5);
        assert_eq!(loaded[0].errors_info, 10);
        assert_eq!(loaded[0].checkpoint_count, 1);
        assert_eq!(loaded[0].autovacuum_count, 2);
        assert_eq!(loaded[0].slow_query_count, 4);
        assert_eq!(loaded[0].health_score, 85);
        assert_eq!(loaded[2].active_sessions, 100);
        assert_eq!(loaded[2].cpu_pct_x10, 999);
        assert_eq!(loaded[2].cgroup_cpu_pct_x10, 500);
        assert_eq!(loaded[2].cgroup_mem_pct_x10, 950);
        assert_eq!(loaded[2].errors_critical, 1);
        assert_eq!(loaded[2].errors_warning, 20);
        assert_eq!(loaded[2].errors_info, 200);
        assert_eq!(loaded[2].checkpoint_count, 3);
        assert_eq!(loaded[2].autovacuum_count, 7);
        assert_eq!(loaded[2].slow_query_count, 16);
        assert_eq!(loaded[2].health_score, 30);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_bucket_heatmap() {
        let entries = vec![
            (
                100,
                HeatmapEntry {
                    active_sessions: 3,
                    cpu_pct_x10: 200,
                    cgroup_cpu_pct_x10: 100,
                    cgroup_mem_pct_x10: 500,
                    errors_critical: 0,
                    errors_warning: 0,
                    errors_info: 0,
                    checkpoint_count: 1,
                    autovacuum_count: 0,
                    slow_query_count: 2,
                    health_score: 90,
                },
            ),
            (
                150,
                HeatmapEntry {
                    active_sessions: 10,
                    cpu_pct_x10: 700,
                    cgroup_cpu_pct_x10: 400,
                    cgroup_mem_pct_x10: 600,
                    errors_critical: 1,
                    errors_warning: 3,
                    errors_info: 5,
                    checkpoint_count: 0,
                    autovacuum_count: 2,
                    slow_query_count: 1,
                    health_score: 60,
                },
            ),
            (
                200,
                HeatmapEntry {
                    active_sessions: 1,
                    cpu_pct_x10: 100,
                    cgroup_cpu_pct_x10: 50,
                    cgroup_mem_pct_x10: 550,
                    errors_critical: 0,
                    errors_warning: 2,
                    errors_info: 8,
                    checkpoint_count: 1,
                    autovacuum_count: 3,
                    slow_query_count: 3,
                    health_score: 40,
                },
            ),
        ];
        let buckets = bucket_heatmap(&entries, 100, 200, 2);
        assert_eq!(buckets.len(), 2);
        // First bucket [100, 150): entry at 100
        assert_eq!(buckets[0].active, 3);
        assert_eq!(buckets[0].cpu, 200);
        assert_eq!(buckets[0].cgroup_cpu, 100);
        assert_eq!(buckets[0].cgroup_mem, 500);
        assert_eq!(buckets[0].errors_critical, 0);
        assert_eq!(buckets[0].errors_warning, 0);
        assert_eq!(buckets[0].errors_info, 0);
        assert_eq!(buckets[0].checkpoints, 1);
        assert_eq!(buckets[0].autovacuums, 0);
        assert_eq!(buckets[0].slow_queries, 2);
        assert_eq!(buckets[0].health, 90);
        // Second bucket [150, 200]: entries at 150 and 200 — min for health
        assert_eq!(buckets[1].active, 10);
        assert_eq!(buckets[1].cpu, 700);
        assert_eq!(buckets[1].cgroup_cpu, 400);
        assert_eq!(buckets[1].cgroup_mem, 600);
        assert_eq!(buckets[1].errors_critical, 1);
        assert_eq!(buckets[1].errors_warning, 3);
        assert_eq!(buckets[1].errors_info, 8);
        assert_eq!(buckets[1].checkpoints, 1); // 0 + 1
        assert_eq!(buckets[1].autovacuums, 5); // 2 + 3
        assert_eq!(buckets[1].slow_queries, 4); // 1 + 3
        assert_eq!(buckets[1].health, 40); // min(60, 40)
    }

    #[test]
    fn test_idle_hash_stable() {
        let hash = idle_hash();
        assert_eq!(hash, xxh3_64(b"idle"));
        assert_ne!(hash, 0);
    }

    #[test]
    fn test_compute_cgroup_cpu_pct() {
        let prev = CgroupCpuInfo {
            quota: 100_000,
            period: 100_000,
            usage_usec: 1_000_000,
            ..Default::default()
        };
        let curr = CgroupCpuInfo {
            quota: 100_000,
            period: 100_000,
            usage_usec: 6_000_000, // +5s of CPU in 10s wall → 50%
            ..Default::default()
        };
        let pct = compute_cgroup_cpu_pct(&prev, &curr, 10.0);
        assert_eq!(pct, 500); // 50.0% * 10

        // Unlimited quota → 0
        let unlimited = CgroupCpuInfo { quota: -1, ..curr };
        assert_eq!(compute_cgroup_cpu_pct(&prev, &unlimited, 10.0), 0);
    }

    #[test]
    fn test_compute_cgroup_mem_pct() {
        let mem = CgroupMemoryInfo {
            max: 1_073_741_824,   // 1 GiB
            current: 536_870_912, // 512 MiB → 50%
            ..Default::default()
        };
        assert_eq!(compute_cgroup_mem_pct(&mem), 500); // 50.0% * 10

        // Unlimited → 0
        let unlimited = CgroupMemoryInfo {
            max: u64::MAX,
            current: 536_870_912,
            ..Default::default()
        };
        assert_eq!(compute_cgroup_mem_pct(&unlimited), 0);
    }
}
