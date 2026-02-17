use std::cmp::Ordering;

use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::{DataBlock, PgStatStatementsInfo};

fn find_prev_stmt(prev: &[PgStatStatementsInfo], queryid: i64) -> Option<&PgStatStatementsInfo> {
    prev.iter().find(|s| s.queryid == queryid)
}

// ============================================================
// MeanTimeSpikeRule
// ============================================================

pub struct MeanTimeSpikeRule;

impl AnalysisRule for MeanTimeSpikeRule {
    fn id(&self) -> &'static str {
        "stmt_mean_time_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        // Need prev_snapshot to determine which statements are actively running
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };

        let Some(stmts) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatStatements(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_stmts) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatStatements(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        // Only consider statements whose calls increased (actively executing)
        let worst = stmts
            .iter()
            .filter(|s| {
                find_prev_stmt(prev_stmts, s.queryid).is_none_or(|prev| s.calls > prev.calls)
            })
            .max_by(|a, b| {
                a.mean_exec_time
                    .partial_cmp(&b.mean_exec_time)
                    .unwrap_or(Ordering::Equal)
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
            merge_key: None,
            entity_id: Some(worst.queryid),
        }]
    }
}

// ============================================================
// QueryCallSpikeRule — spike in calls/s for a single statement
// ============================================================
//
// Uses relative threshold: alert when a single query accounts for
// a large share of total calls/s.  This avoids false positives on
// high-TPS machines where 500 calls/s is a tiny fraction of load.
//
// Warning:  one query > 30% of total calls/s (min 200 calls/s)
// Critical: one query > 50% of total calls/s (min 500 calls/s)

pub struct QueryCallSpikeRule;

impl AnalysisRule for QueryCallSpikeRule {
    fn id(&self) -> &'static str {
        "stmt_call_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(stmts) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatStatements(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_stmts) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatStatements(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut worst_rate = 0.0_f64;
        let mut worst_query_hash: u64 = 0;
        let mut worst_queryid: i64 = 0;
        let mut worst_calls: i64 = 0;
        let mut total_rate = 0.0_f64;

        for s in stmts {
            let Some(prev) = find_prev_stmt(prev_stmts, s.queryid) else {
                continue;
            };
            if s.collected_at == prev.collected_at {
                continue;
            }
            let dt = (s.collected_at - prev.collected_at) as f64;
            if dt <= 0.0 {
                continue;
            }
            let delta = (s.calls - prev.calls).max(0);
            let rate = delta as f64 / dt;
            total_rate += rate;
            if delta >= 100 && rate > worst_rate {
                worst_rate = rate;
                worst_query_hash = s.query_hash;
                worst_queryid = s.queryid;
                worst_calls = delta;
            }
        }

        if worst_rate < 200.0 || total_rate <= 0.0 {
            return Vec::new();
        }

        let pct = worst_rate / total_rate * 100.0;

        let severity = if pct >= 50.0 && worst_rate >= 500.0 {
            Severity::Critical
        } else if pct >= 30.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        let detail = if worst_query_hash != 0 {
            ctx.interner.resolve(worst_query_hash).map(|q| {
                let truncated: String = q.chars().take(100).collect();
                truncated
            })
        } else {
            None
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "stmt_call_spike",
            category: Category::PgStatements,
            severity,
            title: format!(
                "Query spike: {worst_rate:.0} calls/s ({pct:.0}% of total, {worst_calls} calls)"
            ),
            detail,
            value: worst_rate,
            merge_key: None,
            entity_id: Some(worst_queryid),
        }]
    }
}
