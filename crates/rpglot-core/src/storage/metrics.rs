//! Lightweight per-snapshot metrics for timeline heatmap visualization.
//!
//! Each snapshot produces a 10-byte `MetricsEntry` (active_sessions, host CPU%,
//! cgroup CPU%, cgroup memory%, error_count).
//! These are stored in `.metrics` sidecar files alongside `.zst` chunk files
//! and read without decompressing snapshots — enabling O(1) access to activity
//! data for arbitrary time ranges.
//!
//! ## File format
//!
//! **V3** (current): 4-byte magic `b"MET3"` followed by 10-byte entries.
//! **V2** (legacy): 4-byte magic `b"MET2"` followed by 8-byte entries (error_count = 0).
//! **V1** (legacy): no header, 4-byte entries. Read with cgroup + error fields = 0.

use std::path::{Path, PathBuf};

use serde::Serialize;
use xxhash_rust::xxh3::xxh3_64;

use super::model::{CgroupCpuInfo, CgroupMemoryInfo, DataBlock, Snapshot, SystemCpuInfo};

/// Magic bytes identifying V2 metrics files.
const METRICS_MAGIC_V2: &[u8; 4] = b"MET2";

/// Magic bytes identifying V3 metrics files (adds error_count).
const METRICS_MAGIC_V3: &[u8; 4] = b"MET3";

/// Lightweight per-snapshot metrics for timeline heatmap.
/// V3: 10 bytes per entry. V2: 8 bytes (error_count = 0).
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct MetricsEntry {
    /// Number of pg_stat_activity rows where state != "idle".
    pub active_sessions: u16,
    /// Host CPU utilization * 10 (0..1000 = 0.0%..100.0%).
    /// Computed as delta between consecutive snapshots' SystemCpuInfo jiffies.
    /// First entry in chunk = 0 (no previous data for delta).
    pub cpu_pct_x10: u16,
    /// Cgroup CPU utilization * 10 relative to cgroup limit (0..1000).
    /// Computed as delta(usage_usec) / (delta_time * limit_cores).
    /// 0 when not in a container or no cgroup CPU limit.
    pub cgroup_cpu_pct_x10: u16,
    /// Cgroup memory utilization * 10 (0..1000 = 0.0%..100.0%).
    /// Instant value: memory.current / memory.max.
    /// 0 when not in a container or no cgroup memory limit.
    pub cgroup_mem_pct_x10: u16,
    /// Total PostgreSQL log error count in this snapshot (ERROR+FATAL+PANIC).
    /// 0 when no errors or log collector not configured.
    pub error_count: u16,
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
    /// Max PostgreSQL error count in this bucket.
    pub errors: u16,
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/// Derives `.metrics` path from `.zst` path: `"foo.zst"` -> `"foo.metrics"`.
pub fn metrics_path(chunk_path: &Path) -> PathBuf {
    chunk_path.with_extension("metrics")
}

/// Writes metrics entries to a V3 `.metrics` sidecar file.
/// Format: 4-byte magic `b"MET3"` + 10-byte little-endian entries.
pub fn write_metrics(path: &Path, entries: &[MetricsEntry]) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(4 + entries.len() * 10);
    buf.extend_from_slice(METRICS_MAGIC_V3);
    for e in entries {
        buf.extend_from_slice(&e.active_sessions.to_le_bytes());
        buf.extend_from_slice(&e.cpu_pct_x10.to_le_bytes());
        buf.extend_from_slice(&e.cgroup_cpu_pct_x10.to_le_bytes());
        buf.extend_from_slice(&e.cgroup_mem_pct_x10.to_le_bytes());
        buf.extend_from_slice(&e.error_count.to_le_bytes());
    }
    std::fs::write(path, buf)
}

