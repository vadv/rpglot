use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

// ============================================================
// CheckpointSpikeRule
// ============================================================

pub struct CheckpointSpikeRule;

impl AnalysisRule for CheckpointSpikeRule {
    fn id(&self) -> &'static str {
        "checkpoint_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(bgw) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatBgwriter(info) => Some(info),
            _ => None,
        }) else {
            return Vec::new();
        };

        let req = bgw.checkpoints_req;
        if req == 0 {
            return Vec::new();
        }

        let severity = if req > 2 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "checkpoint_spike",
            category: Category::PgBgwriter,
            severity,
            title: format!(
                "{req} forced checkpoint(s) vs {} timed",
                bgw.checkpoints_timed
            ),
            detail: None,
            value: req as f64,
        }]
    }
}

// ============================================================
// BackendBuffersRule
// ============================================================

pub struct BackendBuffersRule;

impl AnalysisRule for BackendBuffersRule {
    fn id(&self) -> &'static str {
        "backend_buffers_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(bgw) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatBgwriter(info) => Some(info),
            _ => None,
        }) else {
            return Vec::new();
        };

        let bg = bgw.buffers_checkpoint + bgw.buffers_clean;
        let backend = bgw.buffers_backend;

        if backend <= bg || bg == 0 {
            return Vec::new();
        }

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "backend_buffers_high",
            category: Category::PgBgwriter,
            severity: Severity::Warning,
            title: format!("Backend buffers {backend} > bgwriter {bg}"),
            detail: None,
            value: backend as f64,
        }]
    }
}
