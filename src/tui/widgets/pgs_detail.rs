//! Detail popup widget for pg_stat_statements (PGS tab).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{
    delta_style, format_bytes, format_bytes_signed, kv, kv_delta_f64, kv_delta_i64, push_help,
    render_popup_frame, resolve_hash, section,
};

/// Help table for PGS metrics.
const HELP: &[(&str, &str)] = &[
    // Rates
    (
        "dt",
        "Time interval between current and previous pg_stat_statements snapshot used for rate calculation",
    ),
    (
        "calls/s",
        "Query executions per second during the measurement interval",
    ),
    ("rows/s", "Rows processed (returned + affected) per second"),
    (
        "time_ms/s",
        "Milliseconds of execution time consumed per second of wall-clock; >1000 means query uses >1 CPU core on average",
    ),
    (
        "shrd_rd/s",
        "Shared buffer blocks read from disk per second; high values indicate cold cache or working set > shared_buffers",
    ),
    (
        "shrd_hit/s",
        "Shared buffer blocks served from cache per second; ideally >> shrd_rd/s",
    ),
    (
        "shrd_wr/s",
        "Shared buffer blocks written to disk per second by this query (via backend writes)",
    ),
    (
        "tmp_mb/s",
        "Temporary storage (sorts, hashes) written per second in MiB; indicates work_mem overflow",
    ),
    // Identity
    (
        "queryid",
        "Internal hash of the normalized query text; stable across calls with different parameter values",
    ),
    (
        "db",
        "Database in which the query was executed (from pg_database join)",
    ),
    (
        "user",
        "PostgreSQL role that executed the query (from pg_roles join)",
    ),
    (
        "calls",
        "Total number of times this query was executed since last pg_stat_statements_reset()",
    ),
    (
        "rows",
        "Total rows retrieved (SELECT) or affected (INSERT/UPDATE/DELETE) across all executions",
    ),
    (
        "rows/call",
        "Average rows per execution: rows / calls; helps distinguish point lookups from scans",
    ),
    // Timing
    (
        "total_exec_time",
        "Cumulative wall-clock execution time (ms) across all calls; includes I/O waits, lock waits, CPU",
    ),
    (
        "mean_exec_time",
        "Average execution time per call (ms): total_exec_time / calls",
    ),
    (
        "min_exec_time",
        "Fastest single execution (ms); useful for identifying best-case performance",
    ),
    (
        "max_exec_time",
        "Slowest single execution (ms); outlier detection — check for lock contention or bloat",
    ),
    (
        "stddev_exec_time",
        "Standard deviation of execution time (ms); high stddev with normal mean suggests intermittent issues",
    ),
    (
        "total_plan_time",
        "Cumulative planning time (ms); high values suggest complex queries or stale statistics (run ANALYZE)",
    ),
    // I/O
    (
        "shared_blks_read",
        "Shared buffer blocks read from OS (disk or page cache); 1 block = 8 KiB; high reads indicate cache misses",
    ),
    (
        "shared_blks_hit",
        "Shared buffer blocks found in PostgreSQL shared_buffers (no OS read); high hit ratio is good",
    ),
    (
        "hit%",
        "Buffer cache hit ratio: shared_blks_hit / (hit + read) * 100; target >99% for OLTP workloads",
    ),
    (
        "shrd_blks_dirtied",
        "Shared blocks modified (dirtied) in cache by this query; written to disk later by checkpointer/bgwriter",
    ),
    (
        "shrd_blks_written",
        "Shared blocks written directly to disk by this backend (not via bgwriter); high values indicate shared_buffers pressure",
    ),
    (
        "local_blks_read",
        "Local buffer blocks read from disk; used by temporary tables (CREATE TEMP TABLE)",
    ),
    (
        "local_blks_written",
        "Local buffer blocks written to disk; temporary table I/O",
    ),
    // Temp / WAL
    (
        "temp_blks_read",
        "Temporary file blocks read; sorts, hash joins, or CTEs that exceeded work_mem spilled to disk",
    ),
    (
        "temp_blks_written",
        "Temporary file blocks written to disk; increase work_mem to reduce",
    ),
    (
        "tmp_mb",
        "Total temporary storage used (read + written) in MiB: (temp_blks_read + temp_blks_written) * 8 KiB",
    ),
    (
        "wal_records",
        "Total WAL (Write-Ahead Log) records generated; each INSERT/UPDATE/DELETE produces WAL",
    ),
    (
        "wal_bytes",
        "Total WAL bytes generated; high values indicate write-heavy queries impacting replication lag",
    ),
];

