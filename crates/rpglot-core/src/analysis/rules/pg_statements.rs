use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

// ============================================================
// MeanTimeSpikeRule
// ============================================================

pub struct MeanTimeSpikeRule;

impl AnalysisRule for MeanTimeSpikeRule {
    fn id(&self) -> &'static str {
        "stmt_mean_time_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(stmts) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatStatements(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let worst = stmts.iter().max_by(|a, b| {
            a.mean_exec_time
                .partial_cmp(&b.mean_exec_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let Some(worst) = worst else {
            return Vec::new();
        };

        let mean = worst.mean_exec_time;

        let severity = if mean > 5000.0 {
            Severity::Critical
        } else if mean > 1000.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        let detail = if worst.query_hash != 0 {
            ctx.interner.resolve(worst.query_hash).map(|q| {
                let truncated: String = q.chars().take(100).collect();
                truncated
            })
        } else {
            None
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "stmt_mean_time_spike",
            category: Category::PgStatements,
            severity,
            title: format!("Statement mean time {mean:.0}ms"),
            detail,
            value: mean,
        }]
    }
}
