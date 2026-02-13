//! Detail popup widget for pg_stat_statements (PGS tab).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};
use crate::tui::state::{AppState, PopupState};
use crate::tui::style::Styles;

pub fn render_pgs_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => return,
    };

    let queryid = match &state.popup {
        PopupState::PgsDetail { queryid, .. } => *queryid,
        _ => return,
    };

    let Some(stmt) = find_statement(snapshot, queryid) else {
        let popup_area = centered_rect(90, 85, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(" pg_stat_statements detail ")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White).bg(Color::Black));
        frame.render_widget(
            Paragraph::new("Statement not found in current snapshot").block(block),
            popup_area,
        );
        return;
    };

    let rates = state.pgs.rates.get(&queryid);
    let prev_stmt = state.pgs.delta_base.get(&queryid);
    let content = build_content(stmt, prev_stmt, rates, interner);

    let popup_area = centered_rect(90, 85, area);
    frame.render_widget(Clear, popup_area);

    // Split popup into content + footer
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(popup_area);

    let block = Block::default()
        .title(" pg_stat_statements detail ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White).bg(Color::Black));

    // Estimate visual lines after wrapping (inner width = popup width - 2 for borders)
    let inner_width = chunks[0].width.saturating_sub(2) as usize;
    let visual_lines: usize = if inner_width > 0 {
        content
            .iter()
            .map(|line| {
                let line_width: usize = line.spans.iter().map(|s| s.content.len()).sum();
                if line_width == 0 {
                    1
                } else {
                    line_width.div_ceil(inner_width)
                }
            })
            .sum()
    } else {
        content.len()
    };
    let visible_height = chunks[0].height.saturating_sub(2) as usize; // -2 for borders
    let max_scroll = visual_lines.saturating_sub(visible_height);
    let current_scroll = if let PopupState::PgsDetail { scroll, .. } = &mut state.popup {
        if *scroll > max_scroll {
            *scroll = max_scroll;
        }
        *scroll
    } else {
        0
    };

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((current_scroll as u16, 0));
    frame.render_widget(paragraph, chunks[0]);

    let footer = Line::from(vec![
        Span::styled("↑/↓", Styles::dim()),
        Span::raw(" scroll "),
        Span::styled("PgUp/PgDn", Styles::dim()),
        Span::raw(" page "),
        Span::styled("Enter/Esc", Styles::dim()),
        Span::raw(" close "),
    ]);
    frame.render_widget(Paragraph::new(footer).style(Styles::default()), chunks[1]);
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
        lines.push(section_header("Rates (/s)"));
        lines.push(kv("dt", &format!("{:.0}s", r.dt_secs)));
        lines.push(kv("calls/s", &fmt_opt_f64(r.calls_s, 2)));
        lines.push(kv("rows/s", &fmt_opt_f64(r.rows_s, 2)));
        lines.push(kv("time_ms/s", &fmt_opt_f64(r.exec_time_ms_s, 2)));
        lines.push(kv(
            "shrd_rd/s",
            &r.shared_blks_read_s
                .map(|v| format!("{:.1} blk ({})", v, format_bytes(blocks_to_bytes(v))))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(kv(
            "shrd_hit/s",
            &r.shared_blks_hit_s
                .map(|v| format!("{:.1} blk ({})", v, format_bytes(blocks_to_bytes(v))))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(kv(
            "shrd_wr/s",
            &r.shared_blks_written_s
                .map(|v| format!("{:.1} blk ({})", v, format_bytes(blocks_to_bytes(v))))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(kv("tmp_mb/s", &fmt_opt_f64(r.temp_mb_s, 2)));
        lines.push(Line::raw(""));
    }

    // Identity section
    lines.push(section_header("Identity"));
    lines.push(kv("queryid", &stmt.queryid.to_string()));
    lines.push(kv("db", &db));
    lines.push(kv("user", &user));
    lines.push(kv_delta_i64(
        "calls",
        stmt.calls,
        prev_stmt.map(|p| p.calls),
    ));
    lines.push(kv_delta_i64("rows", stmt.rows, prev_stmt.map(|p| p.rows)));
    lines.push(kv("rows/call", &format!("{:.2}", rows_per_call)));
    lines.push(Line::raw(""));

    // Timing section
    lines.push(section_header("Timing (ms)"));
    lines.push(kv_delta_f64(
        "total_exec_time",
        stmt.total_exec_time,
        prev_stmt.map(|p| p.total_exec_time),
        3,
    ));
    lines.push(kv("mean_exec_time", &format!("{:.3}", stmt.mean_exec_time)));
    lines.push(kv("min_exec_time", &format!("{:.3}", stmt.min_exec_time)));
    lines.push(kv("max_exec_time", &format!("{:.3}", stmt.max_exec_time)));
    lines.push(kv(
        "stddev_exec_time",
        &format!("{:.3}", stmt.stddev_exec_time),
    ));
    lines.push(kv_delta_f64(
        "total_plan_time",
        stmt.total_plan_time,
        prev_stmt.map(|p| p.total_plan_time),
        3,
    ));
    lines.push(Line::raw(""));

    // I/O section
    lines.push(section_header("I/O"));
    lines.push(kv_blocks(
        "shared_blks_read",
        stmt.shared_blks_read,
        prev_stmt.map(|p| p.shared_blks_read),
    ));
    lines.push(kv_blocks(
        "shared_blks_hit",
        stmt.shared_blks_hit,
        prev_stmt.map(|p| p.shared_blks_hit),
    ));
    lines.push(kv("hit%", &format!("{:.2}", hit_pct)));
    lines.push(kv_blocks(
        "shrd_blks_dirtied",
        stmt.shared_blks_dirtied,
        prev_stmt.map(|p| p.shared_blks_dirtied),
    ));
    lines.push(kv_blocks(
        "shrd_blks_written",
        stmt.shared_blks_written,
        prev_stmt.map(|p| p.shared_blks_written),
    ));
    lines.push(kv_blocks(
        "local_blks_read",
        stmt.local_blks_read,
        prev_stmt.map(|p| p.local_blks_read),
    ));
    lines.push(kv_blocks(
        "local_blks_written",
        stmt.local_blks_written,
        prev_stmt.map(|p| p.local_blks_written),
    ));
    lines.push(Line::raw(""));

    // Temp / WAL section
    lines.push(section_header("Temp / WAL"));
    lines.push(kv_blocks(
        "temp_blks_read",
        stmt.temp_blks_read,
        prev_stmt.map(|p| p.temp_blks_read),
    ));
    lines.push(kv_blocks(
        "temp_blks_written",
        stmt.temp_blks_written,
        prev_stmt.map(|p| p.temp_blks_written),
    ));
    lines.push(kv("tmp_mb", &format!("{:.2}", tmp_mb)));
    lines.push(kv_delta_i64(
        "wal_records",
        stmt.wal_records,
        prev_stmt.map(|p| p.wal_records),
    ));
    lines.push(kv_bytes(
        "wal_bytes",
        stmt.wal_bytes,
        prev_stmt.map(|p| p.wal_bytes),
    ));
    lines.push(Line::raw(""));

    // Query section
    lines.push(section_header("Query"));
    lines.push(Line::raw(query));

    lines
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_hash(interner: Option<&StringInterner>, hash: u64) -> String {
    interner
        .and_then(|i| i.resolve(hash))
        .unwrap_or("")
        .to_string()
}

fn section_header(name: &str) -> Line<'static> {
    Line::from(Span::styled(
        name.to_string(),
        Styles::modified_item().add_modifier(Modifier::BOLD),
    ))
}

/// Style for delta values: green for positive, red for negative, dark gray for zero.
fn delta_style(delta: i64) -> Style {
    if delta > 0 {
        Style::default().fg(Color::Green)
    } else if delta < 0 {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn delta_style_f64(delta: f64) -> Style {
    if delta > 0.0005 {
        Style::default().fg(Color::Green)
    } else if delta < -0.0005 {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// Key label span (cyan, right-aligned 18 chars with colon).
fn key_span(key: &str) -> Span<'static> {
    Span::styled(format!("{key:>18}: "), Styles::cpu())
}

/// Simple key-value line (no delta).
fn kv(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![key_span(key), Span::raw(value.to_string())])
}

/// Key-value with i64 delta shown as colored span.
fn kv_delta_i64(key: &str, current: i64, prev: Option<i64>) -> Line<'static> {
    let mut spans = vec![key_span(key), Span::raw(current.to_string())];
    if let Some(p) = prev {
        let d = current - p;
        spans.push(Span::styled(format!("  {:+}", d), delta_style(d)));
    }
    Line::from(spans)
}

/// Key-value with f64 delta shown as colored span.
fn kv_delta_f64(key: &str, current: f64, prev: Option<f64>, precision: usize) -> Line<'static> {
    let mut spans = vec![
        key_span(key),
        Span::raw(format!("{:.prec$}", current, prec = precision)),
    ];
    if let Some(p) = prev {
        let d = current - p;
        spans.push(Span::styled(
            format!("  {:+.prec$}", d, prec = precision),
            delta_style_f64(d),
        ));
    }
    Line::from(spans)
}

/// PostgreSQL block size (8 KiB).
const PG_BLOCK_SIZE: u64 = 8192;

/// Convert block count to bytes (blocks x 8 KiB).
fn blocks_to_bytes(blocks: f64) -> u64 {
    (blocks * PG_BLOCK_SIZE as f64) as u64
}

/// Key-value for block counters: value in human bytes, block count dim, delta colored.
fn kv_blocks(key: &str, current: i64, prev: Option<i64>) -> Line<'static> {
    let bytes = current as u64 * PG_BLOCK_SIZE;
    let mut spans = vec![
        key_span(key),
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
    let mut spans = vec![key_span(key), Span::raw(format_bytes(current as u64))];
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

/// Format bytes to human-readable size.
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Format signed bytes to human-readable size with sign.
fn format_bytes_signed(bytes: i64) -> String {
    let sign = if bytes >= 0 { "+" } else { "-" };
    let abs = bytes.unsigned_abs();
    if abs >= 1024 * 1024 * 1024 {
        format!("{}{:.1} GiB", sign, abs as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if abs >= 1024 * 1024 {
        format!("{}{:.1} MiB", sign, abs as f64 / (1024.0 * 1024.0))
    } else if abs >= 1024 {
        format!("{}{:.1} KiB", sign, abs as f64 / 1024.0)
    } else {
        format!("{}{} B", sign, abs)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
