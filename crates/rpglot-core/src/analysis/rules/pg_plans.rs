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
        // Skip when data hasn't changed (within collector cache window).
        // collected_at is set at actual PG query time, identical for cached returns.
        let cur_collected = plans.iter().map(|p| p.collected_at).max().unwrap_or(0);
        let prev_collected = prev_plans.iter().map(|p| p.collected_at).max().unwrap_or(0);
        if cur_collected == prev_collected || cur_collected == 0 || prev_collected == 0 {
            return Vec::new();
        }
        let collection_dt = (cur_collected - prev_collected).max(1) as f64;

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

        // Minimum call rate for the slow plan to be considered "actively regressing".
        // 0.05 calls/s ≈ 3 calls/min. Below this the impact is negligible.
        const MIN_SLOW_RATE: f64 = 0.05;

        // Find the group with the worst ratio, but only flag regression
        // if the SLOW plan is actively being used at a meaningful rate.
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

            // Skip if the slow plan is not actively being used.
            // Check rate (not just delta) to filter plans with a few stray calls
            // over long collection intervals (pg_store_plans caches for 5 min).
            let slow_rate = slow_delta as f64 / collection_dt;
            if slow_rate < MIN_SLOW_RATE {
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

    fn make_plan_at(
        stmt_queryid: i64,
        planid: i64,
        mean_time: f64,
        calls: i64,
        collected_at: i64,
    ) -> PgStorePlansInfo {
        PgStorePlansInfo {
            stmt_queryid,
            planid,
            mean_time,
            calls,
            collected_at,
            ..Default::default()
        }
    }

    fn make_snapshot(plans: Vec<PgStorePlansInfo>) -> Snapshot {
        Snapshot {
            timestamp: 1000,
            blocks: vec![DataBlock::PgStorePlans(plans)],
        }
    }

    // Collection timestamps: 5-minute intervals
    const T0: i64 = 1000;
    const T1: i64 = 1300; // +300s

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
            make_plan_at(100, 1, 10.0, 50, T1),
            make_plan_at(100, 2, 100.0, 20, T1),
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx(&snap, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty(), "no prev_snapshot");
    }

    #[test]
    fn plan_regression_skips_same_collected_at() {
        // Both snapshots have same collected_at → cached data, skip.
        let prev = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 30, T0),
            make_plan_at(100, 2, 100.0, 10, T0),
        ]);
        let snap = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 30, T0),
            make_plan_at(100, 2, 100.0, 10, T0),
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty(), "same collected_at → skip");
    }

    #[test]
    fn plan_regression_detects_multiple_plans() {
        // Both plans gained calls at meaningful rate → both active.
        // 300s interval, +20 and +30 calls → rates 0.067 and 0.1
        let prev = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 30, T0),
            make_plan_at(100, 2, 100.0, 10, T0),
        ]);
        let snap = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 50, T1),  // +20, rate=0.067
            make_plan_at(100, 2, 100.0, 40, T1), // +30, rate=0.1
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
        let prev = make_snapshot(vec![make_plan_at(100, 1, 50.0, 80, T0)]);
        let snap = make_snapshot(vec![make_plan_at(100, 1, 50.0, 100, T1)]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty());
    }

    #[test]
    fn plan_regression_ignores_similar_plans() {
        let prev = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 40, T0),
            make_plan_at(100, 2, 15.0, 20, T0),
        ]);
        let snap = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 60, T1),
            make_plan_at(100, 2, 15.0, 40, T1),
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
            make_plan_at(100, 1, 10.0, 40, T0),
            make_plan_at(100, 2, 50.0, 10, T0),
            make_plan_at(200, 3, 5.0, 80, T0),
            make_plan_at(200, 4, 100.0, 5, T0),
        ]);
        let snap = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 60, T1),
            make_plan_at(100, 2, 50.0, 30, T1),
            make_plan_at(200, 3, 5.0, 100, T1),
            make_plan_at(200, 4, 100.0, 25, T1), // +20, rate=0.067
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
        let prev = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 40, T0),
            make_plan_at(100, 2, 100.0, 20, T0),
        ]);
        let snap = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 55, T1),
            make_plan_at(100, 2, 100.0, 20, T1), // +0 → inactive
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty(), "slow plan inactive");
    }

    #[test]
    fn plan_regression_ignores_low_rate_slow_plan() {
        // Slow plan gets 2 calls in 300s → rate = 0.007 < MIN_SLOW_RATE (0.05).
        // This is negligible — don't flag.
        let prev = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 500, T0),
            make_plan_at(100, 2, 100.0, 20, T0),
        ]);
        let snap = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 600, T1), // +100
            make_plan_at(100, 2, 100.0, 22, T1), // +2, rate=0.007
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(
            anomalies.is_empty(),
            "slow plan rate too low to be impactful"
        );
    }

    #[test]
    fn plan_regression_detects_meaningful_slow_plan() {
        // Slow plan gets 20 calls in 300s → rate = 0.067 >= MIN_SLOW_RATE.
        let prev = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 500, T0),
            make_plan_at(100, 2, 100.0, 100, T0),
        ]);
        let snap = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 600, T1),
            make_plan_at(100, 2, 100.0, 120, T1), // +20, rate=0.067
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert_eq!(anomalies.len(), 1, "slow plan rate above threshold");
    }

    #[test]
    fn plan_regression_skips_plan_not_in_prev() {
        let prev = make_snapshot(vec![make_plan_at(100, 1, 10.0, 40, T0)]);
        let snap = make_snapshot(vec![
            make_plan_at(100, 1, 10.0, 55, T1),
            make_plan_at(100, 2, 100.0, 20, T1), // NOT in prev
        ]);
        let interner = StringInterner::new();
        let ewma = EwmaState::new(0.1);
        let ctx = make_ctx_with_prev(&snap, &prev, &interner, &ewma);

        let anomalies = PlanRegressionRule.evaluate(&ctx);
        assert!(anomalies.is_empty(), "plan not in prev");
    }
}
