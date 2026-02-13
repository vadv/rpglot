//! Detail popup widget for pg_stat_statements (PGS tab).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};
use crate::tui::state::AppState;
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

    let Some(queryid) = state.pgs_detail_queryid else {
        return;
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

    let rates = state.pgs_rates.get(&queryid);
    let content = build_content(stmt, rates, interner);

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
    if state.pgs_detail_scroll > max_scroll {
        state.pgs_detail_scroll = max_scroll;
    }

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((state.pgs_detail_scroll as u16, 0));
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

    if let Some(r) = rates {
        lines.push(section_header("Rates (/s)"));
        lines.push(key_value("dt", &format!("{:.0}s", r.dt_secs)));
        lines.push(key_value(
            "calls/s",
            &r.calls_s
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(key_value(
            "rows/s",
            &r.rows_s
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(key_value(
            "time_ms/s",
            &r.exec_time_ms_s
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(key_value(
            "shrd_rd/s",
            &r.shared_blks_read_s
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(key_value(
            "shrd_wr/s",
            &r.shared_blks_written_s
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(key_value(
            "tmp_mb/s",
            &r.temp_mb_s
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "--".to_string()),
        ));
        lines.push(Line::raw(""));
    }

    lines.push(section_header("Identity"));
    lines.push(key_value("queryid", &stmt.queryid.to_string()));
    lines.push(key_value("db", &db));
    lines.push(key_value("user", &user));
    lines.push(key_value("calls", &stmt.calls.to_string()));
    lines.push(key_value("rows", &stmt.rows.to_string()));
    lines.push(key_value("rows/call", &format!("{:.2}", rows_per_call)));
    lines.push(Line::raw(""));

    lines.push(section_header("Timing (ms)"));
    lines.push(key_value(
        "total_exec_time",
        &format!("{:.3}", stmt.total_exec_time),
    ));
    lines.push(key_value(
        "mean_exec_time",
        &format!("{:.3}", stmt.mean_exec_time),
    ));
    lines.push(key_value(
        "min_exec_time",
        &format!("{:.3}", stmt.min_exec_time),
    ));
    lines.push(key_value(
        "max_exec_time",
        &format!("{:.3}", stmt.max_exec_time),
    ));
    lines.push(key_value(
        "stddev_exec_time",
        &format!("{:.3}", stmt.stddev_exec_time),
    ));
    lines.push(key_value(
        "total_plan_time",
        &format!("{:.3}", stmt.total_plan_time),
    ));
    lines.push(Line::raw(""));

    lines.push(section_header("I/O (blocks)"));
    lines.push(key_value(
        "shared_blks_read",
        &stmt.shared_blks_read.to_string(),
    ));
    lines.push(key_value(
        "shared_blks_hit",
        &stmt.shared_blks_hit.to_string(),
    ));
    lines.push(key_value("hit%", &format!("{:.2}", hit_pct)));
    lines.push(key_value(
        "shared_blks_dirtied",
        &stmt.shared_blks_dirtied.to_string(),
    ));
    lines.push(key_value(
        "shared_blks_written",
        &stmt.shared_blks_written.to_string(),
    ));
    lines.push(key_value(
        "local_blks_read",
        &stmt.local_blks_read.to_string(),
    ));
    lines.push(key_value(
        "local_blks_written",
        &stmt.local_blks_written.to_string(),
    ));
    lines.push(Line::raw(""));

    lines.push(section_header("Temp / WAL"));
    lines.push(key_value(
        "temp_blks_read",
        &stmt.temp_blks_read.to_string(),
    ));
    lines.push(key_value(
        "temp_blks_written",
        &stmt.temp_blks_written.to_string(),
    ));
    lines.push(key_value("tmp_mb", &format!("{:.2}", tmp_mb)));
    lines.push(key_value("wal_records", &stmt.wal_records.to_string()));
    lines.push(key_value("wal_bytes", &stmt.wal_bytes.to_string()));
    lines.push(Line::raw(""));

    lines.push(section_header("Query"));
    lines.push(Line::raw(query));

    lines
}

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

fn key_value(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:>16}: "), Styles::cpu()),
        Span::raw(value.to_string()),
    ])
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
