use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::{DataBlock, PgStatStatementsInfo};

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
            merge_key: None,
        }]
    }
}

// ============================================================
// QueryCallSpikeRule — spike in calls/s for a single statement
// ============================================================

fn find_prev_stmt(prev: &[PgStatStatementsInfo], queryid: i64) -> Option<&PgStatStatementsInfo> {
    prev.iter().find(|s| s.queryid == queryid)
}

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
        let mut worst_calls: i64 = 0;

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
            if delta < 100 {
                continue; // noise filter
            }
            let rate = delta as f64 / dt;
            if rate > worst_rate {
                worst_rate = rate;
                worst_query_hash = s.query_hash;
                worst_calls = delta;
            }
        }

        // Threshold: > 200 calls/s for a single query
        if worst_rate < 200.0 {
            return Vec::new();
        }

        let severity = if worst_rate >= 2000.0 {
            Severity::Critical
        } else {
            Severity::Warning
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
            title: format!("Query spike: {worst_rate:.0} calls/s ({worst_calls} calls)"),
            detail,
            value: worst_rate,
            merge_key: None,
        }]
    }
}
