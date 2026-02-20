pub mod advisor;
pub mod rules;

use crate::api::snapshot::HealthBreakdown;
use crate::provider::HistoryProvider;
use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgSettingEntry, ProcessInfo, Snapshot};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::mem;

// ============================================================
// Core types
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Cpu,
    Memory,
    Disk,
    Network,
    Psi,
    PgActivity,
    PgStatements,
    PgLocks,
    PgTables,
    PgIndexes,
    PgBgwriter,
    PgEvents,
    PgErrors,
    Cgroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

pub struct Anomaly {
    pub timestamp: i64,
    pub rule_id: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub title: String,
    pub detail: Option<String>,
    pub value: f64,
    /// Optional sub-key for merge grouping (e.g. PID).
    /// Anomalies with the same rule_id but different merge_key
    /// will NOT be merged into one incident.
    pub merge_key: Option<String>,
    /// Entity identifier for navigation (PID, queryid, relid, indexrelid).
    pub entity_id: Option<i64>,
}

#[derive(Serialize)]
pub struct Incident {
    pub rule_id: String,
    pub category: Category,
    pub severity: Severity,
    pub first_ts: i64,
    pub last_ts: i64,
    #[serde(skip)]
    pub merge_key: Option<String>,
    pub peak_ts: i64,
    pub peak_value: f64,
    pub title: String,
    pub detail: Option<String>,
    pub snapshot_count: usize,
    /// Entity identifier for navigation (PID, queryid, relid, indexrelid).
    pub entity_id: Option<i64>,
}

#[derive(Serialize)]
pub struct IncidentGroup {
    pub id: u32,
    pub first_ts: i64,
    pub last_ts: i64,
    pub severity: Severity,
    pub persistent: bool,
    pub incidents: Vec<Incident>,
}

#[derive(Serialize)]
pub struct HealthPoint {
    pub ts: i64,
    pub score: u8,
}

#[derive(Serialize)]
pub struct AnalysisReport {
    pub start_ts: i64,
    pub end_ts: i64,
    pub snapshots_analyzed: usize,
    pub groups: Vec<IncidentGroup>,
    pub incidents: Vec<Incident>,
    pub recommendations: Vec<advisor::Recommendation>,
    pub summary: AnalysisSummary,
    pub health_scores: Vec<HealthPoint>,
}

#[derive(Serialize)]
pub struct AnalysisSummary {
    pub total_incidents: usize,
    pub critical_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub categories_affected: Vec<Category>,
}

// ============================================================
// Analysis context passed to each rule
// ============================================================

pub struct AnalysisContext<'a> {
    pub snapshot: &'a Snapshot,
    pub prev_snapshot: Option<&'a Snapshot>,
    pub interner: &'a StringInterner,
    pub timestamp: i64,
    pub ewma: &'a EwmaState,
    pub prev: Option<&'a PrevSample>,
    pub dt: f64,
    /// OS-level page cache hit % for PG backends: (rchar - read_bytes) / rchar.
    /// None when /proc/pid/io data is unavailable (e.g. macOS, permissions).
    pub backend_io_hit_pct: Option<f64>,
}

// ============================================================
// EWMA state — O(1) sliding average
// ============================================================

pub struct EwmaState {
    alpha: f64,
    pub n: usize,
    // System
    pub cpu_pct: f64,
    pub iow_pct: f64,
    pub steal_pct: f64,
    pub mem_used_pct: f64,
    // Disk (aggregate max util / max await)
    pub disk_util_pct: f64,
    pub disk_read_bytes_s: f64,
    pub disk_write_bytes_s: f64,
    pub disk_r_await_ms: f64,
    pub disk_w_await_ms: f64,
    // Network (sum across interfaces)
    pub net_rx_bytes_s: f64,
    pub net_tx_bytes_s: f64,
    // PG
    pub active_sessions: f64,
    pub tps: f64,
    // Cgroup
    pub cgroup_cpu_pct: f64,
    pub cgroup_throttle_pct: f64,
}

