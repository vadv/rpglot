use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

// ============================================================
// LoadAverageHighRule — load average exceeds CPU count
// ============================================================

pub struct LoadAverageHighRule;

impl AnalysisRule for LoadAverageHighRule {
    fn id(&self) -> &'static str {
        "load_average_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(load) = find_block(ctx.snapshot, |b| match b {
            DataBlock::SystemLoad(l) => Some(l),
            _ => None,
        }) else {
            return Vec::new();
        };

        // Count CPU cores (entries with cpu_id >= 0)
        let cpu_count = find_block(ctx.snapshot, |b| match b {
            DataBlock::SystemCpu(v) => Some(v.iter().filter(|c| c.cpu_id >= 0).count()),
            _ => None,
        })
        .unwrap_or(1)
        .max(1) as f64;

        let la1 = load.lavg1 as f64;

        // Threshold: load average > 2x CPU count
        let ratio = la1 / cpu_count;
        if ratio < 2.0 {
            return Vec::new();
        }

        let severity = if ratio >= 4.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "load_average_high",
            category: Category::Cpu,
            severity,
            title: format!("Load average {la1:.1} ({ratio:.1}x of {cpu_count:.0} CPUs)",),
            detail: Some(format!(
                "LA 1/5/15: {:.1}/{:.1}/{:.1}",
                load.lavg1, load.lavg5, load.lavg15,
            )),
            value: la1,
            merge_key: None,
            entity_id: None,
        }]
    }
}
