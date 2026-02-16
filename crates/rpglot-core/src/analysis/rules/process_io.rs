use std::collections::HashMap;

use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

/// The top process must do at least 10 MB/s to be considered a hog.
const MIN_HOG_IO_BYTES_S: f64 = 10_000_000.0;

/// Fraction of total I/O a single process must exceed to be flagged.
const WARNING_FRACTION: f64 = 0.60;
const CRITICAL_FRACTION: f64 = 0.90;

// ============================================================
// ProcessIoHogRule
// ============================================================

pub struct ProcessIoHogRule;

impl AnalysisRule for ProcessIoHogRule {
    fn id(&self) -> &'static str {
        "process_io_hog"
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

        // Build PID → index map for previous processes
        let prev_by_pid: HashMap<u32, usize> = prev_procs
            .iter()
            .enumerate()
            .map(|(i, p)| (p.pid, i))
            .collect();

        // Compute per-process I/O rate
        struct ProcIo {
            pid: u32,
            name_hash: u64,
            cmdline_hash: u64,
            read_bytes_s: f64,
            write_bytes_s: f64,
            total_bytes_s: f64,
        }

        let mut rates: Vec<ProcIo> = Vec::new();
        let mut total_io: f64 = 0.0;

        for p in procs {
            let Some(&prev_idx) = prev_by_pid.get(&p.pid) else {
                continue;
            };
            let prev = &prev_procs[prev_idx];

            let delta_read = p.dsk.rsz.saturating_sub(prev.dsk.rsz) as f64;
            let delta_write = p.dsk.wsz.saturating_sub(prev.dsk.wsz) as f64;
            let read_s = delta_read / ctx.dt;
            let write_s = delta_write / ctx.dt;
            let total_s = read_s + write_s;

            if total_s > 0.0 {
                total_io += total_s;
                rates.push(ProcIo {
                    pid: p.pid,
                    name_hash: p.name_hash,
                    cmdline_hash: p.cmdline_hash,
                    read_bytes_s: read_s,
                    write_bytes_s: write_s,
                    total_bytes_s: total_s,
                });
            }
        }

        // Find top I/O consumer
        let Some(top) = rates.iter().max_by(|a, b| {
            a.total_bytes_s
                .partial_cmp(&b.total_bytes_s)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) else {
            return Vec::new();
        };

        if top.total_bytes_s < MIN_HOG_IO_BYTES_S {
            return Vec::new();
        }

        let fraction = top.total_bytes_s / total_io;
        if fraction < WARNING_FRACTION {
            return Vec::new();
        }

        let severity = if fraction >= CRITICAL_FRACTION {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let pct = (fraction * 100.0) as u32;
        let mb_s = top.total_bytes_s / 1_048_576.0;
        let read_mb_s = top.read_bytes_s / 1_048_576.0;
        let write_mb_s = top.write_bytes_s / 1_048_576.0;
        let total_mb_s = total_io / 1_048_576.0;

        // Enrich with pg_stat_activity query if PID is a PG backend
        let pg_session = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatActivity(v) => Some(v.as_slice()),
            _ => None,
        })
        .and_then(|sessions| sessions.iter().find(|s| s.pid == top.pid as i32));

        let label = if let Some(session) = pg_session {
            let query: String = ctx
                .interner
                .resolve(session.query_hash)
                .unwrap_or("")
                .chars()
                .take(80)
                .collect();
            if query.is_empty() {
                format!("PID {}", top.pid)
            } else {
                format!("[PG] {} (PID {})", query, top.pid)
            }
        } else {
            let proc_name = ctx.interner.resolve(top.name_hash).unwrap_or("?");
            format!("{proc_name} (PID {})", top.pid)
        };

        let title = format!("I/O hog: {label} — {mb_s:.1} MB/s ({pct}% of total)");

        let mut detail_str = format!(
            "Read: {read_mb_s:.1} MB/s, Write: {write_mb_s:.1} MB/s. Total system I/O: {total_mb_s:.1} MB/s",
        );
        if let Some(cmdline) = ctx.interner.resolve(top.cmdline_hash) {
            let truncated: String = cmdline.chars().take(120).collect();
            detail_str.push_str(&format!("\nCmd: {truncated}"));
        }

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "process_io_hog",
            category: Category::Disk,
            severity,
            title,
            detail: Some(detail_str),
            value: top.total_bytes_s,
        }]
    }
}
