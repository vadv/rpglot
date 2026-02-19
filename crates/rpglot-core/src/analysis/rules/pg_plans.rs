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

        // Need prev snapshot to compute call deltas and determine plan activity.
        // Without it we can't distinguish active from inactive plans.
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };
        let Some(prev_plans) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStorePlans(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };
        let prev_calls: HashMap<(i64, i64), i64> = prev_plans
            .iter()
            .map(|p| ((p.stmt_queryid, p.planid), p.calls))
            .collect();

        // Group plans by stmt_queryid, skip queryid == 0.
        // Track (mean_time, calls_delta, plan_hash) per plan.
        let mut by_queryid: HashMap<i64, Vec<(f64, i64, u64)>> = HashMap::new();
        for p in plans {
            if p.stmt_queryid == 0 || p.calls <= 0 || p.mean_time <= 0.0 {
                continue;
            }
            // Plan must exist in both snapshots to determine activity.
            // New/evicted plans get unknown delta — skip them.
            let Some(&prev) = prev_calls.get(&(p.stmt_queryid, p.planid)) else {
                continue;
            };
            let calls_delta = p.calls - prev;
            by_queryid.entry(p.stmt_queryid).or_default().push((
                p.mean_time,
                calls_delta,
                p.plan_hash,
            ));
        }

        // Find the group with the worst ratio, but only flag regression
        // if the SLOW plan is currently active (calls_delta > 0).
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
            let (max_mean, slow_delta, slow_hash) = group
                .iter()
                .copied()
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap();

            // Only flag regression if the slow plan is actively being used.
            // If slow_delta == 0, PostgreSQL already recovered to a faster plan.
            if slow_delta <= 0 {
                continue;
            }

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

    fn make_ctx_with_prev<'a>(
        snapshot: &'a Snapshot,
        prev: &'a Snapshot,
        interner: &'a StringInterner,
        ewma: &'a EwmaState,
    ) -> AnalysisContext<'a> {
        AnalysisContext {
            snapshot,
            prev_snapshot: Some(prev),
            interner,
            timestamp: snapshot.timestamp,
            ewma,
            prev: None,
            dt: 10.0,
            backend_io_hit_pct: None,
        }
    }

    #[test]
    fn plan_regression_skips_without_prev_snapshot() {
        let snap = make_snapshot(vec![
            make_plan(100, 1, 10.0, 50),
            make_plan(100, 2, 100.0, 20),
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx(&snap, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(
            anomalies.is_empty(),
            "no prev_snapshot → cannot determine plan activity"
        );
    }

    #[test]
    fn plan_regression_detects_multiple_plans() {
        // Both plans gained new calls relative to prev → both active.
        let prev = make_snapshot(vec![
            make_plan(100, 1, 10.0, 30),
            make_plan(100, 2, 100.0, 10),
        ]);
        let snap = make_snapshot(vec![
            make_plan(100, 1, 10.0, 50),  // +20
            make_plan(100, 2, 100.0, 20), // +10
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].rule_id, "plan_regression");
        assert!((anomalies[0].value - 10.0).abs() < 0.01);
        assert_eq!(anomalies[0].entity_id, Some(100));
        assert_eq!(anomalies[0].severity, Severity::Critical);
    }

    #[test]
    fn plan_regression_ignores_single_plan() {
        let prev = make_snapshot(vec![make_plan(100, 1, 50.0, 80)]);
        let snap = make_snapshot(vec![make_plan(100, 1, 50.0, 100)]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty());
    }

    #[test]
    fn plan_regression_ignores_similar_plans() {
        let prev = make_snapshot(vec![
            make_plan(100, 1, 10.0, 40),
            make_plan(100, 2, 15.0, 20),
        ]);
        let snap = make_snapshot(vec![
            make_plan(100, 1, 10.0, 50),
            make_plan(100, 2, 15.0, 30),
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty(), "ratio 1.5x < 2.0 threshold");
    }

    #[test]
    fn plan_regression_picks_worst_group() {
        let prev = make_snapshot(vec![
            make_plan(100, 1, 10.0, 40),
            make_plan(100, 2, 50.0, 10),
            make_plan(200, 3, 5.0, 80),
            make_plan(200, 4, 100.0, 5),
        ]);
        let snap = make_snapshot(vec![
            // Group A: ratio = 5x, both active
            make_plan(100, 1, 10.0, 50),
            make_plan(100, 2, 50.0, 20),
            // Group B: ratio = 20x (worst), both active
            make_plan(200, 3, 5.0, 100),
            make_plan(200, 4, 100.0, 10),
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].entity_id, Some(200));
        assert!((anomalies[0].value - 20.0).abs() < 0.01);
    }

    #[test]
    fn plan_regression_ignores_inactive_slow_plan() {
        // Slow plan (planid=2, mean_time=100) has same calls in both snapshots → delta=0.
        // Fast plan (planid=1, mean_time=10) gained new calls → active.
        // Regression is resolved — slow plan not being used anymore.
        let prev = make_snapshot(vec![
            make_plan(100, 1, 10.0, 40),
            make_plan(100, 2, 100.0, 20),
        ]);
        let snap = make_snapshot(vec![
            make_plan(100, 1, 10.0, 55),  // +15 calls → active
            make_plan(100, 2, 100.0, 20), // +0 calls → inactive
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(
            anomalies.is_empty(),
            "slow plan is inactive (calls_delta=0), regression resolved"
        );
    }

    #[test]
    fn plan_regression_detects_active_slow_plan() {
        // Slow plan (planid=2) gained new calls → actively being used → regression.
        let prev = make_snapshot(vec![
            make_plan(100, 1, 10.0, 50),
            make_plan(100, 2, 100.0, 15),
        ]);
        let snap = make_snapshot(vec![
            make_plan(100, 1, 10.0, 52),  // +2 calls
            make_plan(100, 2, 100.0, 20), // +5 calls → active slow plan
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert_eq!(anomalies.len(), 1, "slow plan is active → regression");
        assert!((anomalies[0].value - 10.0).abs() < 0.01);
    }

    #[test]
    fn plan_regression_skips_plan_not_in_prev() {
        // Slow plan (planid=2) appears only in current snapshot (e.g. after cache rotation).
        // Without prev data for this plan, we can't determine activity → skip it.
        let prev = make_snapshot(vec![
            make_plan(100, 1, 10.0, 40), // only fast plan in prev
        ]);
        let snap = make_snapshot(vec![
            make_plan(100, 1, 10.0, 55),  // +15 calls
            make_plan(100, 2, 100.0, 20), // NOT in prev → skipped
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(
            anomalies.is_empty(),
            "plan not in prev → unknown activity → no regression"
        );
    }
}
