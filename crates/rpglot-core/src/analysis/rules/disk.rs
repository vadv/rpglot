use std::collections::HashMap;

use crate::analysis::{
    AnalysisContext, Anomaly, Category, Severity, find_block, is_container_snapshot,
    is_relevant_disk,
};
use crate::storage::model::DataBlock;

use super::AnalysisRule;

// ============================================================
// DiskUtilHighRule
// ============================================================

pub struct DiskUtilHighRule;

impl AnalysisRule for DiskUtilHighRule {
    fn id(&self) -> &'static str {
        "disk_util_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev = match ctx.prev {
            Some(p) => p,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(disks) = find_block(ctx.snapshot, |b| match b {
            DataBlock::SystemDisk(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        // Per-device utilization: take the max across relevant devices.
        let is_container = is_container_snapshot(ctx.snapshot);
        let mut util_pct = 0.0_f64;
        for d in disks {
            if !is_relevant_disk(d, is_container) {
                continue;
            }
            let prev_io_ms = prev
                .disk_io_ms_per_dev
                .get(&d.device_hash)
                .copied()
                .unwrap_or(0);
            let io_ms_d = d.io_ms.saturating_sub(prev_io_ms) as f64;
            let dev_util = (io_ms_d / (ctx.dt * 1000.0) * 100.0).min(100.0);
            util_pct = util_pct.max(dev_util);
        }

        let severity = if util_pct >= 90.0 {
            Severity::Critical
        } else if util_pct >= 70.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "disk_util_high",
            category: Category::Disk,
            severity,
            title: format!("Disk utilization {util_pct:.1}%"),
            detail: None,
            value: util_pct,
            merge_key: None,
            entity_id: None,
        }]
    }
}

// ============================================================
// DiskIoSpikeRule
// ============================================================

pub struct DiskIoSpikeRule;

impl AnalysisRule for DiskIoSpikeRule {
    fn id(&self) -> &'static str {
        "disk_io_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev = match ctx.prev {
            Some(p) => p,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(disks) = find_block(ctx.snapshot, |b| match b {
            DataBlock::SystemDisk(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let is_container = is_container_snapshot(ctx.snapshot);
        let relevant = disks.iter().filter(|d| is_relevant_disk(d, is_container));
        let total_rsz: u64 = relevant.clone().map(|d| d.rsz).sum();
        let total_wsz: u64 = relevant.map(|d| d.wsz).sum();
        let rsz_d = total_rsz.saturating_sub(prev.disk_rsz);
        let wsz_d = total_wsz.saturating_sub(prev.disk_wsz);
        let bytes_s = (rsz_d + wsz_d) as f64 * 512.0 / ctx.dt;

        let avg = ctx.ewma.disk_read_bytes_s + ctx.ewma.disk_write_bytes_s;
        if !ctx.ewma.is_spike(bytes_s, avg, 2.0) {
            return Vec::new();
        }

        let mb_s = bytes_s / 1_048_576.0;
        let avg_mb_s = avg / 1_048_576.0;
        let factor = if avg > 0.0 { bytes_s / avg } else { 0.0 };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "disk_io_spike",
            category: Category::Disk,
            severity: Severity::Warning,
            title: format!("Disk I/O spike {mb_s:.1} MB/s ({factor:.1}x above normal)",),
            detail: Some(format!(
                "Current: {mb_s:.1} MB/s, baseline avg: {avg_mb_s:.1} MB/s",
            )),
            value: bytes_s,
            merge_key: None,
            entity_id: None,
        }]
    }
}

// ============================================================
// DiskLatencyHighRule — r_await / w_await thresholds
// ============================================================

pub struct DiskLatencyHighRule;

impl AnalysisRule for DiskLatencyHighRule {
    fn id(&self) -> &'static str {
        "disk_latency_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev = match ctx.prev {
            Some(p) => p,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(disks) = find_block(ctx.snapshot, |b| match b {
            DataBlock::SystemDisk(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let is_container = is_container_snapshot(ctx.snapshot);
        let prev_per_dev = &prev.disk_read_time_per_dev;
        let prev_wt_per_dev = &prev.disk_write_time_per_dev;
        let prev_rio_per_dev = &prev.disk_rio_per_dev;
        let prev_wio_per_dev = &prev.disk_wio_per_dev;

        let mut max_r_await = 0.0_f64;
        let mut max_w_await = 0.0_f64;

        for d in disks.iter().filter(|d| is_relevant_disk(d, is_container)) {
            let h = d.device_hash;
            let d_rio = d.rio.saturating_sub(get_or(prev_rio_per_dev, h));
            let d_wio = d.wio.saturating_sub(get_or(prev_wio_per_dev, h));
            let d_rt = d.read_time.saturating_sub(get_or(prev_per_dev, h)) as f64;
            let d_wt = d.write_time.saturating_sub(get_or(prev_wt_per_dev, h)) as f64;
            if d_rio > 0 {
                max_r_await = max_r_await.max(d_rt / d_rio as f64);
            }
            if d_wio > 0 {
                max_w_await = max_w_await.max(d_wt / d_wio as f64);
            }
        }

        let worst_await = max_r_await.max(max_w_await);

        let severity = if worst_await >= 50.0 {
            Severity::Critical
        } else if worst_await >= 20.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        let mut parts = Vec::new();
        if max_r_await >= 20.0 {
            parts.push(format!("r_await {max_r_await:.1} ms"));
        }
        if max_w_await >= 20.0 {
            parts.push(format!("w_await {max_w_await:.1} ms"));
        }
        let title = format!("High disk latency: {}", parts.join(", "));

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "disk_latency_high",
            category: Category::Disk,
            severity,
            title,
            detail: Some(format!(
                "r_await: {max_r_await:.1} ms, w_await: {max_w_await:.1} ms"
            )),
            value: worst_await,
            merge_key: None,
            entity_id: None,
        }]
    }
}

fn get_or(map: &HashMap<u64, u64>, key: u64) -> u64 {
    map.get(&key).copied().unwrap_or(0)
}
