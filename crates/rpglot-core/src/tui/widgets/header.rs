//! Header widget showing time, mode, and tabs.

use chrono::{DateTime, Local, TimeZone};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::storage::model::DataBlock;
use crate::tui::state::{AppState, InputMode, Tab};
use crate::tui::style::Styles;

/// Renders the header bar.
pub fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::horizontal([
        Constraint::Length(22), // Time
        Constraint::Length(12), // Mode
        Constraint::Min(20),    // Tabs
        Constraint::Length(16), // PIDs (container)
        Constraint::Length(42), // Position/Filter/Status
    ])
    .split(area);

    // Time
    let timestamp = state
        .current_snapshot
        .as_ref()
        .map(|s| s.timestamp)
        .unwrap_or_else(|| Local::now().timestamp());
    let time_str = Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|dt: DateTime<Local>| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "----".to_string());
    let time = Paragraph::new(time_str).style(Styles::header());
    frame.render_widget(time, chunks[0]);

    // Mode
    let mode_str = if state.is_live {
        if state.paused { " PAUSED " } else { " LIVE " }
    } else {
        " HISTORY "
    };
    let mode = Paragraph::new(mode_str).style(Styles::header());
    frame.render_widget(mode, chunks[1]);

    // Tabs
    let tabs: Vec<Span> = Tab::all()
        .iter()
        .enumerate()
        .flat_map(|(i, tab)| {
            let style = if *tab == state.current_tab {
                Styles::tab_active()
            } else {
                Styles::tab_inactive()
            };
            let num = format!(" {}:", i + 1);
            let name = format!("{} ", tab.name());
            vec![Span::styled(num, Styles::dim()), Span::styled(name, style)]
        })
        .collect();
    let tabs_line = Line::from(tabs);
    let tabs_widget = Paragraph::new(tabs_line).style(Styles::header());
    frame.render_widget(tabs_widget, chunks[2]);

    // PIDs (container only, only when limit is set)
    let pids_info = state.current_snapshot.as_ref().and_then(|snap| {
        snap.blocks.iter().find_map(|b| {
            if let DataBlock::Cgroup(cg) = b {
                cg.pids.as_ref().and_then(|p| {
                    if p.max == u64::MAX {
                        None
                    } else {
                        Some((p.current, p.max))
                    }
                })
            } else {
                None
            }
        })
    });

    let (pids_text, pids_style) = if let Some((current, max)) = pids_info {
        let style = if max > 0 {
            let pct = (current as f64 / max as f64) * 100.0;
            if pct > 95.0 {
                Styles::critical()
            } else if pct > 80.0 {
                Styles::modified_item()
            } else {
                Styles::header()
            }
        } else {
            Styles::header()
        };
        (format!("PIDs: {}/{}", current, max), style)
    } else {
        (String::new(), Styles::header())
    };
    frame.render_widget(Paragraph::new(pids_text).style(pids_style), chunks[3]);

    // Position, Filter input, or status message
    let current_filter = match state.current_tab {
        Tab::Processes => state.process_table.filter.as_deref(),
        Tab::PostgresActive => state.pga.filter.as_deref(),
        Tab::PgStatements => state.pgs.filter.as_deref(),
        Tab::PgTables => state.pgt.filter.as_deref(),
        Tab::PgIndexes => state.pgi.filter.as_deref(),
        Tab::PgErrors => state.pge.filter.as_deref(),
        Tab::PgLocks => state.pgl.filter.as_deref(),
    };
    let (right_content, right_style) = if let Some(msg) = &state.status_message {
        (msg.clone(), Styles::modified_item())
    } else {
        match state.input_mode {
            InputMode::Filter => (
                format!("Filter: {}█", state.filter_input),
                Styles::filter_input(),
            ),
            InputMode::TimeJump => (
                format!("Jump: {}█", state.time_jump_input),
                Styles::filter_input(),
            ),
            InputMode::Normal => {
                let text = if let Some((pos, total)) = state.history_position {
                    format!("{}/{}", pos + 1, total)
                } else if let Some(filter) = current_filter {
                    format!("/{}", filter)
                } else {
                    String::new()
                };
                (text, Styles::header())
            }
        }
    };
    let right = Paragraph::new(right_content).style(right_style);
    frame.render_widget(right, chunks[4]);
}