impl EwmaState {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            n: 0,
            cpu_pct: 0.0,
            iow_pct: 0.0,
            steal_pct: 0.0,
            mem_used_pct: 0.0,
            disk_util_pct: 0.0,
            disk_read_bytes_s: 0.0,
            disk_write_bytes_s: 0.0,
            disk_r_await_ms: 0.0,
            disk_w_await_ms: 0.0,
            net_rx_bytes_s: 0.0,
            net_tx_bytes_s: 0.0,
            active_sessions: 0.0,
            tps: 0.0,
            cgroup_cpu_pct: 0.0,
            cgroup_throttle_pct: 0.0,
        }
    }

    fn update_val(n: usize, alpha: f64, current: f64, avg: &mut f64) {
        if n == 0 {
            *avg = current;
        } else {
            *avg = alpha * current + (1.0 - alpha) * *avg;
        }
    }

    pub fn update(&mut self, snapshot: &Snapshot, prev: Option<&PrevSample>, dt: f64) {
        // CPU
        if let (Some(cpu), Some(p)) = (find_aggregate_cpu(snapshot), prev)
            && dt > 0.0
        {
            let total = sum_cpu_ticks(cpu);
            let prev_total = p.cpu_total;
            let dt_ticks = total.saturating_sub(prev_total) as f64;
            if dt_ticks > 0.0 {
                let idle_d = cpu.idle.saturating_sub(p.cpu_idle) as f64;
                let iow_d = cpu.iowait.saturating_sub(p.cpu_iowait) as f64;
                let steal_d = cpu.steal.saturating_sub(p.cpu_steal) as f64;
                let cpu_pct = (1.0 - idle_d / dt_ticks) * 100.0;
                let iow_pct = (iow_d / dt_ticks) * 100.0;
                let steal_pct = (steal_d / dt_ticks) * 100.0;
                Self::update_val(self.n, self.alpha, cpu_pct, &mut self.cpu_pct);
                Self::update_val(self.n, self.alpha, iow_pct, &mut self.iow_pct);
                Self::update_val(self.n, self.alpha, steal_pct, &mut self.steal_pct);
            }
        }

        // Memory
        if let Some(mem) = find_block(snapshot, |b| match b {
            DataBlock::SystemMem(m) => Some(m),
            _ => None,
        }) && mem.total > 0
        {
            let used_pct = (1.0 - mem.available as f64 / mem.total as f64) * 100.0;
            Self::update_val(self.n, self.alpha, used_pct, &mut self.mem_used_pct);
        }

        // Disk (only relevant devices — skip partitions, loop, ram, container host devices)
        if let (Some(disks), Some(p)) = (
            find_block(snapshot, |b| match b {
                DataBlock::SystemDisk(v) => Some(v.as_slice()),
                _ => None,
            }),
            prev,
        ) && dt > 0.0
        {
            let is_ctr = is_container_snapshot(snapshot);
            let total_rsz: u64 = disks
                .iter()
                .filter(|d| is_relevant_disk(d, is_ctr))
                .map(|d| d.rsz)
                .sum();
            let total_wsz: u64 = disks
                .iter()
                .filter(|d| is_relevant_disk(d, is_ctr))
                .map(|d| d.wsz)
                .sum();
            let read_s = (total_rsz.saturating_sub(p.disk_rsz) as f64 * 512.0) / dt;
            let write_s = (total_wsz.saturating_sub(p.disk_wsz) as f64 * 512.0) / dt;
            // Per-device utilization: max across relevant devices
            let mut max_util = 0.0_f64;
            for d in disks.iter().filter(|d| is_relevant_disk(d, is_ctr)) {
                let prev_io_ms = p
                    .disk_io_ms_per_dev
                    .get(&d.device_hash)
                    .copied()
                    .unwrap_or(0);
                let io_ms_d = d.io_ms.saturating_sub(prev_io_ms) as f64;
                let util = (io_ms_d / (dt * 1000.0) * 100.0).min(100.0);
                max_util = max_util.max(util);
            }
            let util = max_util;
            // Per-device await: max across relevant devices
            let mut max_r_await = 0.0_f64;
            let mut max_w_await = 0.0_f64;
            for d in disks.iter().filter(|d| is_relevant_disk(d, is_ctr)) {
                let prev_rt = p
                    .disk_read_time_per_dev
                    .get(&d.device_hash)
                    .copied()
                    .unwrap_or(0);
                let prev_wt = p
                    .disk_write_time_per_dev
                    .get(&d.device_hash)
                    .copied()
                    .unwrap_or(0);
                let prev_rio = p.disk_rio_per_dev.get(&d.device_hash).copied().unwrap_or(0);
                let prev_wio = p.disk_wio_per_dev.get(&d.device_hash).copied().unwrap_or(0);
                let d_rio = d.rio.saturating_sub(prev_rio);
                let d_wio = d.wio.saturating_sub(prev_wio);
                let d_rt = d.read_time.saturating_sub(prev_rt) as f64;
                let d_wt = d.write_time.saturating_sub(prev_wt) as f64;
                if d_rio > 0 {
                    max_r_await = max_r_await.max(d_rt / d_rio as f64);
                }
                if d_wio > 0 {
                    max_w_await = max_w_await.max(d_wt / d_wio as f64);
                }
            }
            Self::update_val(self.n, self.alpha, util, &mut self.disk_util_pct);
            Self::update_val(self.n, self.alpha, read_s, &mut self.disk_read_bytes_s);
            Self::update_val(self.n, self.alpha, write_s, &mut self.disk_write_bytes_s);
            Self::update_val(self.n, self.alpha, max_r_await, &mut self.disk_r_await_ms);
            Self::update_val(self.n, self.alpha, max_w_await, &mut self.disk_w_await_ms);
        }

        // Network
        if let (Some(nets), Some(p)) = (
            find_block(snapshot, |b| match b {
                DataBlock::SystemNet(v) => Some(v.as_slice()),
                _ => None,
            }),
            prev,
        ) && dt > 0.0
        {
            let total_rx: u64 = nets.iter().map(|n| n.rx_bytes).sum();
            let total_tx: u64 = nets.iter().map(|n| n.tx_bytes).sum();
            let rx_s = total_rx.saturating_sub(p.net_rx_bytes) as f64 / dt;
            let tx_s = total_tx.saturating_sub(p.net_tx_bytes) as f64 / dt;
            Self::update_val(self.n, self.alpha, rx_s, &mut self.net_rx_bytes_s);
            Self::update_val(self.n, self.alpha, tx_s, &mut self.net_tx_bytes_s);
        }

        // PG active sessions
        if let Some(sessions) = find_block(snapshot, |b| match b {
            DataBlock::PgStatActivity(v) => Some(v.as_slice()),
            _ => None,
        }) {
            let idle_hash = xxhash_rust::xxh3::xxh3_64(b"idle");
            let active = sessions
                .iter()
                .filter(|s| s.state_hash != idle_hash && s.state_hash != 0)
                .count() as f64;
            Self::update_val(self.n, self.alpha, active, &mut self.active_sessions);
        }

        // TPS
        if let (Some(dbs), Some(p)) = (
            find_block(snapshot, |b| match b {
                DataBlock::PgStatDatabase(v) => Some(v.as_slice()),
                _ => None,
            }),
            prev,
        ) && dt > 0.0
        {
            let commits: i64 = dbs.iter().map(|d| d.xact_commit).sum();
            let rollbacks: i64 = dbs.iter().map(|d| d.xact_rollback).sum();
            let d_c = (commits - p.pg_xact_commit).max(0) as f64;
            let d_r = (rollbacks - p.pg_xact_rollback).max(0) as f64;
            let tps = (d_c + d_r) / dt;
            Self::update_val(self.n, self.alpha, tps, &mut self.tps);
        }

        // Cgroup
        if let (Some(cg), Some(p)) = (
            find_block(snapshot, |b| match b {
                DataBlock::Cgroup(c) => Some(c),
                _ => None,
            }),
            prev,
        ) && dt > 0.0
            && let Some(cpu_info) = &cg.cpu
        {
            let usage_d = cpu_info.usage_usec.saturating_sub(p.cgroup_usage_usec) as f64;
            let wall_usec = dt * 1_000_000.0;
            let cores = if cpu_info.quota > 0 && cpu_info.period > 0 {
                cpu_info.quota as f64 / cpu_info.period as f64
            } else {
                1.0
            };
            let cpu_pct = (usage_d / wall_usec / cores) * 100.0;
            Self::update_val(self.n, self.alpha, cpu_pct, &mut self.cgroup_cpu_pct);

            let throttle_d = cpu_info
                .throttled_usec
                .saturating_sub(p.cgroup_throttled_usec) as f64;
            let throttle_pct = (throttle_d / wall_usec) * 100.0;
            Self::update_val(
                self.n,
                self.alpha,
                throttle_pct,
                &mut self.cgroup_throttle_pct,
            );
        }

        self.n += 1;
    }

    /// Returns true if current value is a spike (>factor times the average).
    /// Requires at least 5 samples for stable baseline.
    pub fn is_spike(&self, current: f64, avg: f64, factor: f64) -> bool {
        self.n >= 5 && avg > 0.0 && current > avg * factor
    }
}

