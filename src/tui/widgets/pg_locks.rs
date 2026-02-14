//! PostgreSQL lock tree (PGL) tab widget.
//! Shows blocking chains as a flat table with depth-based indentation.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgLockTreeNode};
use crate::tui::state::AppState;
use crate::tui::style::Styles;

use crate::tui::fmt::{format_epoch_age, normalize_query};

/// Shortens PostgreSQL lock mode names for table display.
fn short_lock_mode(mode: &str) -> &str {
    match mode {
        "AccessShareLock" => "AccShr",
        "RowShareLock" => "RowShr",
        "RowExclusiveLock" => "RowExcl",
        "ShareUpdateExclusiveLock" => "ShrUpdExcl",
        "ShareLock" => "Share",
        "ShareRowExclusiveLock" => "ShrRowExcl",
        "ExclusiveLock" => "Excl",
        "AccessExclusiveLock" => "AccExcl",
        "" => "-",
        other => other,
    }
}

/// Formats PID with depth-based dot indentation.
fn format_pid_with_depth(pid: i32, depth: i32) -> String {
    if depth <= 1 {
        format!("{}", pid)
    } else {
        let dots: String = ".".repeat((depth - 1) as usize);
        format!("{}{}", dots, pid)
    }
}

struct LockRow {
    pid: i32,
    depth: i32,
    pid_display: String,
    state: String,
    wait: String,
    duration: String,
    lock_mode: String,
    target: String,
    query: String,
    lock_granted: bool,
}

pub fn render_pg_locks(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let msg = Paragraph::new("No data available").block(
                Block::default()
                    .title("PGL: Lock Tree")
                    .borders(Borders::ALL),
            );
            frame.render_widget(Clear, area);
            frame.render_widget(msg, area);
            return;
        }
    };

    // Find PgLockTree data
    let nodes: &[PgLockTreeNode] = snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgLockTree(v) = b {
                Some(v.as_slice())
            } else {
                None
            }
        })
        .unwrap_or(&[]);

    if nodes.is_empty() {
        let msg = Paragraph::new("No blocking chains detected").block(
            Block::default()
                .title("PGL: Lock Tree")
                .borders(Borders::ALL),
        );
        frame.render_widget(Clear, area);
        frame.render_widget(msg, area);
        return;
    }

    // Build row data
    let resolve = |hash: u64| -> String {
        if hash == 0 {
            return "-".to_string();
        }
        interner
            .and_then(|i| i.resolve(hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("#{:x}", hash))
    };

    let mut rows_data: Vec<LockRow> = nodes
        .iter()
        .map(|n| {
            let state_str = resolve(n.state_hash);
            let wet = resolve(n.wait_event_type_hash);
            let we = resolve(n.wait_event_hash);
            let wait = if wet == "-" && we == "-" {
                "-".to_string()
            } else {
                format!("{}:{}", wet, we)
            };

            // Duration: for root blockers use xact_start, for waiters use state_change
            let duration = if n.depth <= 1 {
                format_epoch_age(n.xact_start)
            } else {
                format_epoch_age(n.state_change)
            };

            let lock_mode_full = resolve(n.lock_mode_hash);
            let lock_mode = short_lock_mode(&lock_mode_full).to_string();

            let lock_target = resolve(n.lock_target_hash);
            let lock_type = resolve(n.lock_type_hash);
            let target = if lock_target == "-" || lock_target.is_empty() {
                lock_type
            } else {
                lock_target
            };

            let query_raw = resolve(n.query_hash);
            let query = normalize_query(&query_raw);

            LockRow {
                pid: n.pid,
                depth: n.depth,
                pid_display: format_pid_with_depth(n.pid, n.depth),
                state: state_str,
                wait,
                duration,
                lock_mode,
                target,
                query,
                lock_granted: n.lock_granted,
            }
        })
        .collect();

    // Apply filter
    if let Some(ref filter) = state.pgl.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.query.to_lowercase().contains(&f)
                || r.target.to_lowercase().contains(&f)
                || r.pid.to_string().contains(&f)
                || r.state.to_lowercase().contains(&f)
        });
    }

    if rows_data.is_empty() {
        let msg = Paragraph::new("No matching rows (filter active)").block(
            Block::default()
                .title("PGL: Lock Tree")
                .borders(Borders::ALL),
        );
        frame.render_widget(Clear, area);
        frame.render_widget(msg, area);
        return;
    }

    // Resolve selection
    let row_pids: Vec<i32> = rows_data.iter().map(|r| r.pid).collect();
    state.pgl.resolve_selection(&row_pids);

    // Headers
    let headers = [
        "PID",
        "STATE",
        "WAIT",
        "DURATION",
        "LOCK_MODE",
        "TARGET",
        "QUERY",
    ];
    let header_cells: Vec<Span> = headers.iter().map(|h| Span::raw(*h)).collect();
    let header = Row::new(header_cells).style(Styles::table_header());

    // Widths
    let widths = [
        ratatui::layout::Constraint::Min(10),
        ratatui::layout::Constraint::Min(12),
        ratatui::layout::Constraint::Min(14),
        ratatui::layout::Constraint::Min(10),
        ratatui::layout::Constraint::Min(12),
        ratatui::layout::Constraint::Min(18),
        ratatui::layout::Constraint::Percentage(100),
    ];

    // Build rows
    let rows: Vec<Row> = rows_data
        .iter()
        .map(|r| {
            let style = if r.depth <= 1 {
                // Root blocker — red
                Styles::critical()
            } else if !r.lock_granted {
                // Waiting — yellow
                Styles::modified_item()
            } else {
                Style::default()
            };

            Row::new(vec![
                Span::raw(r.pid_display.clone()),
                Span::raw(r.state.clone()),
                Span::raw(r.wait.clone()),
                Span::raw(r.duration.clone()),
                Span::raw(r.lock_mode.clone()),
                Span::raw(r.target.clone()),
                Span::raw(r.query.clone()),
            ])
            .style(style)
        })
        .collect();

    let filter_info = state
        .pgl
        .filter
        .as_ref()
        .map(|f| format!(" [filter: {}]", f))
        .unwrap_or_default();

    let title = format!("PGL: Lock Tree ({} rows){}", rows_data.len(), filter_info);

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(Styles::default()),
        )
        .column_spacing(1)
        .row_highlight_style(Styles::selected());

    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pgl.ratatui_state);
}