/// PostgreSQL block size (8 KiB).
const PG_BLOCK_SIZE: u64 = 8192;

/// Convert block count to bytes (blocks × 8 KiB).
fn blocks_to_bytes(blocks: f64) -> u64 {
    (blocks * PG_BLOCK_SIZE as f64) as u64
}

pub fn render_pgs_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let (queryid, show_help) = match &state.popup {
        PopupState::PgsDetail {
            queryid, show_help, ..
        } => (*queryid, *show_help),
        _ => return,
    };

    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => return,
    };

    let Some(stmt) = find_statement(snapshot, queryid) else {
        // Statement not found — render a minimal message using the common frame
        let content = vec![Line::raw("Statement not found in current snapshot")];
        let scroll = match &mut state.popup {
            PopupState::PgsDetail { scroll, .. } => scroll,
            _ => return,
        };
        render_popup_frame(
            frame,
            area,
            "pg_stat_statements detail",
            content,
            scroll,
            false,
        );
        return;
    };

    let rates = state.pgs.rates.get(&queryid);
    let prev_stmt = state.pgs.delta_base.get(&queryid);

    let content = build_content(stmt, prev_stmt, rates, interner, show_help);

    let scroll = match &mut state.popup {
        PopupState::PgsDetail { scroll, .. } => scroll,
        _ => return,
    };

    render_popup_frame(
        frame,
        area,
        "pg_stat_statements detail",
        content,
        scroll,
        show_help,
    );
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
    stmt: &PgStatStatementsInfo,
    prev_stmt: Option<&PgStatStatementsInfo>,
    rates: Option<&crate::tui::state::PgStatementsRates>,
    interner: Option<&StringInterner>,
    show_help: bool,
) -> Vec<Line<'static>> {
    let db = resolve_hash(interner, stmt.datname_hash);
    let user = resolve_hash(interner, stmt.usename_hash);
    let query = resolve_hash(interner, stmt.query_hash);

    let rows_per_call = if stmt.calls > 0 {
        stmt.rows as f64 / stmt.calls as f64
    } else {
        0.0
    };

    let denom = (stmt.shared_blks_hit + stmt.shared_blks_read) as f64;
    let hit_pct = if denom > 0.0 {
        (stmt.shared_blks_hit as f64 / denom) * 100.0
    } else {
        0.0
    };

    let tmp_blocks = (stmt.temp_blks_read + stmt.temp_blks_written) as f64;
    let tmp_mb = (tmp_blocks * 8.0) / 1024.0;

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
        lines.push(kv("tmp_mb/s", &fmt_opt_f64(r.temp_mb_s, 2)));
        push_help(&mut lines, show_help, HELP, "tmp_mb/s");
        lines.push(Line::raw(""));
    }

    // Identity section
    lines.push(section("Identity"));
    lines.push(kv("queryid", &stmt.queryid.to_string()));
    push_help(&mut lines, show_help, HELP, "queryid");
    lines.push(kv("db", &db));
    push_help(&mut lines, show_help, HELP, "db");
    lines.push(kv("user", &user));
    push_help(&mut lines, show_help, HELP, "user");
    lines.push(kv_delta_i64(
        "calls",
        stmt.calls,
        prev_stmt.map(|p| p.calls),
    ));
    push_help(&mut lines, show_help, HELP, "calls");
    lines.push(kv_delta_i64("rows", stmt.rows, prev_stmt.map(|p| p.rows)));
    push_help(&mut lines, show_help, HELP, "rows");
    lines.push(kv("rows/call", &format!("{:.2}", rows_per_call)));
    push_help(&mut lines, show_help, HELP, "rows/call");
    lines.push(Line::raw(""));

    // Timing section
    lines.push(section("Timing (ms)"));
    lines.push(kv_delta_f64(
        "total_exec_time",
        stmt.total_exec_time,
        prev_stmt.map(|p| p.total_exec_time),
        3,
    ));
    push_help(&mut lines, show_help, HELP, "total_exec_time");
    lines.push(kv("mean_exec_time", &format!("{:.3}", stmt.mean_exec_time)));
    push_help(&mut lines, show_help, HELP, "mean_exec_time");
    lines.push(kv("min_exec_time", &format!("{:.3}", stmt.min_exec_time)));
    push_help(&mut lines, show_help, HELP, "min_exec_time");
    lines.push(kv("max_exec_time", &format!("{:.3}", stmt.max_exec_time)));
    push_help(&mut lines, show_help, HELP, "max_exec_time");
    lines.push(kv(
        "stddev_exec_time",
        &format!("{:.3}", stmt.stddev_exec_time),
    ));
    push_help(&mut lines, show_help, HELP, "stddev_exec_time");
    lines.push(kv_delta_f64(
        "total_plan_time",
        stmt.total_plan_time,
        prev_stmt.map(|p| p.total_plan_time),
        3,
    ));
    push_help(&mut lines, show_help, HELP, "total_plan_time");
    lines.push(Line::raw(""));

    // I/O section
    lines.push(section("I/O"));
    lines.push(kv_blocks(
        "shared_blks_read",
        stmt.shared_blks_read,
        prev_stmt.map(|p| p.shared_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "shared_blks_read");
    lines.push(kv_blocks(
        "shared_blks_hit",
        stmt.shared_blks_hit,
        prev_stmt.map(|p| p.shared_blks_hit),
    ));
    push_help(&mut lines, show_help, HELP, "shared_blks_hit");
    lines.push(kv("hit%", &format!("{:.2}", hit_pct)));
    push_help(&mut lines, show_help, HELP, "hit%");
    lines.push(kv_blocks(
        "shrd_blks_dirtied",
        stmt.shared_blks_dirtied,
        prev_stmt.map(|p| p.shared_blks_dirtied),
    ));
    push_help(&mut lines, show_help, HELP, "shrd_blks_dirtied");
    lines.push(kv_blocks(
        "shrd_blks_written",
        stmt.shared_blks_written,
        prev_stmt.map(|p| p.shared_blks_written),
    ));
    push_help(&mut lines, show_help, HELP, "shrd_blks_written");
    lines.push(kv_blocks(
        "local_blks_read",
        stmt.local_blks_read,
        prev_stmt.map(|p| p.local_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "local_blks_read");
    lines.push(kv_blocks(
        "local_blks_written",
        stmt.local_blks_written,
        prev_stmt.map(|p| p.local_blks_written),
    ));
    push_help(&mut lines, show_help, HELP, "local_blks_written");
    lines.push(Line::raw(""));

    // Temp / WAL section
    lines.push(section("Temp / WAL"));
    lines.push(kv_blocks(
        "temp_blks_read",
        stmt.temp_blks_read,
        prev_stmt.map(|p| p.temp_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "temp_blks_read");
    lines.push(kv_blocks(
        "temp_blks_written",
        stmt.temp_blks_written,
        prev_stmt.map(|p| p.temp_blks_written),
    ));
    push_help(&mut lines, show_help, HELP, "temp_blks_written");
    lines.push(kv("tmp_mb", &format!("{:.2}", tmp_mb)));
    push_help(&mut lines, show_help, HELP, "tmp_mb");
    lines.push(kv_delta_i64(
        "wal_records",
        stmt.wal_records,
        prev_stmt.map(|p| p.wal_records),
    ));
    push_help(&mut lines, show_help, HELP, "wal_records");
    lines.push(kv_bytes(
        "wal_bytes",
        stmt.wal_bytes,
        prev_stmt.map(|p| p.wal_bytes),
    ));
    push_help(&mut lines, show_help, HELP, "wal_bytes");
    lines.push(Line::raw(""));

    // Query section
    lines.push(section("Query"));
    lines.push(Line::raw(query));

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

/// Key-value for byte counters: value in human bytes, delta colored.
fn kv_bytes(key: &str, current: i64, prev: Option<i64>) -> Line<'static> {
    let key_span = Span::styled(format!("{:>20}: ", key), crate::tui::style::Styles::cpu());
    let mut spans = vec![key_span, Span::raw(format_bytes(current as u64))];
    if let Some(p) = prev {
        let d = current - p;
        spans.push(Span::styled(
            format!("  {}", format_bytes_signed(d)),
            delta_style(d),
        ));
    }
    Line::from(spans)
}

fn fmt_opt_f64(v: Option<f64>, precision: usize) -> String {
    v.map(|v| format!("{:.prec$}", v, prec = precision))
        .unwrap_or_else(|| "--".to_string())
}