// ============================================================
// Backend IO hit — OS page cache hit % for PG backends
// ============================================================

/// Compute OS-level page cache hit % for PostgreSQL backends.
///
/// Uses `/proc/<pid>/io` data: `rchar` (all read syscalls, incl. page cache)
/// vs `read_bytes` (physical disk reads). Returns `(rchar - read_bytes) / rchar * 100`.
pub fn compute_backend_io_hit(snap: &Snapshot, prev: Option<&Snapshot>) -> Option<f64> {
    let prev = prev?;

    let pga = find_block(snap, |b| match b {
        DataBlock::PgStatActivity(rows) => Some(rows.as_slice()),
        _ => None,
    })?;

    let pg_pids: HashSet<u32> = pga
        .iter()
        .filter_map(|a| u32::try_from(a.pid).ok())
        .collect();
    if pg_pids.is_empty() {
        return None;
    }

    let curr_procs = find_block(snap, |b| match b {
        DataBlock::Processes(procs) => Some(procs.as_slice()),
        _ => None,
    })?;
    let prev_procs = find_block(prev, |b| match b {
        DataBlock::Processes(procs) => Some(procs.as_slice()),
        _ => None,
    })?;

    let (curr_rchar, curr_rsz) = sum_pg_io(curr_procs, &pg_pids);
    let (prev_rchar, prev_rsz) = sum_pg_io(prev_procs, &pg_pids);

    let delta_rchar = curr_rchar.saturating_sub(prev_rchar);
    let delta_rsz = curr_rsz.saturating_sub(prev_rsz);

    if delta_rchar == 0 {
        return Some(100.0);
    }

    let cache_bytes = delta_rchar.saturating_sub(delta_rsz);
    Some(cache_bytes as f64 * 100.0 / delta_rchar as f64)
}

