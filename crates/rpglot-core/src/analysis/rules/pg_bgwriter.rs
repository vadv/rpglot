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
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };

        let Some(bgw) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatBgwriter(info) => Some(info),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_bgw) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatBgwriter(info) => Some(info),
            _ => None,
        }) else {
            return Vec::new();
        };

        // Delta of cumulative counters
        let d_req = (bgw.checkpoints_req - prev_bgw.checkpoints_req).max(0);
        let d_timed = (bgw.checkpoints_timed - prev_bgw.checkpoints_timed).max(0);

        if d_req == 0 && d_timed == 0 {
            return Vec::new();
        }

        // Forced checkpoints are concerning — they indicate WAL pressure
        let severity = if d_req > 2 {
            Severity::Critical
        } else if d_req >= 1 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        let detail = format!("Δforced: {d_req}, Δtimed: {d_timed}");

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "checkpoint_spike",
            category: Category::PgBgwriter,
            severity,
            title: format!("{d_req} forced checkpoint(s), {d_timed} timed"),
            detail: Some(detail),
            value: d_req as f64,
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
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };

        let Some(bgw) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatBgwriter(info) => Some(info),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_bgw) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatBgwriter(info) => Some(info),
            _ => None,
        }) else {
            return Vec::new();
        };

        let d_backend = (bgw.buffers_backend - prev_bgw.buffers_backend).max(0);
        let d_checkpoint = (bgw.buffers_checkpoint - prev_bgw.buffers_checkpoint).max(0);
        let d_clean = (bgw.buffers_clean - prev_bgw.buffers_clean).max(0);
        let d_bg = d_checkpoint + d_clean;

        if d_backend == 0 || d_backend <= d_bg {
            return Vec::new();
        }

        let detail =
            format!("Δbackend: {d_backend}, Δcheckpoint: {d_checkpoint}, Δclean: {d_clean}");

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "backend_buffers_high",
            category: Category::PgBgwriter,
            severity: Severity::Warning,
            title: format!("Backend wrote {d_backend} buffers > bgwriter {d_bg}"),
            detail: Some(detail),
            value: d_backend as f64,
        }]
    }
}
