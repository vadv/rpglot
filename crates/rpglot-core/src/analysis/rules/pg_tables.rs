use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

// ============================================================
// DeadTuplesHighRule
// ============================================================

pub struct DeadTuplesHighRule;

impl AnalysisRule for DeadTuplesHighRule {
    fn id(&self) -> &'static str {
        "dead_tuples_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(tables) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut worst_pct: f64 = 0.0;
        let mut worst_name_hash: u64 = 0;

        for t in tables {
            let total = t.n_live_tup + t.n_dead_tup;
            if total <= 1000 {
                continue;
            }
            let dead_pct = t.n_dead_tup as f64 * 100.0 / total as f64;
            if dead_pct > worst_pct {
                worst_pct = dead_pct;
                worst_name_hash = t.relname_hash;
            }
        }

        if worst_pct < 20.0 {
            return Vec::new();
        }

        let severity = if worst_pct >= 50.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let name = ctx.interner.resolve(worst_name_hash).unwrap_or("unknown");

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "dead_tuples_high",
            category: Category::PgTables,
            severity,
            title: format!("Table {name} has {worst_pct:.0}% dead tuples"),
            detail: None,
            value: worst_pct,
        }]
    }
}

// ============================================================
// SeqScanDominantRule
// ============================================================

pub struct SeqScanDominantRule;

impl AnalysisRule for SeqScanDominantRule {
    fn id(&self) -> &'static str {
        "seq_scan_dominant"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(tables) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut worst_pct: f64 = 0.0;
        let mut worst_name_hash: u64 = 0;

        for t in tables {
            // Small tables: seq scan is optimal, planner picks it intentionally
            if t.n_live_tup < 10_000 {
                continue;
            }
            let total = t.seq_scan + t.idx_scan;
            if total <= 100 {
                continue;
            }
            let seq_pct = t.seq_scan as f64 * 100.0 / total as f64;
            if seq_pct > worst_pct {
                worst_pct = seq_pct;
                worst_name_hash = t.relname_hash;
            }
        }

        if worst_pct <= 80.0 {
            return Vec::new();
        }

        let name = ctx.interner.resolve(worst_name_hash).unwrap_or("unknown");

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "seq_scan_dominant",
            category: Category::PgTables,
            severity: Severity::Warning,
            title: format!("Table {name}: {worst_pct:.0}% sequential scans"),
            detail: None,
            value: worst_pct,
        }]
    }
}