fn sum_pg_io(procs: &[ProcessInfo], pg_pids: &HashSet<u32>) -> (u64, u64) {
    let mut rchar = 0u64;
    let mut rsz = 0u64;
    for p in procs {
        if pg_pids.contains(&p.pid) {
            rchar += p.dsk.rchar;
            rsz += p.dsk.rsz;
        }
    }
    (rchar, rsz)
}

// ============================================================
// Health score — 100 minus penalties
// ============================================================

/// Compute health score (0..100) from snapshot data and previous sample deltas.
///
/// Penalties:
/// - Active PGA sessions: -1 per 2 active backends
/// - CPU > 60%: -1 per percent above 60
/// - Disk IOPS > 1000: -5 per 1000 total IOPS
/// - Disk bandwidth > 50 MB/s: -5 per 50 MB/s
pub fn compute_health_score(
    snapshot: &Snapshot,
    prev: Option<&PrevSample>,
    dt: f64,
) -> (u8, HealthBreakdown) {
    let mut bd = HealthBreakdown::default();

    // 1. Active PGA sessions (state = "active" only, not "idle in transaction" etc.)
    if let Some(sessions) = find_block(snapshot, |b| match b {
        DataBlock::PgStatActivity(v) => Some(v.as_slice()),
        _ => None,
    }) {
        let active_hash = xxhash_rust::xxh3::xxh3_64(b"active");
        let active = sessions
            .iter()
            .filter(|s| s.state_hash == active_hash)
            .count() as i32;
        bd.sessions = (active / 2).clamp(0, 100) as u8;
    }

    if let Some(p) = prev
        && dt > 0.0
    {
        // 2. CPU > 60%
        if let Some(cpu) = find_aggregate_cpu(snapshot) {
            let total = sum_cpu_ticks(cpu);
            let dt_ticks = total.saturating_sub(p.cpu_total) as f64;
            if dt_ticks > 0.0 {
                let idle_d = cpu.idle.saturating_sub(p.cpu_idle) as f64;
                let cpu_pct = (1.0 - idle_d / dt_ticks) * 100.0;
                if cpu_pct > 60.0 {
                    bd.cpu = (cpu_pct - 60.0).round().clamp(0.0, 100.0) as u8;
                }
            }
        }

        // 3. Disk IOPS + 4. Disk bandwidth
        if let Some(disks) = find_block(snapshot, |b| match b {
            DataBlock::SystemDisk(v) => Some(v.as_slice()),
            _ => None,
        }) {
            let is_ctr = is_container_snapshot(snapshot);
            let relevant = disks.iter().filter(|d| is_relevant_disk(d, is_ctr));
            let total_rio: u64 = relevant.clone().map(|d| d.rio).sum();
            let total_wio: u64 = relevant.clone().map(|d| d.wio).sum();
            let d_iops = (total_rio.saturating_sub(p.disk_rio)
                + total_wio.saturating_sub(p.disk_wio)) as f64
                / dt;
            bd.disk_iops = ((d_iops / 1000.0) as i32 * 5).clamp(0, 100) as u8;

            let total_rsz: u64 = relevant.clone().map(|d| d.rsz).sum();
            let total_wsz: u64 = relevant.map(|d| d.wsz).sum();
            let bw_bytes = (total_rsz.saturating_sub(p.disk_rsz)
                + total_wsz.saturating_sub(p.disk_wsz)) as f64
                * 512.0
                / dt;
            let bw_mb = bw_bytes / (1024.0 * 1024.0);
            bd.disk_bw = ((bw_mb / 50.0) as i32 * 5).clamp(0, 100) as u8;
        }
    }

    let total_penalty =
        bd.sessions as i32 + bd.cpu as i32 + bd.disk_iops as i32 + bd.disk_bw as i32;
    let score = (100 - total_penalty).clamp(0, 100) as u8;
    (score, bd)
}

