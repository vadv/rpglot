use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

// ============================================================
// ThrottledRule — container CPU throttling
// ============================================================

pub struct ThrottledRule;

impl AnalysisRule for ThrottledRule {
    fn id(&self) -> &'static str {
        "cgroup_throttled"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev = match ctx.prev {
            Some(p) => p,
            None => return Vec::new(),
        };

        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let cg = match find_block(ctx.snapshot, |b| match b {
            DataBlock::Cgroup(c) => Some(c),
            _ => None,
        }) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let cpu_info = match &cg.cpu {
            Some(c) => c,
            None => return Vec::new(),
        };

        let throttle_d = cpu_info
            .throttled_usec
            .saturating_sub(prev.cgroup_throttled_usec) as f64;
        let wall_usec = ctx.dt * 1_000_000.0;
        let throttle_pct = (throttle_d / wall_usec) * 100.0;

        let severity = if throttle_pct > 20.0 {
            Severity::Critical
        } else if throttle_pct > 5.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "cgroup_throttled",
            category: Category::Cgroup,
            severity,
            title: format!("Container CPU throttled {throttle_pct:.1}% of time"),
            detail: None,
            value: throttle_pct,
            merge_key: None,
            entity_id: None,
        }]
    }
}

// ============================================================
// OomKillRule — container OOM kills
// ============================================================

pub struct OomKillRule;

impl AnalysisRule for OomKillRule {
    fn id(&self) -> &'static str {
        "cgroup_oom_kill"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let cg = match find_block(ctx.snapshot, |b| match b {
            DataBlock::Cgroup(c) => Some(c),
            _ => None,
        }) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mem = match &cg.memory {
            Some(m) => m,
            None => return Vec::new(),
        };

        if mem.oom_kill == 0 {
            return Vec::new();
        }

        let count = mem.oom_kill;

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "cgroup_oom_kill",
            category: Category::Cgroup,
            severity: Severity::Warning,
            title: format!("Container OOM kill detected ({count} events)"),
            detail: None,
            value: count as f64,
            merge_key: None,
            entity_id: None,
        }]
    }
}
