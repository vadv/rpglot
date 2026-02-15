//! Lightweight per-snapshot metrics for timeline heatmap visualization.
//!
//! Each snapshot produces a 4-byte `MetricsEntry` (active_sessions + cpu_pct).
//! These are stored in `.metrics` sidecar files alongside `.zst` chunk files
//! and read without decompressing snapshots — enabling O(1) access to activity
//! data for arbitrary time ranges.

use std::path::{Path, PathBuf};

use serde::Serialize;
use xxhash_rust::xxh3::xxh3_64;

use super::model::{DataBlock, Snapshot, SystemCpuInfo};

/// Lightweight per-snapshot metrics for timeline heatmap.
/// Packed into 4 bytes for minimal disk/RAM footprint.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct MetricsEntry {
    /// Number of pg_stat_activity rows where state != "idle".
    pub active_sessions: u16,
    /// CPU utilization * 10 (0..1000 = 0.0%..100.0%).
    /// Computed as delta between consecutive snapshots' SystemCpuInfo jiffies.
    /// First entry in chunk = 0 (no previous data for delta).
    pub cpu_pct_x10: u16,
}

/// A bucketed heatmap data point for frontend display.
#[derive(Serialize, Clone, Debug)]
pub struct HeatmapBucket {
    /// Bucket start timestamp (epoch seconds).
    pub ts: i64,
    /// Max active sessions in this bucket.
    pub active: u16,
    /// Max CPU% * 10 in this bucket (0..1000).
    pub cpu: u16,
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/// Derives `.metrics` path from `.zst` path: `"foo.zst"` -> `"foo.metrics"`.
pub fn metrics_path(chunk_path: &Path) -> PathBuf {
    chunk_path.with_extension("metrics")
}

/// Writes metrics entries to a `.metrics` sidecar file (little-endian, no header).
pub fn write_metrics(path: &Path, entries: &[MetricsEntry]) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(entries.len() * 4);
    for e in entries {
        buf.extend_from_slice(&e.active_sessions.to_le_bytes());
        buf.extend_from_slice(&e.cpu_pct_x10.to_le_bytes());
    }
    std::fs::write(path, buf)
}

/// Reads metrics entries from a `.metrics` sidecar file.
pub fn read_metrics(path: &Path) -> std::io::Result<Vec<MetricsEntry>> {
    let data = std::fs::read(path)?;
    if data.len() % 4 != 0 {
        return Err(std::io::Error::other("invalid metrics file size"));
    }
    let count = data.len() / 4;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * 4;
        let active = u16::from_le_bytes([data[off], data[off + 1]]);
        let cpu = u16::from_le_bytes([data[off + 2], data[off + 3]]);
        entries.push(MetricsEntry {
            active_sessions: active,
            cpu_pct_x10: cpu,
        });
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Metric extraction from raw snapshots
// ---------------------------------------------------------------------------

/// Precomputed xxh3 hash of "idle" for fast state comparison
/// without needing the StringInterner.
fn idle_hash() -> u64 {
    xxh3_64(b"idle")
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

/// Build MetricsEntry array from a sequence of snapshots.
/// CPU% is computed as delta between consecutive snapshots.
/// First snapshot gets cpu_pct_x10 = 0 (no previous data).
pub fn build_metrics_from_snapshots(snapshots: &[Snapshot]) -> Vec<MetricsEntry> {
    let mut entries = Vec::with_capacity(snapshots.len());
    let mut prev_cpu: Option<&SystemCpuInfo> = None;

    for snap in snapshots {
        let active = count_active_sessions(snap);
        let cpu = match (prev_cpu, extract_system_cpu(snap)) {
            (Some(prev), Some(curr)) => compute_cpu_pct(prev, curr),
            _ => 0,
        };
        entries.push(MetricsEntry {
            active_sessions: active,
            cpu_pct_x10: cpu,
        });
        prev_cpu = extract_system_cpu(snap);
    }
    entries
}

// ---------------------------------------------------------------------------
// Bucketing for frontend display
// ---------------------------------------------------------------------------

/// Aggregate raw metrics into a fixed number of buckets.
/// Each bucket = max(active_sessions), max(cpu_pct_x10) within that time range.
pub fn bucket_metrics(
    entries: &[(i64, MetricsEntry)],
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
            }
        })
        .collect();

    for &(ts, ref entry) in entries {
        let idx = ((ts - start_ts) as f64 / range * num_buckets as f64) as usize;
        let idx = idx.min(num_buckets - 1);
        buckets[idx].active = buckets[idx].active.max(entry.active_sessions);
        buckets[idx].cpu = buckets[idx].cpu.max(entry.cpu_pct_x10);
    }

    buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_roundtrip() {
        let entries = vec![
            MetricsEntry {
                active_sessions: 5,
                cpu_pct_x10: 450,
            },
            MetricsEntry {
                active_sessions: 0,
                cpu_pct_x10: 0,
            },
            MetricsEntry {
                active_sessions: 100,
                cpu_pct_x10: 999,
            },
        ];
        let dir = std::env::temp_dir().join("rpglot_test_metrics");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.metrics");

        write_metrics(&path, &entries).unwrap();
        let loaded = read_metrics(&path).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].active_sessions, 5);
        assert_eq!(loaded[0].cpu_pct_x10, 450);
        assert_eq!(loaded[2].active_sessions, 100);
        assert_eq!(loaded[2].cpu_pct_x10, 999);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_bucket_metrics() {
        let entries = vec![
            (
                100,
                MetricsEntry {
                    active_sessions: 3,
                    cpu_pct_x10: 200,
                },
            ),
            (
                150,
                MetricsEntry {
                    active_sessions: 10,
                    cpu_pct_x10: 700,
                },
            ),
            (
                200,
                MetricsEntry {
                    active_sessions: 1,
                    cpu_pct_x10: 100,
                },
            ),
        ];
        let buckets = bucket_metrics(&entries, 100, 200, 2);
        assert_eq!(buckets.len(), 2);
        // First bucket [100, 150): entry at 100 → active=3, cpu=200
        // But 150 maps to idx = ((150-100)/100 * 2) = 1.0 → idx=1
        // So first bucket has ts=100 entry: active=3, cpu=200
        assert_eq!(buckets[0].active, 3);
        assert_eq!(buckets[0].cpu, 200);
        // Second bucket [150, 200]: entries at 150 and 200
        assert_eq!(buckets[1].active, 10);
        assert_eq!(buckets[1].cpu, 700);
    }

    #[test]
    fn test_idle_hash_stable() {
        // Ensure idle hash matches what StringInterner would produce
        let hash = idle_hash();
        assert_eq!(hash, xxh3_64(b"idle"));
        // Hash should be non-zero
        assert_ne!(hash, 0);
    }
}
