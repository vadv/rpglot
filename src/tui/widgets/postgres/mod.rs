//! PostgreSQL activity table widget for PGA tab.
//! Displays pg_stat_activity data with CPU/RSS from OS process info.
//! Stats view mode enriches data with pg_stat_statements metrics (linked by query_id).

mod row_data;
mod styling;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::tui::state::{AppState, PgActivityViewMode};
use crate::tui::style::Styles;

use row_data::{
    PgActivityRowData, extract_pg_activities, extract_pg_statements_map, extract_processes,
};
use styling::*;

/// Column headers for PGA table (Generic view).
const PGA_HEADERS_GENERIC: &[&str] = &[
    "PID", "CPU%", "RSS", "DB", "USER", "STATE", "WAIT", "QDUR", "XDUR", "BDUR", "BTYPE", "QUERY",
];

/// Column headers for PGA table (Stats view).
/// Shows pg_stat_statements metrics: MEAN, MAX, CALL/s, HIT%
const PGA_HEADERS_STATS: &[&str] = &[
    "PID", "DB", "USER", "STATE", "QDUR", "MEAN", "MAX", "CALL/s", "HIT%", "QUERY",
];

/// Default column widths for Generic view (QUERY uses Fill constraint, so not included here).
/// DB and USER widths accommodate names like "product-service-data-shard-120"
const PGA_WIDTHS_GENERIC: &[u16] = &[7, 6, 8, 32, 32, 16, 20, 8, 8, 8, 14];

/// Column widths for Stats view.
const PGA_WIDTHS_STATS: &[u16] = &[7, 32, 32, 16, 8, 8, 8, 8, 6];

