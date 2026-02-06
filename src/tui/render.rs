//! Main rendering logic for TUI.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::collector::CollectorTiming;
use crate::storage::StringInterner;

use super::state::{AppState, InputMode, Tab};
use super::widgets::{
    calculate_summary_height, render_debug_popup, render_header, render_help, render_pg_detail,
    render_pg_statements, render_pgs_detail, render_postgres, render_process_detail,
    render_processes, render_quit_confirm, render_summary, render_time_jump,
};

/// Main render function.
pub fn render(
    frame: &mut Frame,
    state: &mut AppState,
    interner: Option<&StringInterner>,
    timing: Option<&CollectorTiming>,
) {
    let area = frame.area();

    // Calculate summary height dynamically based on content
    let summary_height = calculate_summary_height(state.current_snapshot.as_ref());

    // Main layout: header, summary, content
    let chunks = Layout::vertical([
        Constraint::Length(1),              // Header
        Constraint::Length(summary_height), // Summary (dynamic: MEM, SWP, DSK×N, NET×N | CPL, CPU, cpu×N, Help)
        Constraint::Min(10),                // Content area
    ])
    .split(area);

    // Header
    render_header(frame, chunks[0], state);

    // Summary (atop-style: CPL, CPU, MEM, SWP, DSK, NET)
    render_summary(
        frame,
        chunks[1],
        state.current_snapshot.as_ref(),
        state.previous_snapshot.as_ref(),
        state.current_tab,
    );

    // Content based on tab
    render_content(frame, chunks[2], state, interner);

    // Help popup (rendered last to overlay everything)
    if state.show_help {
        render_help(
            frame,
            area,
            state.current_tab,
            state.process_view_mode,
            state.pgs_view_mode,
            &mut state.help_scroll,
        );
    }

    // Process detail popup (on PRC tab, Enter key)
    if state.show_process_detail && state.current_tab == Tab::Processes {
        render_process_detail(frame, area, state);
    }

    // PostgreSQL session detail popup (on PGA tab, Enter key)
    if state.show_pg_detail && state.current_tab == Tab::PostgresActive {
        render_pg_detail(frame, area, state, interner);
    }

    // pg_stat_statements detail popup (on PGS tab, Enter key)
    if state.show_pgs_detail && state.current_tab == Tab::PgStatements {
        render_pgs_detail(frame, area, state, interner);
    }

    // Time jump popup (history mode, `b`)
    if state.input_mode == InputMode::TimeJump && !state.show_quit_confirm {
        render_time_jump(
            frame,
            area,
            &state.time_jump_input,
            state.time_jump_error.as_deref(),
        );
    }

    // Debug popup (live mode only)
    if state.show_debug_popup && state.is_live {
        render_debug_popup(frame, area, state, timing);
    }

    // Quit confirmation popup (rendered last to overlay everything)
    if state.show_quit_confirm {
        render_quit_confirm(frame, area);
    }
}

/// Renders content based on current tab.
fn render_content(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    match state.current_tab {
        Tab::Processes => render_processes(frame, area, state),
        Tab::PostgresActive => render_postgres(frame, area, state, interner),
        Tab::PgStatements => render_pg_statements(frame, area, state, interner),
    }
}
