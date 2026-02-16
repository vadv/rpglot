use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

use super::AnalysisRule;

// ============================================================
// MemoryLowRule
// ============================================================

pub struct MemoryLowRule;

impl AnalysisRule for MemoryLowRule {
    fn id(&self) -> &'static str {
        "memory_low"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(mem) = find_block(ctx.snapshot, |b| match b {
            DataBlock::SystemMem(m) => Some(m),
            _ => None,
        }) else {
            return Vec::new();
        };

        if mem.total == 0 {
            return Vec::new();
        }

        let avail_pct = mem.available as f64 / mem.total as f64 * 100.0;

        let severity = if avail_pct < 10.0 {
            Severity::Critical
        } else if avail_pct < 20.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "memory_low",
            category: Category::Memory,
            severity,
            title: format!("Available memory {avail_pct:.1}%"),
            detail: Some(format!(
                "Available: {} MB / {} MB",
                mem.available / 1024,
                mem.total / 1024,
            )),
            value: avail_pct,
        }]
    }
}

// ============================================================
// SwapUsageRule
// ============================================================

pub struct SwapUsageRule;

impl AnalysisRule for SwapUsageRule {
    fn id(&self) -> &'static str {
        "swap_usage"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(mem) = find_block(ctx.snapshot, |b| match b {
            DataBlock::SystemMem(m) => Some(m),
            _ => None,
        }) else {
            return Vec::new();
        };

        if mem.swap_total == 0 {
            return Vec::new();
        }

        let swap_used = mem.swap_total.saturating_sub(mem.swap_free);
        if swap_used == 0 {
            return Vec::new();
        }

        let swap_used_pct = swap_used as f64 / mem.swap_total as f64 * 100.0;

        let severity = if swap_used_pct > 50.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "swap_usage",
            category: Category::Memory,
            severity,
            title: format!("Swap usage {swap_used_pct:.1}%"),
            detail: Some(format!(
                "Swap used: {} MB / {} MB",
                swap_used / 1024,
                mem.swap_total / 1024,
            )),
            value: swap_used_pct,
        }]
    }
}