/// Renders the PostgreSQL activity table.
pub fn render_postgres(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Activity (PGA) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            let paragraph = Paragraph::new("No data available").block(block);
            frame.render_widget(paragraph, area);
            return;
        }
    };

    // Extract PgStatActivity data
    let pg_activities = extract_pg_activities(snapshot);

    if pg_activities.is_empty() {
        let block = Block::default()
            .title(" PostgreSQL Activity (PGA) ")
            .borders(Borders::ALL)
            .style(Styles::default());
        let message = state
            .pga
            .last_error
            .as_deref()
            .unwrap_or("No active PostgreSQL sessions");
        let paragraph = Paragraph::new(message).block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    // Get process info for CPU/RSS lookup (by PID)
    let processes = extract_processes(snapshot);
    let process_map: std::collections::HashMap<u32, &crate::storage::model::ProcessInfo> =
        processes.iter().map(|p| (p.pid, *p)).collect();

    // Current timestamp for duration calculation
    let now = snapshot.timestamp;

    // Build rows with sorting (idle at bottom, then by QDUR desc)
    let mut rows_data: Vec<PgActivityRowData> = pg_activities
        .iter()
        .map(|pg| {
            // Convert i32 pid to u32 for lookup (should always be positive)
            let process = if pg.pid > 0 {
                process_map.get(&(pg.pid as u32))
            } else {
                None
            };
            PgActivityRowData::from_pg_activity(pg, process, now, interner)
        })
        .collect();

    // Apply filter if set (matches PID, query_id, DB, USER, QUERY)
    if let Some(filter) = &state.pga.filter {
        let filter_lower = filter.to_lowercase();
        rows_data.retain(|row| {
            // Check PID (exact prefix match for numbers)
            row.pid.to_string().starts_with(&filter_lower)
                // Check query_id (exact prefix match for numbers)
                || (row.query_id != 0 && row.query_id.to_string().starts_with(&filter_lower))
                // Check text fields (substring match)
                || row.db.to_lowercase().contains(&filter_lower)
                || row.user.to_lowercase().contains(&filter_lower)
                || row.query.to_lowercase().contains(&filter_lower)
        });
    }

    // Hide idle sessions if flag is set
    if state.pga.hide_idle {
        rows_data.retain(|row| !is_idle_state(&row.state));
    }

    // Get view mode early as it affects sorting
    let view_mode = state.pga.view_mode;

    // For Stats view, enrich rows with pg_stat_statements data before sorting
    if view_mode == PgActivityViewMode::Stats {
        // Build map of queryid -> PgStatStatementsInfo
        let pgs_map = extract_pg_statements_map(snapshot);
        for row in &mut rows_data {
            if row.query_id != 0
                && let Some(pgs_info) = pgs_map.get(&row.query_id)
            {
                let rates = state.pgs.rates.get(&row.query_id);
                row.enrich_with_pgs_stats(pgs_info, rates);
            }
        }
    }

    // Sort by selected column, with idle sessions always at bottom
    let sort_col = state.pga.sort_column;
    let sort_asc = state.pga.sort_ascending;
    rows_data.sort_by(|a, b| {
        let a_idle = is_idle_state(&a.state);
        let b_idle = is_idle_state(&b.state);

        // Idle sessions always at bottom
        match (a_idle, b_idle) {
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            _ => {}
        }

        // Sort by selected column (using view mode-specific sort key)
        let cmp = a
            .sort_key_for_mode(sort_col, view_mode)
            .partial_cmp(&b.sort_key_for_mode(sort_col, view_mode))
            .unwrap_or(std::cmp::Ordering::Equal);
        if sort_asc { cmp } else { cmp.reverse() }
    });

    // Resolve selection: navigate_to, tracking, clamping, ratatui sync
    let row_pids: Vec<i32> = rows_data.iter().map(|r| r.pid).collect();
    state.pga.resolve_selection(&row_pids);

    // Select headers and widths based on view mode
    let (headers_arr, widths_arr, view_indicator) = match view_mode {
        PgActivityViewMode::Generic => (PGA_HEADERS_GENERIC, PGA_WIDTHS_GENERIC, "g:generic"),
        PgActivityViewMode::Stats => (PGA_HEADERS_STATS, PGA_WIDTHS_STATS, "v:stats"),
    };

    // Build header row with sort indicator
    let sort_col = state.pga.sort_column;
    let sort_asc = state.pga.sort_ascending;
    let headers: Vec<Span> = headers_arr
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let indicator = if i == sort_col {
                if sort_asc { "▲" } else { "▼" }
            } else {
                ""
            };
            Span::styled(format!("{}{}", h, indicator), Styles::table_header())
        })
        .collect();
    let header = Row::new(headers).style(Styles::table_header()).height(1);

    // Build data rows based on view mode
    let rows: Vec<Row> = rows_data
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let is_selected = idx == state.pga.selected;
            let is_idle = is_idle_state(&row.state);

            let cells: Vec<Span> = match view_mode {
                PgActivityViewMode::Generic => vec![
                    Span::raw(format!("{:>6}", row.pid)),
                    styled_cpu(row.cpu_percent),
                    Span::raw(format!("{:>7}", format_bytes(row.rss_bytes))),
                    Span::raw(truncate(&row.db, 32)),
                    Span::raw(truncate(&row.user, 32)),
                    styled_state(&row.state),
                    styled_wait(&row.wait, is_idle),
                    styled_duration(row.query_duration_secs, &row.state),
                    Span::raw(format_duration_or_dash(row.xact_duration_secs)),
                    Span::raw(format_duration_or_dash(row.backend_duration_secs)),
                    Span::raw(truncate(&row.backend_type, 14)),
                    Span::raw(normalize_query(&row.query)),
                ],
                PgActivityViewMode::Stats => vec![
                    Span::raw(format!("{:>6}", row.pid)),
                    Span::raw(truncate(&row.db, 32)),
                    Span::raw(truncate(&row.user, 32)),
                    styled_state(&row.state),
                    styled_qdur_with_anomaly(
                        row.query_duration_secs,
                        &row.state,
                        row.pgs_mean_exec_time,
                        row.pgs_max_exec_time,
                    ),
                    styled_mean(row.pgs_mean_exec_time),
                    styled_max(row.pgs_max_exec_time),
                    styled_calls_s(row.pgs_calls_s),
                    styled_hit_pct(row.pgs_hit_pct),
                    Span::raw(normalize_query(&row.query)),
                ],
            };

            let row_style = if is_selected {
                Styles::selected()
            } else if is_idle {
                Style::default().fg(Color::DarkGray)
            } else {
                Styles::default()
            };

            Row::new(cells).style(row_style).height(1)
        })
        .collect();

    let title = {
        let idle_marker = if state.pga.hide_idle {
            " [hide idle]"
        } else {
            ""
        };
        if let Some(filter) = &state.pga.filter {
            format!(
                " PostgreSQL Activity (PGA) [{}] (filter: {}){} [{} sessions] ",
                view_indicator,
                filter,
                idle_marker,
                rows_data.len()
            )
        } else {
            format!(
                " PostgreSQL Activity (PGA) [{}]{} [{} sessions] ",
                view_indicator,
                idle_marker,
                rows_data.len()
            )
        }
    };

    // Build widths with QUERY taking remaining space
    let mut widths: Vec<ratatui::layout::Constraint> = widths_arr
        .iter()
        .map(|&w| ratatui::layout::Constraint::Length(w))
        .collect();
    widths.push(ratatui::layout::Constraint::Fill(1)); // QUERY column fills remaining space

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(Styles::default()),
        )
        .row_highlight_style(Styles::selected());

    // Clear the area before rendering to avoid artifacts
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pga.ratatui_state);
}
