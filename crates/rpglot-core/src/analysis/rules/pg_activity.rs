use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::{DataBlock, Snapshot};

/// Effective CPU count: cgroup quota/period if set, otherwise host CPU cores.
fn effective_cpus(snapshot: &Snapshot) -> f64 {
    if let Some(cg) = find_block(snapshot, |b| match b {
        DataBlock::Cgroup(c) => Some(c),
        _ => None,
    }) && let Some(cpu) = &cg.cpu
        && cpu.quota > 0
        && cpu.period > 0
    {
        return cpu.quota as f64 / cpu.period as f64;
    }
    find_block(snapshot, |b| match b {
        DataBlock::SystemCpu(v) => Some(v.iter().filter(|c| c.cpu_id >= 0).count()),
        _ => None,
    })
    .unwrap_or(1)
    .max(1) as f64
}

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
        let mut max_duration: f64 = 0.0;

        for s in sessions {
            if s.state_hash == idle_in_tx_hash && s.xact_start > 0.0 {
                let duration = ctx.snapshot.timestamp as f64 - s.xact_start;
                if duration > 30.0 {
                    count += 1;
                    if duration > max_duration {
                        max_duration = duration;
                    }
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

        let max_dur_display = max_duration as i64;
        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "idle_in_transaction",
            category: Category::PgActivity,
            severity,
            title: format!("{count} idle-in-transaction session(s), longest {max_dur_display}s"),
            detail: None,
            value: count as f64,
            merge_key: None,
            entity_id: None,
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
        let autovacuum_hash = xxhash_rust::xxh3::xxh3_64(b"autovacuum worker");

        let mut count = 0u64;
        let mut max_duration: f64 = 0.0;
        let mut longest_query_hash: u64 = 0;
        let mut worst_pid: i32 = 0;

        for s in sessions {
            // Skip replication and autovacuum — long-running queries by design
            if s.backend_type_hash == walsender_hash
                || s.backend_type_hash == walreceiver_hash
                || s.backend_type_hash == autovacuum_hash
            {
                continue;
            }
            if s.state_hash == active_hash && s.query_start > 0.0 {
                let duration = ctx.timestamp as f64 - s.query_start;
                if duration > 30.0 {
                    count += 1;
                    if duration > max_duration {
                        max_duration = duration;
                        longest_query_hash = s.query_hash;
                        worst_pid = s.pid;
                    }
                }
            }
        }

        if count == 0 {
            return Vec::new();
        }

        let severity = if max_duration > 300.0 {
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
            title: format!("{count} long query(s), longest {}s", max_duration as i64),
            detail,
            value: max_duration,
            merge_key: None,
            entity_id: Some(worst_pid as i64),
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

        // Normalize by CPU count: on a 20-core machine, 2 waiters = 10% — normal.
        let cpus = effective_cpus(ctx.snapshot);
        let pct = count as f64 / cpus * 100.0;

        let severity = if pct >= 50.0 {
            Severity::Critical
        } else if pct >= 20.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "wait_sync_replica",
            category: Category::PgActivity,
            severity,
            title: format!(
                "{count} session(s) waiting on synchronous replication ({pct:.0}% of CPUs)"
            ),
            detail: None,
            value: count as f64,
            merge_key: None,
            entity_id: None,
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

        // Normalize by CPU count: on a 20-core machine, 2 lock waiters = 10% — normal.
        let cpus = effective_cpus(ctx.snapshot);
        let pct = count as f64 / cpus * 100.0;

        let severity = if pct >= 50.0 {
            Severity::Critical
        } else if pct >= 20.0 {
            Severity::Warning
        } else {
            return Vec::new();
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "wait_lock",
            category: Category::PgActivity,
            severity,
            title: format!("{count} sessions waiting on locks ({pct:.0}% of CPUs)"),
            detail: None,
            value: count as f64,
            merge_key: None,
            entity_id: None,
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
            merge_key: None,
            entity_id: None,
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
            merge_key: None,
            entity_id: None,
        }]
    }
}
