use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

// ============================================================
// BlockedSessionsRule
// ============================================================

pub struct BlockedSessionsRule;

impl AnalysisRule for BlockedSessionsRule {
    fn id(&self) -> &'static str {
        "blocked_sessions"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(nodes) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgLockTree(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let count = nodes.iter().filter(|n| !n.lock_granted).count() as u64;

        if count == 0 {
            return Vec::new();
        }

        let severity = if count >= 5 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "blocked_sessions",
            category: Category::PgLocks,
            severity,
            title: format!("{count} blocked session(s)"),
            detail: None,
            value: count as f64,
        }]
    }
}
