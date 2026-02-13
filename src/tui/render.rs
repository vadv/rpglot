//! Main rendering logic for TUI.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::collector::CollectorTiming;
use crate::storage::StringInterner;

use super::state::{AppState, InputMode, PopupState, Tab};
use super::widgets::{
    calculate_summary_height, render_debug_popup, render_header, render_help, render_pg_detail,
    render_pg_indexes, render_pg_locks, render_pg_statements, render_pg_tables, render_pgi_detail,
    render_pgl_detail, render_pgs_detail, render_pgt_detail, render_postgres,
    render_process_detail, render_processes, render_quit_confirm, render_summary, render_time_jump,
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

    // Popups (rendered last to overlay everything).
    // Determine which popup to render first, then call render functions
    // (avoids borrow conflicts between &mut state.popup and &mut state).
    #[derive(Clone, Copy)]
    enum ActivePopup {
        None,
        Help,
        ProcessDetail,
        PgDetail,
        PgsDetail,
        PgtDetail,
        PgiDetail,
        PglDetail,
        Debug,
        QuitConfirm,
    }
    let active = match &state.popup {
        PopupState::None => ActivePopup::None,
        PopupState::Help { .. } => ActivePopup::Help,
        PopupState::ProcessDetail { .. } if state.current_tab == Tab::Processes => {
            ActivePopup::ProcessDetail
        }
        PopupState::PgDetail { .. } if state.current_tab == Tab::PostgresActive => {
            ActivePopup::PgDetail
        }
        PopupState::PgsDetail { .. } if state.current_tab == Tab::PgStatements => {
            ActivePopup::PgsDetail
        }
        PopupState::PgtDetail { .. } if state.current_tab == Tab::PgTables => {
            ActivePopup::PgtDetail
        }
        PopupState::PgiDetail { .. } if state.current_tab == Tab::PgIndexes => {
            ActivePopup::PgiDetail
        }
        PopupState::PglDetail { .. } if state.current_tab == Tab::PgLocks => ActivePopup::PglDetail,
        PopupState::Debug if state.is_live => ActivePopup::Debug,
        PopupState::QuitConfirm => ActivePopup::QuitConfirm,
        _ => ActivePopup::None,
    };
    match active {
        ActivePopup::Help => {
            if let PopupState::Help { ref mut scroll } = state.popup {
                render_help(
                    frame,
                    area,
                    state.current_tab,
                    state.process_view_mode,
                    state.pgs.view_mode,
                    state.pgt.view_mode,
                    state.pgi.view_mode,
                    scroll,
                );
            }
        }
        ActivePopup::ProcessDetail => render_process_detail(frame, area, state),
        ActivePopup::PgDetail => render_pg_detail(frame, area, state, interner),
        ActivePopup::PgsDetail => render_pgs_detail(frame, area, state, interner),
        ActivePopup::PgtDetail => render_pgt_detail(frame, area, state, interner),
        ActivePopup::PgiDetail => render_pgi_detail(frame, area, state, interner),
        ActivePopup::PglDetail => render_pgl_detail(frame, area, state, interner),
        ActivePopup::Debug => render_debug_popup(frame, area, state, timing),
        ActivePopup::QuitConfirm => render_quit_confirm(frame, area),
        ActivePopup::None => {}
    }

    // Time jump popup (history mode, `b`) - not part of PopupState since it's an InputMode
    if state.input_mode == InputMode::TimeJump && !matches!(state.popup, PopupState::QuitConfirm) {
        render_time_jump(
            frame,
            area,
            &state.time_jump_input,
            state.time_jump_error.as_deref(),
        );
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
        Tab::PgTables => render_pg_tables(frame, area, state, interner),
        Tab::PgIndexes => render_pg_indexes(frame, area, state, interner),
        Tab::PgLocks => render_pg_locks(frame, area, state, interner),
    }
}
