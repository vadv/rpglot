use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
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

        // Compute aggregate util from total io_ms delta.
        let total_io_ms: u64 = disks.iter().map(|d| d.io_ms).sum();
        let io_ms_d = total_io_ms.saturating_sub(prev.disk_io_ms) as f64;
        let util_pct = (io_ms_d / (ctx.dt * 1000.0) * 100.0).min(100.0);

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

        let total_rsz: u64 = disks.iter().map(|d| d.rsz).sum();
        let total_wsz: u64 = disks.iter().map(|d| d.wsz).sum();
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
        }]
    }
}
