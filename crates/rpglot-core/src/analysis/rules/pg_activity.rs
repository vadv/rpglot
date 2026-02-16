use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

// ============================================================
// IdleInTransactionRule
// ============================================================

pub struct IdleInTransactionRule;

impl AnalysisRule for IdleInTransactionRule {
    fn id(&self) -> &'static str {
        "idle_in_transaction"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(sessions) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatActivity(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let idle_in_tx_hash = xxhash_rust::xxh3::xxh3_64(b"idle in transaction");

        let mut count = 0u64;
        let mut max_duration: i64 = 0;

        for s in sessions {
            if s.state_hash == idle_in_tx_hash && s.xact_start > 0 {
                let duration = ctx.snapshot.timestamp - s.xact_start;
                if duration > 30 {
                    count += 1;
                    max_duration = max_duration.max(duration);
                }
            }
        }

        if count == 0 {
            return Vec::new();
        }

        let severity = if count >= 3 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "idle_in_transaction",
            category: Category::PgActivity,
            severity,
            title: format!("{count} idle-in-transaction session(s), longest {max_duration}s"),
            detail: None,
            value: count as f64,
        }]
    }
}

// ============================================================
// LongQueryRule
// ============================================================

pub struct LongQueryRule;

impl AnalysisRule for LongQueryRule {
    fn id(&self) -> &'static str {
        "long_query"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(sessions) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatActivity(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let active_hash = xxhash_rust::xxh3::xxh3_64(b"active");
        let walsender_hash = xxhash_rust::xxh3::xxh3_64(b"walsender");
        let walreceiver_hash = xxhash_rust::xxh3::xxh3_64(b"walreceiver");

        let mut count = 0u64;
        let mut max_duration: i64 = 0;
        let mut longest_query_hash: u64 = 0;

        for s in sessions {
            // Skip replication processes — they hold long-running queries by design
            if s.backend_type_hash == walsender_hash || s.backend_type_hash == walreceiver_hash {
                continue;
            }
            if s.state_hash == active_hash && s.query_start > 0 {
                let duration = ctx.timestamp - s.query_start;
                if duration > 30 {
                    count += 1;
                    if duration > max_duration {
                        max_duration = duration;
                        longest_query_hash = s.query_hash;
                    }
                }
            }
        }

        if count == 0 {
            return Vec::new();
        }

        let severity = if max_duration > 300 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let detail = if longest_query_hash != 0 {
            ctx.interner.resolve(longest_query_hash).map(|q| {
                let truncated: String = q.chars().take(100).collect();
                truncated
            })
        } else {
            None
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "long_query",
            category: Category::PgActivity,
            severity,
            title: format!("{count} long query(s), longest {max_duration}s"),
            detail,
            value: max_duration as f64,
        }]
    }
}

// ============================================================
// WaitSyncReplicaRule
// ============================================================

pub struct WaitSyncReplicaRule;

impl AnalysisRule for WaitSyncReplicaRule {
    fn id(&self) -> &'static str {
        "wait_sync_replica"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(sessions) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatActivity(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let ipc_hash = xxhash_rust::xxh3::xxh3_64(b"IPC");
        let syncrep_hash = xxhash_rust::xxh3::xxh3_64(b"SyncRep");

        let count = sessions
            .iter()
            .filter(|s| s.wait_event_type_hash == ipc_hash && s.wait_event_hash == syncrep_hash)
            .count() as u64;

        if count == 0 {
            return Vec::new();
        }

        let severity = if count >= 3 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "wait_sync_replica",
            category: Category::PgActivity,
            severity,
            title: format!("{count} session(s) waiting on synchronous replication"),
            detail: None,
            value: count as f64,
        }]
    }
}

// ============================================================
// WaitLockRule
// ============================================================

pub struct WaitLockRule;

impl AnalysisRule for WaitLockRule {
    fn id(&self) -> &'static str {
        "wait_lock"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(sessions) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatActivity(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let lock_hash = xxhash_rust::xxh3::xxh3_64(b"Lock");

        let count = sessions
            .iter()
            .filter(|s| s.wait_event_type_hash == lock_hash)
            .count() as u64;

        if count == 0 {
            return Vec::new();
        }

        let severity = if count >= 5 {
            Severity::Critical
        } else if count >= 2 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "wait_lock",
            category: Category::PgActivity,
            severity,
            title: format!("{count} sessions waiting on locks"),
            detail: None,
            value: count as f64,
        }]
    }
}

// ============================================================
// HighActiveSessionsRule
// ============================================================

pub struct HighActiveSessionsRule;

impl AnalysisRule for HighActiveSessionsRule {
    fn id(&self) -> &'static str {
        "high_active_sessions"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(sessions) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatActivity(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let active_hash = xxhash_rust::xxh3::xxh3_64(b"active");

        let count = sessions
            .iter()
            .filter(|s| s.state_hash == active_hash)
            .count() as f64;

        let avg = ctx.ewma.active_sessions;
        if !ctx.ewma.is_spike(count, avg, 2.0) {
            return Vec::new();
        }

        let factor = if avg > 0.0 { count / avg } else { 0.0 };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "high_active_sessions",
            category: Category::PgActivity,
            severity: Severity::Warning,
            title: format!("{count:.0} active sessions ({factor:.1}x above normal)",),
            detail: Some(format!("Baseline avg: {avg:.0} sessions")),
            value: count,
        }]
    }
}

// ============================================================
// TpsSpikeRule — transaction throughput spike
// ============================================================

pub struct TpsSpikeRule;

impl AnalysisRule for TpsSpikeRule {
    fn id(&self) -> &'static str {
        "tps_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev = match ctx.prev {
            Some(p) => p,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(dbs) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatDatabase(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let commits: i64 = dbs.iter().map(|d| d.xact_commit).sum();
        let rollbacks: i64 = dbs.iter().map(|d| d.xact_rollback).sum();
        let d_c = (commits - prev.pg_xact_commit).max(0) as f64;
        let d_r = (rollbacks - prev.pg_xact_rollback).max(0) as f64;
        let tps = (d_c + d_r) / ctx.dt;

        let avg = ctx.ewma.tps;
        if !ctx.ewma.is_spike(tps, avg, 2.0) {
            return Vec::new();
        }

        let factor = if avg > 0.0 { tps / avg } else { 0.0 };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "tps_spike",
            category: Category::PgActivity,
            severity: Severity::Warning,
            title: format!("TPS spike: {tps:.0} tx/s ({factor:.1}x above normal)"),
            detail: Some(format!("Baseline avg: {avg:.0} tx/s")),
            value: tps,
        }]
    }
}
