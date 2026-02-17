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
            merge_key: None,
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

        let d_total = d_backend + d_bg;
        if d_total == 0 || d_backend <= d_bg {
            return Vec::new();
        }

        let pct = d_backend as f64 / d_total as f64 * 100.0;
        let backend_mb = d_backend as f64 * 8.0 / 1024.0;

        let severity = if pct > 80.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let detail = format!(
            "Backends flushed {backend_mb:.1} MiB dirty buffers to disk themselves instead of bgwriter/checkpointer. \
             This adds latency to queries. Tuning bgwriter_delay, bgwriter_lru_maxpages, \
             or shared_buffers may help."
        );

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "backend_buffers_high",
            category: Category::PgBgwriter,
            severity,
            title: format!(
                "Backends flush {pct:.0}% dirty buffers ({backend_mb:.1} MiB) — bgwriter too slow"
            ),
            detail: Some(detail),
            value: d_backend as f64,
            merge_key: None,
        }]
    }
}
