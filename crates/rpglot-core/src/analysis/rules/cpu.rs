use crate::analysis::{find_block, AnalysisContext, Anomaly, Category, Severity};
use crate::storage::model::DataBlock;

use super::AnalysisRule;

// ============================================================
// Helpers
// ============================================================

fn cpu_deltas(ctx: &AnalysisContext) -> Option<(f64, f64, f64, f64)> {
    let prev = ctx.prev?;
    if ctx.dt <= 0.0 {
        return None;
    }

    let cpu = find_block(ctx.snapshot, |b| match b {
        DataBlock::SystemCpu(v) => v.iter().find(|c| c.cpu_id == -1),
        _ => None,
    })?;

    let total = cpu.user
        + cpu.nice
        + cpu.system
        + cpu.idle
        + cpu.iowait
        + cpu.irq
        + cpu.softirq
        + cpu.steal;

    let dt_ticks = total.saturating_sub(prev.cpu_total) as f64;
    if dt_ticks <= 0.0 {
        return None;
    }

    let idle_d = cpu.idle.saturating_sub(prev.cpu_idle) as f64;
    let iow_d = cpu.iowait.saturating_sub(prev.cpu_iowait) as f64;
    let steal_d = cpu.steal.saturating_sub(prev.cpu_steal) as f64;

    let cpu_pct = (1.0 - idle_d / dt_ticks) * 100.0;
    let iow_pct = (iow_d / dt_ticks) * 100.0;
    let steal_pct = (steal_d / dt_ticks) * 100.0;

    Some((dt_ticks, cpu_pct, iow_pct, steal_pct))
}

// ============================================================
// CpuHighRule
// ============================================================

pub struct CpuHighRule;

impl AnalysisRule for CpuHighRule {
    fn id(&self) -> &'static str {
        "cpu_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some((_, cpu_pct, _, _)) = cpu_deltas(ctx) else {
            return Vec::new();
        };

        let severity = if cpu_pct >= 90.0 {
            Severity::Critical
        } else if cpu_pct >= 70.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "cpu_high",
            category: Category::Cpu,
            severity,
            title: format!("CPU usage {cpu_pct:.1}%"),
            detail: None,
            value: cpu_pct,
        }]
    }
}

// ============================================================
// IowaitHighRule
// ============================================================

pub struct IowaitHighRule;

impl AnalysisRule for IowaitHighRule {
    fn id(&self) -> &'static str {
        "iowait_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some((_, _, iow_pct, _)) = cpu_deltas(ctx) else {
            return Vec::new();
        };

        let severity = if iow_pct >= 15.0 {
            Severity::Critical
        } else if iow_pct >= 5.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "iowait_high",
            category: Category::Cpu,
            severity,
            title: format!("IOWait {iow_pct:.1}%"),
            detail: None,
            value: iow_pct,
        }]
    }
}

// ============================================================
// StealHighRule
// ============================================================

pub struct StealHighRule;

impl AnalysisRule for StealHighRule {
    fn id(&self) -> &'static str {
        "steal_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some((_, _, _, steal_pct)) = cpu_deltas(ctx) else {
            return Vec::new();
        };

        let severity = if steal_pct >= 10.0 {
            Severity::Critical
        } else if steal_pct >= 3.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "steal_high",
            category: Category::Cpu,
            severity,
            title: format!("CPU steal {steal_pct:.1}%"),
            detail: None,
            value: steal_pct,
        }]
    }
}
