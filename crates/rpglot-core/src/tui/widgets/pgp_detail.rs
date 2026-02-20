//! Detail popup widget for pg_store_plans (PGP tab).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::models::PgStorePlansRates;
use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatStatementsInfo, PgStorePlansInfo, Snapshot};
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{
    delta_style, format_bytes, format_bytes_signed, format_epoch_age, kv, kv_delta_f64,
    kv_delta_i64, push_help, render_popup_frame, resolve_hash, section,
};

/// Help table for PGP metrics.
const HELP: &[(&str, &str)] = &[
    // Rates
    (
        "dt",
        "Time interval between current and previous pg_store_plans snapshot used for rate calculation",
    ),
    (
        "calls/s",
        "Plan executions per second during the measurement interval",
    ),
    ("rows/s", "Rows processed per second by this plan"),
    (
        "time_ms/s",
        "Milliseconds of execution time consumed per second of wall-clock; >1000 means >1 CPU core on average",
    ),
    (
        "shrd_rd/s",
        "Shared buffer blocks read from disk per second; high values indicate cache misses",
    ),
    (
        "shrd_hit/s",
        "Shared buffer blocks served from cache per second; ideally >> shrd_rd/s",
    ),
    (
        "shrd_wr/s",
        "Shared buffer blocks written to disk per second by this plan (via backend writes)",
    ),
    // Identity
    (
        "planid",
        "Internal hash code identifying the execution plan; stable for same plan text",
    ),
    (
        "stmt_queryid",
        "queryid from pg_stat_statements linking this plan to its parent query",
    ),
    (
        "db",
        "Database in which the plan was executed (from pg_database join)",
    ),
    (
        "user",
        "PostgreSQL role that executed the plan (from pg_roles join)",
    ),
    (
        "calls",
        "Total number of times this plan was executed since last reset",
    ),
    (
        "rows",
        "Total rows retrieved or affected across all executions of this plan",
    ),
    ("rows/call", "Average rows per execution: rows / calls"),
    // Timing
    (
        "total_time",
        "Cumulative execution time (ms) across all calls of this plan",
    ),
    (
        "mean_time",
        "Average execution time per call (ms): total_time / calls",
    ),
    ("min_time", "Fastest single execution of this plan (ms)"),
    (
        "max_time",
        "Slowest single execution of this plan (ms); outlier detection",
    ),
    (
        "stddev_time",
        "Standard deviation of execution time (ms); high stddev suggests intermittent issues",
    ),
    // I/O
    (
        "shared_blks_read",
        "Shared buffer blocks read from OS; 1 block = 8 KiB; high reads indicate cache misses",
    ),
    (
        "shared_blks_hit",
        "Shared buffer blocks found in shared_buffers (no OS read); high hit ratio is good",
    ),
    (
        "hit%",
        "Buffer cache hit ratio: shared_blks_hit / (hit + read) * 100; target >99% for OLTP",
    ),
    (
        "shrd_blks_dirtied",
        "Shared blocks modified in cache by this plan; written to disk later by checkpointer",
    ),
    (
        "shrd_blks_written",
        "Shared blocks written directly to disk by this backend; high values indicate shared_buffers pressure",
    ),
    (
        "local_blks_read",
        "Local buffer blocks read from disk; used by temporary tables",
    ),
    (
        "local_blks_written",
        "Local buffer blocks written to disk; temporary table I/O",
    ),
    // Temp
    (
        "temp_blks_read",
        "Temporary file blocks read; sorts, hash joins that exceeded work_mem",
    ),
    (
        "temp_blks_written",
        "Temporary file blocks written to disk; increase work_mem to reduce",
    ),
    // IO timing
    (
        "blk_read_time",
        "Total block read time (ms); requires track_io_timing = on",
    ),
    (
        "blk_write_time",
        "Total block write time (ms); requires track_io_timing = on",
    ),
    // Timestamps
    ("first_call", "Time since the first execution of this plan"),
    ("last_call", "Time since the last execution of this plan"),
];

/// PostgreSQL block size (8 KiB).
const PG_BLOCK_SIZE: u64 = 8192;

/// Convert block count to bytes (blocks * 8 KiB).
fn blocks_to_bytes(blocks: f64) -> u64 {
    (blocks * PG_BLOCK_SIZE as f64) as u64
}

pub fn render_pgp_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let (planid, show_help) = match &state.popup {
        PopupState::PgpDetail {
            planid, show_help, ..
        } => (*planid, *show_help),
        _ => return,
    };

    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => return,
    };

    let Some(plan) = find_plan(snapshot, planid) else {
        let content = vec![Line::raw("Plan not found in current snapshot")];
        let scroll = match &mut state.popup {
            PopupState::PgpDetail { scroll, .. } => scroll,
            _ => return,
        };
        render_popup_frame(frame, area, "pg_store_plans detail", content, scroll, false);
        return;
    };

    let rates = state.pgp.rates.get(&planid);
    let prev_plan = state.pgp.delta_base.get(&planid);

    // Find the parent query from pg_stat_statements for cross-reference
    let parent_query = find_statement(snapshot, plan.stmt_queryid);

    let content = build_content(plan, prev_plan, rates, parent_query, interner, show_help);

    let scroll = match &mut state.popup {
        PopupState::PgpDetail { scroll, .. } => scroll,
        _ => return,
    };

    render_popup_frame(
        frame,
        area,
        "pg_store_plans detail",
        content,
        scroll,
        show_help,
    );
}

