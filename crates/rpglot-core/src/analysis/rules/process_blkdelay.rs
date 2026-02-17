use std::collections::HashMap;

use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

/// Clock ticks per second (standard on Linux).
const CLK_TCK: f64 = 100.0;

/// Minimum blkdelay delta (seconds) to trigger a warning.
const WARN_SECONDS: f64 = 1.0;
/// Minimum blkdelay delta (seconds) to trigger critical.
const CRIT_SECONDS: f64 = 5.0;

// ============================================================
// HighBlkDelayRule
// ============================================================

pub struct HighBlkDelayRule;

impl AnalysisRule for HighBlkDelayRule {
    fn id(&self) -> &'static str {
        "high_blk_delay"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(procs) = find_block(ctx.snapshot, |b| match b {
            DataBlock::Processes(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_snap) = ctx.prev_snapshot else {
            return Vec::new();
        };

        let Some(prev_procs) = find_block(prev_snap, |b| match b {
            DataBlock::Processes(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let prev_by_pid: HashMap<u32, &crate::storage::model::ProcessInfo> =
            prev_procs.iter().map(|p| (p.pid, p)).collect();

        // Find the process with the highest blkdelay delta
        let mut top_pid: u32 = 0;
        let mut top_name_hash: u64 = 0;
        let mut top_delta_sec: f64 = 0.0;

        for p in procs {
            let Some(prev) = prev_by_pid.get(&p.pid) else {
                continue;
            };
            let delta_ticks = p.cpu.blkdelay.saturating_sub(prev.cpu.blkdelay);
            if delta_ticks == 0 {
                continue;
            }
            let delta_sec = delta_ticks as f64 / CLK_TCK;
            if delta_sec > top_delta_sec {
                top_delta_sec = delta_sec;
                top_pid = p.pid;
                top_name_hash = p.name_hash;
            }
        }

        if top_delta_sec < WARN_SECONDS {
            return Vec::new();
        }

        let severity = if top_delta_sec >= CRIT_SECONDS {
            Severity::Critical
        } else {
            Severity::Warning
        };

        // Enrich with PG info if this is a PG backend
        let pg_session = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatActivity(v) => Some(v.as_slice()),
            _ => None,
        })
        .and_then(|sessions| sessions.iter().find(|s| s.pid == top_pid as i32));

        let label = if let Some(session) = pg_session {
            let backend_type = ctx
                .interner
                .resolve(session.backend_type_hash)
                .unwrap_or("");
            let query: String = ctx
                .interner
                .resolve(session.query_hash)
                .unwrap_or("")
                .chars()
                .take(80)
                .collect();
            if !query.is_empty() {
                format!("[PG {backend_type}] {query} (PID {top_pid})")
            } else if !backend_type.is_empty() {
                format!("[PG {backend_type}] (PID {top_pid})")
            } else {
                format!("[PG] (PID {top_pid})")
            }
        } else {
            let proc_name = ctx.interner.resolve(top_name_hash).unwrap_or("?");
            format!("{proc_name} (PID {top_pid})")
        };

        let pct_of_interval = (top_delta_sec / ctx.dt) * 100.0;

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "high_blk_delay",
            category: Category::Disk,
            severity,
            title: format!(
                "I/O delay: {label} — {top_delta_sec:.1}s blocked ({pct_of_interval:.0}% of interval)"
            ),
            detail: Some(
                "Process spent significant time waiting for block I/O. \
                 Check disk utilization, await times, and whether the workload exceeds disk capacity."
                    .into(),
            ),
            value: top_delta_sec,
            merge_key: Some(top_pid.to_string()),
        }]
    }
}