// ============================================================
// PrevSample — lightweight extract from previous snapshot
// ============================================================

pub struct PrevSample {
    pub timestamp: i64,
    pub cpu_total: u64,
    pub cpu_idle: u64,
    pub cpu_iowait: u64,
    pub cpu_steal: u64,
    pub disk_rsz: u64,
    pub disk_wsz: u64,
    pub disk_rio: u64,
    pub disk_wio: u64,
    /// Per-device io_ms (device_hash → cumulative io_ms).
    pub disk_io_ms_per_dev: HashMap<u64, u64>,
    /// Per-device read_time (device_hash → cumulative read_time ms).
    pub disk_read_time_per_dev: HashMap<u64, u64>,
    /// Per-device write_time (device_hash → cumulative write_time ms).
    pub disk_write_time_per_dev: HashMap<u64, u64>,
    /// Per-device rio (device_hash → cumulative read I/Os).
    pub disk_rio_per_dev: HashMap<u64, u64>,
    /// Per-device wio (device_hash → cumulative write I/Os).
    pub disk_wio_per_dev: HashMap<u64, u64>,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub pg_xact_commit: i64,
    pub pg_xact_rollback: i64,
    pub cgroup_usage_usec: u64,
    pub cgroup_throttled_usec: u64,
}

impl PrevSample {
    pub fn extract(snapshot: &Snapshot) -> Self {
        let mut s = Self {
            timestamp: snapshot.timestamp,
            cpu_total: 0,
            cpu_idle: 0,
            cpu_iowait: 0,
            cpu_steal: 0,
            disk_rsz: 0,
            disk_wsz: 0,
            disk_rio: 0,
            disk_wio: 0,
            disk_io_ms_per_dev: HashMap::new(),
            disk_read_time_per_dev: HashMap::new(),
            disk_write_time_per_dev: HashMap::new(),
            disk_rio_per_dev: HashMap::new(),
            disk_wio_per_dev: HashMap::new(),
            net_rx_bytes: 0,
            net_tx_bytes: 0,
            pg_xact_commit: 0,
            pg_xact_rollback: 0,
            cgroup_usage_usec: 0,
            cgroup_throttled_usec: 0,
        };

        if let Some(cpu) = find_aggregate_cpu(snapshot) {
            s.cpu_total = sum_cpu_ticks(cpu);
            s.cpu_idle = cpu.idle;
            s.cpu_iowait = cpu.iowait;
            s.cpu_steal = cpu.steal;
        }

        if let Some(disks) = find_block(snapshot, |b| match b {
            DataBlock::SystemDisk(v) => Some(v.as_slice()),
            _ => None,
        }) {
            let is_ctr = is_container_snapshot(snapshot);
            let relevant = disks.iter().filter(|d| is_relevant_disk(d, is_ctr));
            s.disk_rsz = relevant.clone().map(|d| d.rsz).sum();
            s.disk_wsz = relevant.clone().map(|d| d.wsz).sum();
            s.disk_rio = relevant.clone().map(|d| d.rio).sum();
            s.disk_wio = relevant.clone().map(|d| d.wio).sum();
            for d in relevant {
                s.disk_io_ms_per_dev.insert(d.device_hash, d.io_ms);
                s.disk_read_time_per_dev.insert(d.device_hash, d.read_time);
                s.disk_write_time_per_dev
                    .insert(d.device_hash, d.write_time);
                s.disk_rio_per_dev.insert(d.device_hash, d.rio);
                s.disk_wio_per_dev.insert(d.device_hash, d.wio);
            }
        }

        if let Some(nets) = find_block(snapshot, |b| match b {
            DataBlock::SystemNet(v) => Some(v.as_slice()),
            _ => None,
        }) {
            s.net_rx_bytes = nets.iter().map(|n| n.rx_bytes).sum();
            s.net_tx_bytes = nets.iter().map(|n| n.tx_bytes).sum();
        }

        if let Some(dbs) = find_block(snapshot, |b| match b {
            DataBlock::PgStatDatabase(v) => Some(v.as_slice()),
            _ => None,
        }) {
            s.pg_xact_commit = dbs.iter().map(|d| d.xact_commit).sum();
            s.pg_xact_rollback = dbs.iter().map(|d| d.xact_rollback).sum();
        }

        if let Some(cg) = find_block(snapshot, |b| match b {
            DataBlock::Cgroup(c) => Some(c),
            _ => None,
        }) && let Some(cpu_info) = &cg.cpu
        {
            s.cgroup_usage_usec = cpu_info.usage_usec;
            s.cgroup_throttled_usec = cpu_info.throttled_usec;
        }

        s
    }
}