fn find_plan(snapshot: &Snapshot, planid: i64) -> Option<&PgStorePlansInfo> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStorePlans(v) = b {
            v.iter().find(|p| p.planid == planid)
        } else {
            None
        }
    })
}

fn find_statement(snapshot: &Snapshot, queryid: i64) -> Option<&PgStatStatementsInfo> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatStatements(v) = b {
            v.iter().find(|s| s.queryid == queryid)
        } else {
            None
        }
    })
}

fn build_content(
    plan: &PgStorePlansInfo,
    prev_plan: Option<&PgStorePlansInfo>,
    rates: Option<&PgStorePlansRates>,
    parent_query: Option<&PgStatStatementsInfo>,
    interner: Option<&StringInterner>,
    show_help: bool,
) -> Vec<Line<'static>> {
    let db = resolve_hash(interner, plan.datname_hash);
    let user = resolve_hash(interner, plan.usename_hash);
    let plan_text = resolve_hash(interner, plan.plan_hash);

    let rows_per_call = if plan.calls > 0 {
        plan.rows as f64 / plan.calls as f64
    } else {
        0.0
    };

    let denom = (plan.shared_blks_hit + plan.shared_blks_read) as f64;
    let hit_pct = if denom > 0.0 {
        (plan.shared_blks_hit as f64 / denom) * 100.0
    } else {
        0.0
    };

    let mut lines = Vec::new();

    // Rates section
    if let Some(r) = rates {
        lines.push(section("Rates (/s)"));
        lines.push(kv("dt", &format!("{:.0}s", r.dt_secs)));
        push_help(&mut lines, show_help, HELP, "dt");
        lines.push(kv("calls/s", &fmt_opt_f64(r.calls_s, 2)));
        push_help(&mut lines, show_help, HELP, "calls/s");
        lines.push(kv("rows/s", &fmt_opt_f64(r.rows_s, 2)));
        push_help(&mut lines, show_help, HELP, "rows/s");
        lines.push(kv("time_ms/s", &fmt_opt_f64(r.exec_time_ms_s, 2)));
        push_help(&mut lines, show_help, HELP, "time_ms/s");
        lines.push(kv(
            "shrd_rd/s",
            &r.shared_blks_read_s
                .map(|v| format!("{:.1} blk ({})", v, format_bytes(blocks_to_bytes(v))))
                .unwrap_or_else(|| "--".to_string()),
        ));
        push_help(&mut lines, show_help, HELP, "shrd_rd/s");
        lines.push(kv(
            "shrd_hit/s",
            &r.shared_blks_hit_s
                .map(|v| format!("{:.1} blk ({})", v, format_bytes(blocks_to_bytes(v))))
                .unwrap_or_else(|| "--".to_string()),
        ));
        push_help(&mut lines, show_help, HELP, "shrd_hit/s");
        lines.push(kv(
            "shrd_wr/s",
            &r.shared_blks_written_s
                .map(|v| format!("{:.1} blk ({})", v, format_bytes(blocks_to_bytes(v))))
                .unwrap_or_else(|| "--".to_string()),
        ));
        push_help(&mut lines, show_help, HELP, "shrd_wr/s");
        lines.push(Line::raw(""));
    }

    // Identity section
    lines.push(section("Identity"));
    lines.push(kv("planid", &plan.planid.to_string()));
    push_help(&mut lines, show_help, HELP, "planid");
    lines.push(kv("stmt_queryid", &plan.stmt_queryid.to_string()));
    push_help(&mut lines, show_help, HELP, "stmt_queryid");
    lines.push(kv("db", &db));
    push_help(&mut lines, show_help, HELP, "db");
    lines.push(kv("user", &user));
    push_help(&mut lines, show_help, HELP, "user");
    lines.push(kv_delta_i64(
        "calls",
        plan.calls,
        prev_plan.map(|p| p.calls),
    ));
    push_help(&mut lines, show_help, HELP, "calls");
    lines.push(kv_delta_i64("rows", plan.rows, prev_plan.map(|p| p.rows)));
    push_help(&mut lines, show_help, HELP, "rows");
    lines.push(kv("rows/call", &format!("{:.2}", rows_per_call)));
    push_help(&mut lines, show_help, HELP, "rows/call");
    lines.push(Line::raw(""));

    // Timing section
    lines.push(section("Timing (ms)"));
    lines.push(kv_delta_f64(
        "total_time",
        plan.total_time,
        prev_plan.map(|p| p.total_time),
        3,
    ));
    push_help(&mut lines, show_help, HELP, "total_time");
    lines.push(kv("mean_time", &format!("{:.3}", plan.mean_time)));
    push_help(&mut lines, show_help, HELP, "mean_time");
    lines.push(kv("min_time", &format!("{:.3}", plan.min_time)));
    push_help(&mut lines, show_help, HELP, "min_time");
    lines.push(kv("max_time", &format!("{:.3}", plan.max_time)));
    push_help(&mut lines, show_help, HELP, "max_time");
    lines.push(kv("stddev_time", &format!("{:.3}", plan.stddev_time)));
    push_help(&mut lines, show_help, HELP, "stddev_time");
    lines.push(Line::raw(""));

    // I/O section
    lines.push(section("I/O"));
    lines.push(kv_blocks(
        "shared_blks_read",
        plan.shared_blks_read,
        prev_plan.map(|p| p.shared_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "shared_blks_read");
    lines.push(kv_blocks(
        "shared_blks_hit",
        plan.shared_blks_hit,
        prev_plan.map(|p| p.shared_blks_hit),
    ));
    push_help(&mut lines, show_help, HELP, "shared_blks_hit");
    lines.push(kv("hit%", &format!("{:.2}", hit_pct)));
    push_help(&mut lines, show_help, HELP, "hit%");
    lines.push(kv_blocks(
        "shrd_blks_dirtied",
        plan.shared_blks_dirtied,
        prev_plan.map(|p| p.shared_blks_dirtied),
    ));
    push_help(&mut lines, show_help, HELP, "shrd_blks_dirtied");
    lines.push(kv_blocks(
        "shrd_blks_written",
        plan.shared_blks_written,
        prev_plan.map(|p| p.shared_blks_written),
    ));
    push_help(&mut lines, show_help, HELP, "shrd_blks_written");
    lines.push(kv_blocks(
        "local_blks_read",
        plan.local_blks_read,
        prev_plan.map(|p| p.local_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "local_blks_read");
    lines.push(kv_blocks(
        "local_blks_written",
        plan.local_blks_written,
        prev_plan.map(|p| p.local_blks_written),
    ));
    push_help(&mut lines, show_help, HELP, "local_blks_written");
    lines.push(Line::raw(""));

    // Temp / IO timing section
    lines.push(section("Temp / IO Timing"));
    lines.push(kv_blocks(
        "temp_blks_read",
        plan.temp_blks_read,
        prev_plan.map(|p| p.temp_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "temp_blks_read");
    lines.push(kv_blocks(
        "temp_blks_written",
        plan.temp_blks_written,
        prev_plan.map(|p| p.temp_blks_written),
    ));
    push_help(&mut lines, show_help, HELP, "temp_blks_written");
    lines.push(kv_delta_f64(
        "blk_read_time",
        plan.blk_read_time,
        prev_plan.map(|p| p.blk_read_time),
        3,
    ));
    push_help(&mut lines, show_help, HELP, "blk_read_time");
    lines.push(kv_delta_f64(
        "blk_write_time",
        plan.blk_write_time,
        prev_plan.map(|p| p.blk_write_time),
        3,
    ));
    push_help(&mut lines, show_help, HELP, "blk_write_time");
    lines.push(Line::raw(""));

    // Timestamps section
    lines.push(section("Timestamps"));
    lines.push(kv("first_call", &format_epoch_age(plan.first_call as i64)));
    push_help(&mut lines, show_help, HELP, "first_call");
    lines.push(kv("last_call", &format_epoch_age(plan.last_call as i64)));
    push_help(&mut lines, show_help, HELP, "last_call");
    lines.push(Line::raw(""));

    // Query section (from pg_stat_statements cross-reference)
    if let Some(stmt) = parent_query {
        let query_text = resolve_hash(interner, stmt.query_hash);
        lines.push(section("Query (from pg_stat_statements)"));
        for line in query_text.lines() {
            let sanitized = line.replace('\t', "    ");
            lines.push(Line::raw(format!("  {}", sanitized)));
        }
        lines.push(Line::raw(""));
    }

    // Plan section
    lines.push(section("Plan"));
    for line in plan_text.lines() {
        let sanitized = line.replace('\t', "    ");
        lines.push(Line::raw(format!("  {}", sanitized)));
    }

    lines
}

// ---------------------------------------------------------------------------
// Local helpers (domain-specific, not shared)
// ---------------------------------------------------------------------------

/// Key-value for block counters: value in human bytes, block count dim, delta colored.
fn kv_blocks(key: &str, current: i64, prev: Option<i64>) -> Line<'static> {
    let bytes = current as u64 * PG_BLOCK_SIZE;
    let key_span = Span::styled(format!("{:>20}: ", key), crate::tui::style::Styles::cpu());
    let mut spans = vec![
        key_span,
        Span::raw(format_bytes(bytes)),
        Span::styled(
            format!("  ({} blk)", current),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    if let Some(p) = prev {
        let d = current - p;
        let d_bytes = d * PG_BLOCK_SIZE as i64;
        spans.push(Span::styled(
            format!("  {:+} blk / {}", d, format_bytes_signed(d_bytes)),
            delta_style(d),
        ));
    }
    Line::from(spans)
}

fn fmt_opt_f64(v: Option<f64>, precision: usize) -> String {
    v.map(|v| format!("{:.prec$}", v, prec = precision))
        .unwrap_or_else(|| "--".to_string())
}