/// Reads metrics entries from a `.metrics` sidecar file.
/// Supports V3 (10 bytes/entry), V2 (8 bytes/entry), and
/// V1 (no header, 4 bytes/entry) formats transparently.
pub fn read_metrics(path: &Path) -> std::io::Result<Vec<MetricsEntry>> {
    let data = std::fs::read(path)?;

    if data.len() >= 4 && data[0..4] == *METRICS_MAGIC_V3 {
        // V3 format: 4-byte header + 10-byte entries
        let payload = &data[4..];
        if payload.len() % 10 != 0 {
            return Err(std::io::Error::other("invalid v3 metrics file size"));
        }
        let count = payload.len() / 10;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let off = i * 10;
            entries.push(MetricsEntry {
                active_sessions: u16::from_le_bytes([payload[off], payload[off + 1]]),
                cpu_pct_x10: u16::from_le_bytes([payload[off + 2], payload[off + 3]]),
                cgroup_cpu_pct_x10: u16::from_le_bytes([payload[off + 4], payload[off + 5]]),
                cgroup_mem_pct_x10: u16::from_le_bytes([payload[off + 6], payload[off + 7]]),
                error_count: u16::from_le_bytes([payload[off + 8], payload[off + 9]]),
            });
        }
        Ok(entries)
    } else if data.len() >= 4 && data[0..4] == *METRICS_MAGIC_V2 {
        // V2 format: 4-byte header + 8-byte entries
        let payload = &data[4..];
        if payload.len() % 8 != 0 {
            return Err(std::io::Error::other("invalid v2 metrics file size"));
        }
        let count = payload.len() / 8;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let off = i * 8;
            entries.push(MetricsEntry {
                active_sessions: u16::from_le_bytes([payload[off], payload[off + 1]]),
                cpu_pct_x10: u16::from_le_bytes([payload[off + 2], payload[off + 3]]),
                cgroup_cpu_pct_x10: u16::from_le_bytes([payload[off + 4], payload[off + 5]]),
                cgroup_mem_pct_x10: u16::from_le_bytes([payload[off + 6], payload[off + 7]]),
                error_count: 0,
            });
        }
        Ok(entries)
    } else {
        // V1 format: no header, 4-byte entries (legacy)
        if data.len() % 4 != 0 {
            return Err(std::io::Error::other("invalid metrics file size"));
        }
        let count = data.len() / 4;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let off = i * 4;
            entries.push(MetricsEntry {
                active_sessions: u16::from_le_bytes([data[off], data[off + 1]]),
                cpu_pct_x10: u16::from_le_bytes([data[off + 2], data[off + 3]]),
                cgroup_cpu_pct_x10: 0,
                cgroup_mem_pct_x10: 0,
                error_count: 0,
            });
        }
        Ok(entries)
    }
}

// ---------------------------------------------------------------------------
// Metric extraction from raw snapshots
// ---------------------------------------------------------------------------

/// Precomputed xxh3 hash of "idle" for fast state comparison
/// without needing the StringInterner.
fn idle_hash() -> u64 {
    xxh3_64(b"idle")
}

/// Count total PostgreSQL log errors in a snapshot.
pub fn count_error_entries(snapshot: &Snapshot) -> u16 {
    let total: u64 = snapshot
        .blocks
        .iter()
        .filter_map(|b| {
            if let DataBlock::PgLogErrors(entries) = b {
                Some(entries.iter().map(|e| e.count as u64).sum::<u64>())
            } else {
                None
            }
        })
        .sum();
    total.min(u16::MAX as u64) as u16
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

/// Build MetricsEntry array from a sequence of snapshots.
/// Host CPU% and cgroup CPU% are computed as deltas between consecutive snapshots.
/// First snapshot gets cpu values = 0 (no previous data).
pub fn build_metrics_from_snapshots(snapshots: &[Snapshot]) -> Vec<MetricsEntry> {
    let mut entries = Vec::with_capacity(snapshots.len());
    let mut prev_cpu: Option<&SystemCpuInfo> = None;
    let mut prev_cgroup_cpu: Option<&CgroupCpuInfo> = None;
    let mut prev_timestamp: Option<i64> = None;

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

        // PostgreSQL log error count
        let errors = count_error_entries(snap);

        entries.push(MetricsEntry {
            active_sessions: active,
            cpu_pct_x10: cpu,
            cgroup_cpu_pct_x10: cgroup_cpu,
            cgroup_mem_pct_x10: cgroup_mem,
            error_count: errors,
        });

        prev_cpu = extract_system_cpu(snap);
        prev_cgroup_cpu = extract_cgroup_cpu(snap);
        prev_timestamp = Some(snap.timestamp);
    }
    entries
}

// ---------------------------------------------------------------------------
// Bucketing for frontend display
// ---------------------------------------------------------------------------

