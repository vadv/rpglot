pub mod advisor;
pub mod rules;

use crate::provider::HistoryProvider;
use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, Snapshot};
use serde::Serialize;
use std::collections::HashSet;

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
    PgBgwriter,
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
}

#[derive(Serialize)]
pub struct Incident {
    pub rule_id: String,
    pub category: Category,
    pub severity: Severity,
    pub first_ts: i64,
    pub last_ts: i64,
    pub peak_ts: i64,
    pub peak_value: f64,
    pub title: String,
    pub detail: Option<String>,
    pub snapshot_count: usize,
}

#[derive(Serialize)]
pub struct AnalysisReport {
    pub start_ts: i64,
    pub end_ts: i64,
    pub snapshots_analyzed: usize,
    pub incidents: Vec<Incident>,
    pub recommendations: Vec<advisor::Recommendation>,
    pub summary: AnalysisSummary,
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
    pub interner: &'a StringInterner,
    pub timestamp: i64,
    pub ewma: &'a EwmaState,
    pub prev: Option<&'a PrevSample>,
    pub dt: f64,
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
    // Disk (aggregate max util)
    pub disk_util_pct: f64,
    pub disk_read_bytes_s: f64,
    pub disk_write_bytes_s: f64,
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

        // Disk
        if let (Some(disks), Some(p)) = (
            find_block(snapshot, |b| match b {
                DataBlock::SystemDisk(v) => Some(v.as_slice()),
                _ => None,
            }),
            prev,
        ) && dt > 0.0
        {
            let total_rsz: u64 = disks.iter().map(|d| d.rsz).sum();
            let total_wsz: u64 = disks.iter().map(|d| d.wsz).sum();
            let read_s = (total_rsz.saturating_sub(p.disk_rsz) as f64 * 512.0) / dt;
            let write_s = (total_wsz.saturating_sub(p.disk_wsz) as f64 * 512.0) / dt;
            // Per-device utilization: max across all devices
            let mut max_util = 0.0_f64;
            for d in disks {
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
            Self::update_val(self.n, self.alpha, util, &mut self.disk_util_pct);
            Self::update_val(self.n, self.alpha, read_s, &mut self.disk_read_bytes_s);
            Self::update_val(self.n, self.alpha, write_s, &mut self.disk_write_bytes_s);
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
    /// Per-device io_ms (device_hash → cumulative io_ms).
    pub disk_io_ms_per_dev: std::collections::HashMap<u64, u64>,
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
            disk_io_ms_per_dev: std::collections::HashMap::new(),
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
            s.disk_rsz = disks.iter().map(|d| d.rsz).sum();
            s.disk_wsz = disks.iter().map(|d| d.wsz).sum();
            for d in disks {
                s.disk_io_ms_per_dev.insert(d.device_hash, d.io_ms);
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

fn sum_cpu_ticks(cpu: &crate::storage::model::SystemCpuInfo) -> u64 {
    cpu.user + cpu.nice + cpu.system + cpu.idle + cpu.iowait + cpu.irq + cpu.softirq + cpu.steal
}

// ============================================================
// Merge anomalies into incidents
// ============================================================

fn merge_anomalies(mut anomalies: Vec<Anomaly>) -> Vec<Incident> {
    anomalies.sort_by(|a, b| a.rule_id.cmp(b.rule_id).then(a.timestamp.cmp(&b.timestamp)));

    const BASE_GAP: i64 = 60;
    const MAX_GAP: i64 = 300;

    let mut incidents: Vec<Incident> = Vec::new();

    for anomaly in anomalies {
        let should_merge = incidents.last().is_some_and(|last| {
            let duration = last.last_ts - last.first_ts;
            let adaptive_gap = (duration / 5).clamp(BASE_GAP, MAX_GAP);
            last.rule_id == anomaly.rule_id && (anomaly.timestamp - last.last_ts) <= adaptive_gap
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
            }
            if incident.detail.is_none() && anomaly.detail.is_some() {
                incident.detail = anomaly.detail;
            }
        } else {
            incidents.push(Incident {
                rule_id: anomaly.rule_id.to_string(),
                category: anomaly.category,
                severity: anomaly.severity,
                first_ts: anomaly.timestamp,
                last_ts: anomaly.timestamp,
                peak_ts: anomaly.timestamp,
                peak_value: anomaly.value,
                title: anomaly.title,
                detail: anomaly.detail,
                snapshot_count: 1,
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

        let mut ewma = EwmaState::new(0.1);
        let mut prev_sample: Option<PrevSample> = None;
        let mut anomalies: Vec<Anomaly> = Vec::new();
        let mut snapshots_analyzed: usize = 0;

        for pos in start_pos..end_pos {
            let Some((snapshot, interner)) = provider.snapshot_with_interner_at(pos) else {
                continue;
            };

            let dt = prev_sample
                .as_ref()
                .map(|p| (snapshot.timestamp - p.timestamp) as f64)
                .unwrap_or(0.0);

            ewma.update(&snapshot, prev_sample.as_ref(), dt);

            let ctx = AnalysisContext {
                snapshot: &snapshot,
                interner: &interner,
                timestamp: snapshot.timestamp,
                ewma: &ewma,
                prev: prev_sample.as_ref(),
                dt,
            };

            for rule in &self.rules {
                anomalies.extend(rule.evaluate(&ctx));
            }

            prev_sample = Some(PrevSample::extract(&snapshot));
            snapshots_analyzed += 1;
        }

        // Layer 2: merge
        let incidents = merge_anomalies(anomalies);

        // Layer 3: advisors
        let mut recommendations = Vec::new();
        for adv in &self.advisors {
            recommendations.extend(adv.evaluate(&incidents));
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

        AnalysisReport {
            start_ts,
            end_ts,
            snapshots_analyzed,
            incidents,
            recommendations,
            summary,
        }
    }
}