// ============================================================
// Helpers
// ============================================================

pub fn find_block<'a, T>(
    snapshot: &'a Snapshot,
    extract: impl Fn(&'a DataBlock) -> Option<T>,
) -> Option<T> {
    snapshot.blocks.iter().find_map(extract)
}

fn find_aggregate_cpu(snapshot: &Snapshot) -> Option<&crate::storage::model::SystemCpuInfo> {
    find_block(snapshot, |b| match b {
        DataBlock::SystemCpu(v) => v.iter().find(|c| c.cpu_id == -1),
        _ => None,
    })
}

/// Check if snapshot comes from a container (has Cgroup block).
pub fn is_container_snapshot(snapshot: &Snapshot) -> bool {
    snapshot
        .blocks
        .iter()
        .any(|b| matches!(b, DataBlock::Cgroup(_)))
}

/// Filter disk suitable for analysis (same logic as api/convert.rs).
///
/// Skips loop/ram devices, partitions (name ending in digit, except nvme),
/// and in container mode skips devices without mountinfo (major=0, minor=0).
pub fn is_relevant_disk(disk: &crate::storage::model::SystemDiskInfo, is_container: bool) -> bool {
    if is_container && disk.major == 0 && disk.minor == 0 {
        return false;
    }
    if disk.device_name.starts_with("loop") || disk.device_name.starts_with("ram") {
        return false;
    }
    if !is_container
        && disk
            .device_name
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_digit())
        && !disk.device_name.starts_with("nvme")
    {
        return false;
    }
    true
}

fn sum_cpu_ticks(cpu: &crate::storage::model::SystemCpuInfo) -> u64 {
    cpu.user + cpu.nice + cpu.system + cpu.idle + cpu.iowait + cpu.irq + cpu.softirq + cpu.steal
}

// ============================================================
// Merge anomalies into incidents
// ============================================================

fn merge_anomalies(mut anomalies: Vec<Anomaly>) -> Vec<Incident> {
    anomalies.sort_by(|a, b| {
        a.rule_id
            .cmp(b.rule_id)
            .then(a.merge_key.cmp(&b.merge_key))
            .then(a.timestamp.cmp(&b.timestamp))
    });

    const BASE_GAP: i64 = 60;
    const MAX_GAP: i64 = 300;

    let mut incidents: Vec<Incident> = Vec::new();

    for anomaly in anomalies {
        let should_merge = incidents.last().is_some_and(|last| {
            let duration = last.last_ts - last.first_ts;
            let adaptive_gap = (duration / 5).clamp(BASE_GAP, MAX_GAP);
            last.rule_id == anomaly.rule_id
                && last.merge_key == anomaly.merge_key
                && (anomaly.timestamp - last.last_ts) <= adaptive_gap
        });

        if should_merge {
            let incident = incidents.last_mut().unwrap();
            incident.last_ts = anomaly.timestamp;
            incident.snapshot_count += 1;
            if anomaly.severity > incident.severity {
                incident.severity = anomaly.severity;
            }
            if anomaly.value > incident.peak_value {
                incident.peak_value = anomaly.value;
                incident.peak_ts = anomaly.timestamp;
                incident.title = anomaly.title;
                incident.detail = anomaly.detail;
                incident.entity_id = anomaly.entity_id;
            }
        } else {
            incidents.push(Incident {
                rule_id: anomaly.rule_id.to_string(),
                category: anomaly.category,
                severity: anomaly.severity,
                first_ts: anomaly.timestamp,
                last_ts: anomaly.timestamp,
                merge_key: anomaly.merge_key,
                peak_ts: anomaly.timestamp,
                peak_value: anomaly.value,
                title: anomaly.title,
                detail: anomaly.detail,
                snapshot_count: 1,
                entity_id: anomaly.entity_id,
            });
        }
    }

    incidents.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.first_ts.cmp(&b.first_ts))
    });
    incidents
}

// ============================================================
// Correlate incidents into groups
// ============================================================