/// Aggregate raw metrics into a fixed number of buckets.
/// Each bucket = max of each field within that time range.
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
                cgroup_cpu: 0,
                cgroup_mem: 0,
                errors: 0,
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
        buckets[idx].errors = buckets[idx].errors.max(entry.error_count);
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
                cgroup_cpu_pct_x10: 300,
                cgroup_mem_pct_x10: 750,
                error_count: 3,
            },
            MetricsEntry {
                active_sessions: 0,
                cpu_pct_x10: 0,
                cgroup_cpu_pct_x10: 0,
                cgroup_mem_pct_x10: 0,
                error_count: 0,
            },
            MetricsEntry {
                active_sessions: 100,
                cpu_pct_x10: 999,
                cgroup_cpu_pct_x10: 500,
                cgroup_mem_pct_x10: 950,
                error_count: 42,
            },
        ];
        let dir = std::env::temp_dir().join("rpglot_test_metrics_v3");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.metrics");

        write_metrics(&path, &entries).unwrap();
        let loaded = read_metrics(&path).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].active_sessions, 5);
        assert_eq!(loaded[0].cpu_pct_x10, 450);
        assert_eq!(loaded[0].cgroup_cpu_pct_x10, 300);
        assert_eq!(loaded[0].cgroup_mem_pct_x10, 750);
        assert_eq!(loaded[0].error_count, 3);
        assert_eq!(loaded[2].active_sessions, 100);
        assert_eq!(loaded[2].cpu_pct_x10, 999);
        assert_eq!(loaded[2].cgroup_cpu_pct_x10, 500);
        assert_eq!(loaded[2].cgroup_mem_pct_x10, 950);
        assert_eq!(loaded[2].error_count, 42);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_metrics_v1_backward_compat() {
        // Simulate a V1 file: no magic header, 4-byte entries
        let dir = std::env::temp_dir().join("rpglot_test_metrics_v1");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("legacy.metrics");

        let mut buf = Vec::new();
        buf.extend_from_slice(&5u16.to_le_bytes());
        buf.extend_from_slice(&450u16.to_le_bytes());
        buf.extend_from_slice(&10u16.to_le_bytes());
        buf.extend_from_slice(&200u16.to_le_bytes());
        std::fs::write(&path, &buf).unwrap();

        let loaded = read_metrics(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].active_sessions, 5);
        assert_eq!(loaded[0].cpu_pct_x10, 450);
        assert_eq!(loaded[0].cgroup_cpu_pct_x10, 0);
        assert_eq!(loaded[0].cgroup_mem_pct_x10, 0);
        assert_eq!(loaded[1].active_sessions, 10);
        assert_eq!(loaded[1].cpu_pct_x10, 200);
        assert_eq!(loaded[1].cgroup_cpu_pct_x10, 0);
        assert_eq!(loaded[1].cgroup_mem_pct_x10, 0);

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
                    cgroup_cpu_pct_x10: 100,
                    cgroup_mem_pct_x10: 500,
                    error_count: 0,
                },
            ),
            (
                150,
                MetricsEntry {
                    active_sessions: 10,
                    cpu_pct_x10: 700,
                    cgroup_cpu_pct_x10: 400,
                    cgroup_mem_pct_x10: 600,
                    error_count: 5,
                },
            ),
            (
                200,
                MetricsEntry {
                    active_sessions: 1,
                    cpu_pct_x10: 100,
                    cgroup_cpu_pct_x10: 50,
                    cgroup_mem_pct_x10: 550,
                    error_count: 2,
                },
            ),
        ];
        let buckets = bucket_metrics(&entries, 100, 200, 2);
        assert_eq!(buckets.len(), 2);
        // First bucket [100, 150): entry at 100
        assert_eq!(buckets[0].active, 3);
        assert_eq!(buckets[0].cpu, 200);
        assert_eq!(buckets[0].cgroup_cpu, 100);
        assert_eq!(buckets[0].cgroup_mem, 500);
        assert_eq!(buckets[0].errors, 0);
        // Second bucket [150, 200]: entries at 150 and 200
        assert_eq!(buckets[1].active, 10);
        assert_eq!(buckets[1].cpu, 700);
        assert_eq!(buckets[1].cgroup_cpu, 400);
        assert_eq!(buckets[1].cgroup_mem, 600);
        assert_eq!(buckets[1].errors, 5);
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
        let unlimited = CgroupCpuInfo {
            quota: -1,
            ..curr.clone()
        };
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
