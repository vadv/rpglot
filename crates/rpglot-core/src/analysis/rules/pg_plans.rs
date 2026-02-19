use std::collections::HashMap;

use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

// ============================================================
// PlanRegressionRule — detect plan flip via pg_store_plans
// ============================================================

pub struct PlanRegressionRule;

impl AnalysisRule for PlanRegressionRule {
    fn id(&self) -> &'static str {
        "plan_regression"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(plans) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStorePlans(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        // Group plans by stmt_queryid, skip queryid == 0
        let mut by_queryid: HashMap<i64, Vec<(f64, i64, u64)>> = HashMap::new();
        for p in plans {
            if p.stmt_queryid == 0 || p.calls <= 0 || p.mean_time <= 0.0 {
                continue;
            }
            by_queryid
                .entry(p.stmt_queryid)
                .or_default()
                .push((p.mean_time, p.calls, p.plan_hash));
        }

        // Find the group with the worst ratio
        let mut worst_ratio = 0.0_f64;
        let mut worst_qid: i64 = 0;
        let mut worst_plan_count: usize = 0;
        let mut worst_slow_plan_hash: u64 = 0;

        for (qid, group) in &by_queryid {
            if group.len() < 2 {
                continue;
            }

            let min_mean = group
                .iter()
                .map(|(m, _, _)| *m)
                .fold(f64::INFINITY, f64::min);
            let (max_mean, _, slow_hash) = group
                .iter()
                .copied()
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap();

            if min_mean <= 0.0 {
                continue;
            }
            let ratio = max_mean / min_mean;
            if ratio < 2.0 {
                continue;
            }

            if ratio > worst_ratio {
                worst_ratio = ratio;
                worst_qid = *qid;
                worst_plan_count = group.len();
                worst_slow_plan_hash = slow_hash;
            }
        }

        if worst_ratio < 2.0 {
            return Vec::new();
        }

        let severity = if worst_ratio >= 10.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let detail = if worst_slow_plan_hash != 0 {
            ctx.interner.resolve(worst_slow_plan_hash).map(|text| {
                let truncated: String = text.chars().take(120).collect();
                truncated
            })
        } else {
            None
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "plan_regression",
            category: Category::PgStatements,
            severity,
            title: format!(
                "Plan regression: {worst_ratio:.1}x (queryid {worst_qid}, {worst_plan_count} plans)"
            ),
            detail,
            value: worst_ratio,
            merge_key: None,
            entity_id: Some(worst_qid),
        }]
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::EwmaState;
    use crate::storage::StringInterner;
    use crate::storage::model::{PgStorePlansInfo, Snapshot};

    fn make_plan(stmt_queryid: i64, planid: i64, mean_time: f64, calls: i64) -> PgStorePlansInfo {
        PgStorePlansInfo {
            stmt_queryid,
            planid,
            mean_time,
            calls,
            ..Default::default()
        }
    }

    fn make_snapshot(plans: Vec<PgStorePlansInfo>) -> Snapshot {
        Snapshot {
            timestamp: 1000,
            blocks: vec![DataBlock::PgStorePlans(plans)],
        }
    }

    fn make_ctx<'a>(
        snapshot: &'a Snapshot,
        interner: &'a StringInterner,
        ewma: &'a EwmaState,
    ) -> AnalysisContext<'a> {
        AnalysisContext {
            snapshot,
            prev_snapshot: None,
            interner,
            timestamp: snapshot.timestamp,
            ewma,
            prev: None,
            dt: 0.0,
            backend_io_hit_pct: None,
        }
    }

    #[test]
    fn plan_regression_detects_multiple_plans() {
        let snap = make_snapshot(vec![
            make_plan(100, 1, 10.0, 50),
            make_plan(100, 2, 100.0, 20),
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx(&snap, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].rule_id, "plan_regression");
        assert!((anomalies[0].value - 10.0).abs() < 0.01);
        assert_eq!(anomalies[0].entity_id, Some(100));
        assert_eq!(anomalies[0].severity, Severity::Critical);
    }

    #[test]
    fn plan_regression_ignores_single_plan() {
        let snap = make_snapshot(vec![make_plan(100, 1, 50.0, 100)]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx(&snap, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty());
    }

    #[test]
    fn plan_regression_ignores_similar_plans() {
        let snap = make_snapshot(vec![
            make_plan(100, 1, 10.0, 50),
            make_plan(100, 2, 15.0, 30),
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx(&snap, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty(), "ratio 1.5x < 2.0 threshold");
    }

    #[test]
    fn plan_regression_picks_worst_group() {
        let snap = make_snapshot(vec![
            // Group A: ratio = 5x
            make_plan(100, 1, 10.0, 50),
            make_plan(100, 2, 50.0, 20),
            // Group B: ratio = 20x (worst)
            make_plan(200, 3, 5.0, 100),
            make_plan(200, 4, 100.0, 10),
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx(&snap, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].entity_id, Some(200));
        assert!((anomalies[0].value - 20.0).abs() < 0.01);
    }
}