/// Group temporally overlapping incidents.
///
/// - Persistent incidents (duration > 30% of the analysis window) get their own group.
/// - Short incidents are grouped by interval merging with a 30s gap.
/// - Every incident ends up in exactly one group (even singletons).
fn correlate_incidents(incidents: Vec<Incident>, start_ts: i64, end_ts: i64) -> Vec<IncidentGroup> {
    let window = (end_ts - start_ts).max(1);
    let persistent_threshold = window * 30 / 100; // 30% of hour
    const CORRELATION_GAP: i64 = 30;

    // Determine persistent rule_ids: if the total duration of all incidents
    // for a given rule_id exceeds 30% of the analysis window, treat ALL
    // incidents of that rule_id as persistent. This catches "quasi-persistent"
    // issues like cache_miss that produce many short incidents spanning the hour.
    let persistent_rules: HashSet<String> = {
        let mut duration_by_rule: HashMap<&str, i64> = HashMap::new();
        for inc in &incidents {
            *duration_by_rule.entry(&inc.rule_id).or_default() += inc.last_ts - inc.first_ts;
        }
        duration_by_rule
            .into_iter()
            .filter(|(_, d)| *d > persistent_threshold)
            .map(|(r, _)| r.to_owned())
            .collect()
    };

    let mut persistent = Vec::new();
    let mut transient = Vec::new();

    for inc in incidents {
        if persistent_rules.contains(&inc.rule_id) {
            persistent.push(inc);
        } else {
            transient.push(inc);
        }
    }

    // Sort transient by first_ts for interval merging
    transient.sort_by_key(|i| i.first_ts);

    let mut groups: Vec<IncidentGroup> = Vec::new();
    let mut group_id: u32 = 0;

    // Persistent: group all incidents of the same rule_id together
    let mut persistent_by_rule: HashMap<String, Vec<Incident>> = HashMap::new();
    for inc in persistent {
        persistent_by_rule
            .entry(inc.rule_id.clone())
            .or_default()
            .push(inc);
    }
    // Sort rules by max severity desc
    let mut persistent_entries: Vec<_> = persistent_by_rule.into_iter().collect();
    persistent_entries.sort_by(|a, b| {
        let sev_a =
            a.1.iter()
                .map(|i| i.severity)
                .max()
                .unwrap_or(Severity::Info);
        let sev_b =
            b.1.iter()
                .map(|i| i.severity)
                .max()
                .unwrap_or(Severity::Info);
        sev_b.cmp(&sev_a).then(a.0.cmp(&b.0))
    });
    for (_rule_id, mut incs) in persistent_entries {
        incs.sort_by_key(|i| i.first_ts);
        groups.push(flush_group(&mut incs, group_id, true));
        group_id += 1;
    }

    // Transient: interval merging with GAP.
    // Use first_ts as the merge anchor — a new incident joins the group
    // if its first_ts is within GAP of the latest first_ts in the group.
    // This prevents long bars from "stretching" the group far into the future.
    let mut pending: Vec<Incident> = Vec::new();
    let mut latest_first_ts: i64 = i64::MIN;

    for inc in transient {
        if !pending.is_empty() && inc.first_ts <= latest_first_ts + CORRELATION_GAP {
            // Add to current group
            latest_first_ts = latest_first_ts.max(inc.first_ts);
            pending.push(inc);
        } else {
            // Flush previous group
            if !pending.is_empty() {
                groups.push(flush_group(&mut pending, group_id, false));
                group_id += 1;
            }
            latest_first_ts = inc.first_ts;
            pending.push(inc);
        }
    }
    if !pending.is_empty() {
        groups.push(flush_group(&mut pending, group_id, false));
    }

    groups
}

fn flush_group(incidents: &mut Vec<Incident>, id: u32, persistent: bool) -> IncidentGroup {
    let mut taken = mem::take(incidents);
    // Sort within group: severity desc, peak_value desc
    taken.sort_by(|a, b| {
        b.severity.cmp(&a.severity).then(
            b.peak_value
                .partial_cmp(&a.peak_value)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    let first_ts = taken.iter().map(|i| i.first_ts).min().unwrap_or(0);
    let last_ts = taken.iter().map(|i| i.last_ts).max().unwrap_or(0);
    let severity = taken
        .iter()
        .map(|i| i.severity)
        .max()
        .unwrap_or(Severity::Info);
    IncidentGroup {
        id,
        first_ts,
        last_ts,
        severity,
        persistent,
        incidents: taken,
    }
}

// ============================================================
// Analyzer — orchestrator
// ============================================================

pub struct Analyzer {
    rules: Vec<Box<dyn rules::AnalysisRule>>,
    advisors: Vec<Box<dyn advisor::Advisor>>,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            rules: rules::all_rules(),
            advisors: advisor::all_advisors(),
        }
    }

    pub fn analyze(
        &self,
        provider: &mut HistoryProvider,
        start_ts: i64,
        end_ts: i64,
    ) -> AnalysisReport {
        let timestamps = provider.timestamps().to_vec();
        let start_pos = timestamps.partition_point(|&ts| ts < start_ts);
        let end_pos = timestamps.partition_point(|&ts| ts <= end_ts);

        // Pre-load health scores from heatmap (already computed during heatmap build)
        let heatmap_health: HashMap<i64, u8> = provider
            .load_heatmap_range(start_ts, end_ts)
            .into_iter()
            .map(|(ts, entry)| (ts, entry.health_score))
            .collect();

        let mut ewma = EwmaState::new(0.1);
        let mut prev_sample: Option<PrevSample> = None;
        let mut prev_snap: Option<Snapshot> = None;
        let mut prev_prev_snap: Option<Snapshot> = None;
        let mut anomalies: Vec<Anomaly> = Vec::new();
        let mut health_scores: Vec<HealthPoint> = Vec::new();
        let mut snapshots_analyzed: usize = 0;
        let mut pg_settings_data: Option<Vec<PgSettingEntry>> = None;

        for pos in start_pos..end_pos {
            let Some((snapshot, interner)) = provider.snapshot_with_interner_at(pos) else {
                continue;
            };

            let dt = prev_sample
                .as_ref()
                .map(|p| (snapshot.timestamp - p.timestamp) as f64)
                .unwrap_or(0.0);

            ewma.update(&snapshot, prev_sample.as_ref(), dt);

            let backend_io_hit_pct = compute_backend_io_hit(&snapshot, prev_snap.as_ref());

            let ctx = AnalysisContext {
                snapshot: &snapshot,
                prev_snapshot: prev_snap.as_ref(),
                interner: &interner,
                timestamp: snapshot.timestamp,
                ewma: &ewma,
                prev: prev_sample.as_ref(),
                dt,
                backend_io_hit_pct,
            };

            for rule in &self.rules {
                anomalies.extend(rule.evaluate(&ctx));
            }

            // Use pre-computed health from heatmap; fallback to recompute if missing
            let score = heatmap_health
                .get(&snapshot.timestamp)
                .copied()
                .unwrap_or_else(|| compute_health_score(&snapshot, prev_sample.as_ref(), dt).0);
            health_scores.push(HealthPoint {
                ts: snapshot.timestamp,
                score,
            });

            // Extract pg_settings from the first snapshot that has them
            if pg_settings_data.is_none()
                && let Some(settings) = find_block(&snapshot, |b| match b {
                    DataBlock::PgSettings(v) => Some(v.clone()),
                    _ => None,
                })
            {
                pg_settings_data = Some(settings);
            }

            prev_sample = Some(PrevSample::extract(&snapshot));
            prev_prev_snap = prev_snap.take();
            prev_snap = Some(snapshot);
            snapshots_analyzed += 1;
        }

        // Layer 2: merge
        let incidents = merge_anomalies(anomalies);

        // Layer 3: advisors (run before correlate consumes incidents)
        let advisor_ctx = advisor::AdvisorContext {
            incidents: &incidents,
            settings: pg_settings_data.as_deref().map(advisor::PgSettings::new),
            snapshot: prev_snap.as_ref(),
            prev_snapshot: prev_prev_snap.as_ref(),
        };
        let mut recommendations = Vec::new();
        for adv in &self.advisors {
            recommendations.extend(adv.evaluate(&advisor_ctx));
        }
        recommendations.sort_by(|a, b| b.severity.cmp(&a.severity));

        let summary = AnalysisSummary {
            total_incidents: incidents.len(),
            critical_count: incidents
                .iter()
                .filter(|i| i.severity == Severity::Critical)
                .count(),
            warning_count: incidents
                .iter()
                .filter(|i| i.severity == Severity::Warning)
                .count(),
            info_count: incidents
                .iter()
                .filter(|i| i.severity == Severity::Info)
                .count(),
            categories_affected: {
                let set: HashSet<Category> = incidents.iter().map(|i| i.category).collect();
                let mut cats: Vec<_> = set.into_iter().collect();
                cats.sort_by_key(|c| *c as u8);
                cats
            },
        };

        // Layer 4: correlate into groups
        let groups = correlate_incidents(incidents, start_ts, end_ts);

        // Flat incidents list for backward compatibility
        let flat_incidents: Vec<Incident> = groups
            .iter()
            .flat_map(|g| &g.incidents)
            .map(|i| Incident {
                rule_id: i.rule_id.clone(),
                category: i.category,
                severity: i.severity,
                first_ts: i.first_ts,
                last_ts: i.last_ts,
                merge_key: None,
                peak_ts: i.peak_ts,
                peak_value: i.peak_value,
                title: i.title.clone(),
                detail: i.detail.clone(),
                snapshot_count: i.snapshot_count,
                entity_id: i.entity_id,
            })
            .collect();

        AnalysisReport {
            start_ts,
            end_ts,
            snapshots_analyzed,
            groups,
            incidents: flat_incidents,
            recommendations,
            summary,
            health_scores,
        }
    }
}
