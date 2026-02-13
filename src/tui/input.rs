//! Input handling and keybindings.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::state::{AppState, InputMode, ProcessViewMode, Tab};

/// Result of handling a key event.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// No action, continue.
    None,
    /// Quit the application.
    Quit,
    /// Advance to next snapshot.
    Advance,
    /// Rewind to previous snapshot.
    Rewind,
    /// Jump to a specific time (history mode, `b`).
    JumpToTime,
}

/// Handles key input and updates state.
pub fn handle_key(state: &mut AppState, key: KeyEvent) -> KeyAction {
    if state.show_quit_confirm {
        return handle_quit_confirm(state, key);
    }
    match state.input_mode {
        InputMode::Normal => handle_normal_mode(state, key),
        InputMode::Filter => handle_filter_mode(state, key),
        InputMode::TimeJump => handle_time_jump_mode(state, key),
    }
}

fn handle_quit_confirm(state: &mut AppState, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('Q') => {
            state.show_quit_confirm = false;
            KeyAction::Quit
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.show_quit_confirm = false;
            KeyAction::Quit
        }
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            state.show_quit_confirm = false;
            KeyAction::None
        }
        _ => KeyAction::None,
    }
}

/// Handles keys in normal mode.
fn handle_normal_mode(state: &mut AppState, key: KeyEvent) -> KeyAction {
    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            state.show_quit_confirm = true;
            KeyAction::None
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,

        // Tab navigation (blocked when a detail popup is open)
        KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Char('1')
        | KeyCode::Char('2')
        | KeyCode::Char('3')
            if state.any_popup_open() =>
        {
            state.status_message = Some("Close popup (Esc) before switching tabs".to_string());
            KeyAction::None
        }
        KeyCode::Tab => {
            state.switch_tab(state.current_tab.next());
            KeyAction::None
        }
        KeyCode::BackTab => {
            state.switch_tab(state.current_tab.prev());
            KeyAction::None
        }
        KeyCode::Char('1') => {
            state.switch_tab(Tab::Processes);
            KeyAction::None
        }
        KeyCode::Char('2') => {
            state.switch_tab(Tab::PostgresActive);
            KeyAction::None
        }
        KeyCode::Char('3') => {
            state.switch_tab(Tab::PgStatements);
            KeyAction::None
        }

        // Row navigation (or popup scroll if popup is open)
        KeyCode::Up | KeyCode::Char('k') => {
            if state.show_help {
                state.help_scroll = state.help_scroll.saturating_sub(1);
            } else if state.show_process_detail {
                state.process_detail_scroll = state.process_detail_scroll.saturating_sub(1);
            } else if state.show_pg_detail {
                state.pg_detail_scroll = state.pg_detail_scroll.saturating_sub(1);
            } else if state.show_pgs_detail {
                state.pgs_detail_scroll = state.pgs_detail_scroll.saturating_sub(1);
            } else if state.current_tab == Tab::Processes {
                state.process_table.select_up();
            } else if state.current_tab == Tab::PostgresActive {
                state.pg_selected = state.pg_selected.saturating_sub(1);
                state.pg_tracked_pid = None; // Clear tracking on manual navigation
            } else if state.current_tab == Tab::PgStatements {
                state.pgs_selected = state.pgs_selected.saturating_sub(1);
                state.pgs_tracked_queryid = None; // Clear tracking on manual navigation
            }
            KeyAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.show_help {
                state.help_scroll = state.help_scroll.saturating_add(1);
            } else if state.show_process_detail {
                // Increment but will be clamped during render
                state.process_detail_scroll = state.process_detail_scroll.saturating_add(1);
            } else if state.show_pg_detail {
                state.pg_detail_scroll = state.pg_detail_scroll.saturating_add(1);
            } else if state.show_pgs_detail {
                state.pgs_detail_scroll = state.pgs_detail_scroll.saturating_add(1);
            } else if state.current_tab == Tab::Processes {
                state.process_table.select_down();
            } else if state.current_tab == Tab::PostgresActive {
                // Will be clamped during render
                state.pg_selected = state.pg_selected.saturating_add(1);
                state.pg_tracked_pid = None; // Clear tracking on manual navigation
            } else if state.current_tab == Tab::PgStatements {
                // Will be clamped during render
                state.pgs_selected = state.pgs_selected.saturating_add(1);
                state.pgs_tracked_queryid = None; // Clear tracking on manual navigation
            }
            KeyAction::None
        }
        KeyCode::PageUp => {
            if state.show_help {
                state.help_scroll = state.help_scroll.saturating_sub(10);
            } else if state.show_process_detail {
                state.process_detail_scroll = state.process_detail_scroll.saturating_sub(10);
            } else if state.show_pg_detail {
                state.pg_detail_scroll = state.pg_detail_scroll.saturating_sub(10);
            } else if state.show_pgs_detail {
                state.pgs_detail_scroll = state.pgs_detail_scroll.saturating_sub(10);
            } else if state.current_tab == Tab::Processes {
                state.process_table.page_up(20);
            } else if state.current_tab == Tab::PostgresActive {
                state.pg_selected = state.pg_selected.saturating_sub(20);
                state.pg_tracked_pid = None; // Clear tracking on manual navigation
            } else if state.current_tab == Tab::PgStatements {
                state.pgs_selected = state.pgs_selected.saturating_sub(20);
                state.pgs_tracked_queryid = None; // Clear tracking on manual navigation
            }
            KeyAction::None
        }
        KeyCode::PageDown => {
            if state.show_help {
                state.help_scroll = state.help_scroll.saturating_add(10);
            } else if state.show_process_detail {
                state.process_detail_scroll += 10;
            } else if state.show_pg_detail {
                state.pg_detail_scroll += 10;
            } else if state.show_pgs_detail {
                state.pgs_detail_scroll += 10;
            } else if state.current_tab == Tab::Processes {
                state.process_table.page_down(20);
            } else if state.current_tab == Tab::PostgresActive {
                state.pg_selected = state.pg_selected.saturating_add(20);
                state.pg_tracked_pid = None; // Clear tracking on manual navigation
            } else if state.current_tab == Tab::PgStatements {
                state.pgs_selected = state.pgs_selected.saturating_add(20);
                state.pgs_tracked_queryid = None; // Clear tracking on manual navigation
            }
            KeyAction::None
        }
        KeyCode::Home => {
            if state.current_tab == Tab::Processes {
                state.process_table.selected = 0;
            } else if state.current_tab == Tab::PostgresActive {
                state.pg_selected = 0;
                state.pg_tracked_pid = None; // Clear tracking on manual navigation
            } else if state.current_tab == Tab::PgStatements {
                state.pgs_selected = 0;
                state.pgs_tracked_queryid = None; // Clear tracking on manual navigation
            }
            KeyAction::None
        }
        KeyCode::End => {
            if state.current_tab == Tab::Processes {
                let len = state.process_table.filtered_items().len();
                if len > 0 {
                    state.process_table.selected = len - 1;
                }
            } else if state.current_tab == Tab::PostgresActive {
                // Will be clamped during render; use large value to go to end
                state.pg_selected = usize::MAX;
                state.pg_tracked_pid = None; // Clear tracking on manual navigation
            } else if state.current_tab == Tab::PgStatements {
                // Will be clamped during render; use large value to go to end
                state.pgs_selected = usize::MAX;
                state.pgs_tracked_queryid = None; // Clear tracking on manual navigation
            }
            KeyAction::None
        }

        // Sorting
        KeyCode::Char('s') | KeyCode::Char('S') => {
            match state.current_tab {
                Tab::Processes => state.next_process_sort_column(),
                Tab::PostgresActive => state.next_pg_sort_column(),
                Tab::PgStatements => state.next_pgs_sort_column(),
            }
            KeyAction::None
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            match state.current_tab {
                Tab::Processes => state.toggle_process_sort_direction(),
                Tab::PostgresActive => state.toggle_pg_sort_direction(),
                Tab::PgStatements => state.toggle_pgs_sort_direction(),
            }
            KeyAction::None
        }

        // Filter mode (/ or p/P for process name filter like atop)
        KeyCode::Char('/') | KeyCode::Char('p') | KeyCode::Char('P') => {
            state.input_mode = InputMode::Filter;
            state.filter_input.clear();
            KeyAction::None
        }

        // Jump to time (history mode only)
        KeyCode::Char('b') | KeyCode::Char('B') => {
            if !state.is_live {
                state.input_mode = InputMode::TimeJump;
                state.time_jump_input.clear();
                state.time_jump_error = None;
            }
            KeyAction::None
        }

        // History navigation (arrows or t/T)
        KeyCode::Left | KeyCode::Char('T') => {
            if !state.is_live {
                KeyAction::Rewind
            } else {
                KeyAction::None
            }
        }
        KeyCode::Right => {
            if !state.is_live {
                KeyAction::Advance
            } else {
                KeyAction::None
            }
        }

        // PGS view mode: t/c/i/e (context-sensitive, overrides history 't' on PGS tab)
        KeyCode::Char('t') => {
            if state.current_tab == Tab::PgStatements {
                state.pgs_view_mode = super::state::PgStatementsViewMode::Time;
                state.pgs_selected = 0;
                state.pgs_sort_column = 1; // TIME/s
                state.pgs_sort_ascending = false;
                KeyAction::None
            } else if !state.is_live {
                KeyAction::Advance
            } else {
                KeyAction::None
            }
        }

        // Pause/Resume
        KeyCode::Char(' ') => {
            state.paused = !state.paused;
            KeyAction::None
        }

        // Process view modes (atop-style) and PGA view modes
        KeyCode::Char('g') | KeyCode::Char('G') => {
            if state.current_tab == Tab::Processes {
                state.process_view_mode = ProcessViewMode::Generic;
                state.horizontal_scroll = 0;
                state.process_table.sort_column = 0;
                state.apply_process_sort();
            } else if state.current_tab == Tab::PostgresActive {
                state.pga_view_mode = super::state::PgActivityViewMode::Generic;
                state.pg_sort_column = 7; // QDUR
                state.pg_sort_ascending = false;
            }
            KeyAction::None
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            if state.current_tab == Tab::Processes {
                state.process_view_mode = ProcessViewMode::Command;
                state.horizontal_scroll = 0;
                state.process_table.sort_column = 0;
                state.apply_process_sort();
            } else if state.current_tab == Tab::PgStatements {
                state.pgs_view_mode = super::state::PgStatementsViewMode::Calls;
                state.pgs_selected = 0;
                state.pgs_sort_column = 0; // CALLS/s
                state.pgs_sort_ascending = false;
            }
            KeyAction::None
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            if state.current_tab == Tab::Processes {
                state.process_view_mode = ProcessViewMode::Memory;
                state.horizontal_scroll = 0;
                state.process_table.sort_column = 0;
                state.apply_process_sort();
            }
            KeyAction::None
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            if state.current_tab == Tab::Processes {
                state.process_view_mode = ProcessViewMode::Disk;
                state.horizontal_scroll = 0;
                state.process_table.sort_column = 0;
                state.apply_process_sort();
            }
            KeyAction::None
        }

        // Horizontal scroll for wide tables
        KeyCode::Char('h') => {
            if state.current_tab == Tab::Processes && state.horizontal_scroll > 0 {
                state.horizontal_scroll -= 1;
            }
            KeyAction::None
        }
        KeyCode::Char('l') => {
            if state.current_tab == Tab::Processes {
                state.horizontal_scroll += 1;
            }
            KeyAction::None
        }

        // PGA view mode: v for Stats view
        KeyCode::Char('v') | KeyCode::Char('V') => {
            if state.current_tab == Tab::PostgresActive {
                // Toggle between Generic and Stats view
                state.pga_view_mode = match state.pga_view_mode {
                    super::state::PgActivityViewMode::Generic => {
                        state.pg_sort_column = 4; // QDUR in Stats view
                        super::state::PgActivityViewMode::Stats
                    }
                    super::state::PgActivityViewMode::Stats => {
                        state.pg_sort_column = 7; // QDUR in Generic view
                        super::state::PgActivityViewMode::Generic
                    }
                };
                state.pg_sort_ascending = false;
            }
            KeyAction::None
        }

        // PGA hide idle / PGS IO view
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if state.current_tab == Tab::PostgresActive {
                state.pg_hide_idle = !state.pg_hide_idle;
                state.pg_selected = 0; // Reset selection when toggling
            } else if state.current_tab == Tab::PgStatements {
                state.pgs_view_mode = super::state::PgStatementsViewMode::Io;
                state.pgs_selected = 0;
                state.pgs_sort_column = 1; // BLK_RD/s
                state.pgs_sort_ascending = false;
            }
            KeyAction::None
        }

        // PGS Temp view
        KeyCode::Char('e') | KeyCode::Char('E') => {
            if state.current_tab == Tab::PgStatements {
                state.pgs_view_mode = super::state::PgStatementsViewMode::Temp;
                state.pgs_selected = 0;
                state.pgs_sort_column = 3; // TMP_MB/s
                state.pgs_sort_ascending = false;
            }
            KeyAction::None
        }

        // Help popup
        KeyCode::Char('?') | KeyCode::Char('H') => {
            state.show_help = !state.show_help;
            if state.show_help {
                state.help_scroll = 0; // Reset scroll on open
            }
            KeyAction::None
        }

        // Debug popup (collector timing, rates state) - live mode only
        KeyCode::Char('!') => {
            if state.is_live {
                state.show_debug_popup = !state.show_debug_popup;
            }
            KeyAction::None
        }

        // Detail popup (Enter on PRC or PGA tab)
        KeyCode::Enter => {
            if state.current_tab == Tab::Processes && !state.process_table.items.is_empty() {
                state.show_process_detail = !state.show_process_detail;
                if state.show_process_detail {
                    // Remember PID of the selected process
                    let filtered = state.process_table.filtered_items();
                    let selected_idx = state
                        .process_table
                        .selected
                        .min(filtered.len().saturating_sub(1));
                    if let Some(row) = filtered.get(selected_idx) {
                        state.process_detail_pid = Some(row.pid);
                    }
                    state.process_detail_scroll = 0; // Reset scroll on open
                } else {
                    state.process_detail_pid = None;
                }
            } else if state.current_tab == Tab::PostgresActive {
                // Toggle PG detail popup
                state.show_pg_detail = !state.show_pg_detail;
                if state.show_pg_detail {
                    // Copy tracked PID to detail PID (locks the popup to this session)
                    state.pg_detail_pid = state.pg_tracked_pid;
                    state.pg_detail_scroll = 0;
                } else {
                    state.pg_detail_pid = None;
                }
            } else if state.current_tab == Tab::PgStatements {
                // Toggle PGS detail popup
                state.show_pgs_detail = !state.show_pgs_detail;
                if state.show_pgs_detail {
                    // Copy tracked queryid to detail queryid (locks the popup to this statement)
                    state.pgs_detail_queryid = state.pgs_tracked_queryid;
                    state.pgs_detail_scroll = 0;
                } else {
                    state.pgs_detail_queryid = None;
                }
            }
            KeyAction::None
        }

        // Drill-down navigation (PRC -> PGA -> PGS)
        KeyCode::Char('>') | KeyCode::Char('J') => {
            if state.any_popup_open() {
                state.status_message = Some("Close popup (Esc) before drill-down".to_string());
            } else if state.current_tab == Tab::Processes
                || state.current_tab == Tab::PostgresActive
            {
                state.drill_down_requested = true;
            }
            KeyAction::None
        }

        // Close popups with Escape
        KeyCode::Esc => {
            state.status_message = None;
            if state.show_process_detail {
                state.show_process_detail = false;
                state.process_detail_pid = None;
            } else if state.show_pg_detail {
                state.show_pg_detail = false;
                state.pg_detail_pid = None;
            } else if state.show_pgs_detail {
                state.show_pgs_detail = false;
                state.pgs_detail_queryid = None;
            } else if state.show_help {
                state.show_help = false;
            } else if state.show_debug_popup {
                state.show_debug_popup = false;
            }
            KeyAction::None
        }

        _ => KeyAction::None,
    }
}

fn handle_time_jump_mode(state: &mut AppState, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            state.time_jump_input.clear();
            state.time_jump_error = None;
            KeyAction::None
        }
        KeyCode::Enter => KeyAction::JumpToTime,
        KeyCode::Backspace => {
            state.time_jump_input.pop();
            state.time_jump_error = None;
            KeyAction::None
        }
        KeyCode::Char(c) => {
            // Ignore control/alt-modified chars
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                return KeyAction::None;
            }
            state.time_jump_input.push(c);
            state.time_jump_error = None;
            KeyAction::None
        }
        _ => KeyAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::PgStatementsViewMode;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn pgs_tab_switches_with_3() {
        let mut state = AppState::new(true);
        assert_eq!(state.current_tab, Tab::Processes);

        let action = handle_key(&mut state, key(KeyCode::Char('3')));
        assert_eq!(action, KeyAction::None);
        assert_eq!(state.current_tab, Tab::PgStatements);
    }

    #[test]
    fn pgs_view_mode_keys_switch_modes_and_set_defaults() {
        let mut state = AppState::new(true);
        state.current_tab = Tab::PgStatements;

        let _ = handle_key(&mut state, key(KeyCode::Char('c')));
        assert_eq!(state.pgs_view_mode, PgStatementsViewMode::Calls);
        assert_eq!(state.pgs_sort_column, 0);
        assert!(!state.pgs_sort_ascending);

        let _ = handle_key(&mut state, key(KeyCode::Char('i')));
        assert_eq!(state.pgs_view_mode, PgStatementsViewMode::Io);
        assert_eq!(state.pgs_sort_column, 1);

        let _ = handle_key(&mut state, key(KeyCode::Char('e')));
        assert_eq!(state.pgs_view_mode, PgStatementsViewMode::Temp);
        assert_eq!(state.pgs_sort_column, 3);

        let _ = handle_key(&mut state, key(KeyCode::Char('t')));
        assert_eq!(state.pgs_view_mode, PgStatementsViewMode::Time);
        assert_eq!(state.pgs_sort_column, 1);
    }

    #[test]
    fn filter_mode_applies_to_pgs_filter() {
        let mut state = AppState::new(true);
        state.current_tab = Tab::PgStatements;

        // Enter filter mode
        let _ = handle_key(&mut state, key(KeyCode::Char('/')));
        assert_eq!(state.input_mode, InputMode::Filter);
        assert_eq!(state.pgs_filter, None);

        // Type a filter string
        let _ = handle_key(&mut state, key(KeyCode::Char('a')));
        assert_eq!(state.pgs_filter.as_deref(), Some("a"));

        // Cancel
        let _ = handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.pgs_filter, None);
    }

    #[test]
    fn quit_requires_confirmation_and_quits_on_qq() {
        let mut state = AppState::new(true);

        let action = handle_key(&mut state, key(KeyCode::Char('q')));
        assert_eq!(action, KeyAction::None);
        assert!(state.show_quit_confirm);

        let action = handle_key(&mut state, key(KeyCode::Char('q')));
        assert_eq!(action, KeyAction::Quit);
        assert!(!state.show_quit_confirm);
    }

    #[test]
    fn quit_confirmation_cancels_on_esc() {
        let mut state = AppState::new(true);

        let _ = handle_key(&mut state, key(KeyCode::Char('q')));
        assert!(state.show_quit_confirm);

        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(action, KeyAction::None);
        assert!(!state.show_quit_confirm);
    }

    #[test]
    fn quit_confirmation_quits_on_enter() {
        let mut state = AppState::new(true);

        let _ = handle_key(&mut state, key(KeyCode::Char('q')));
        assert!(state.show_quit_confirm);

        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(action, KeyAction::Quit);
        assert!(!state.show_quit_confirm);
    }

    #[test]
    fn tab_switch_blocked_when_popup_open() {
        let mut state = AppState::new(true);
        state.show_process_detail = true;

        // Tab should be blocked
        let _ = handle_key(&mut state, key(KeyCode::Tab));
        assert_eq!(state.current_tab, Tab::Processes);
        assert!(state.status_message.is_some());

        // Number keys should be blocked too
        state.status_message = None;
        let _ = handle_key(&mut state, key(KeyCode::Char('2')));
        assert_eq!(state.current_tab, Tab::Processes);
        assert!(state.status_message.is_some());

        // After closing popup, tab switch works
        let _ = handle_key(&mut state, key(KeyCode::Esc));
        assert!(!state.show_process_detail);
        assert!(state.status_message.is_none());

        let _ = handle_key(&mut state, key(KeyCode::Char('2')));
        assert_eq!(state.current_tab, Tab::PostgresActive);
    }

    #[test]
    fn drill_down_blocked_when_popup_open() {
        let mut state = AppState::new(true);
        state.show_process_detail = true;

        let _ = handle_key(&mut state, key(KeyCode::Char('>')));
        assert!(!state.drill_down_requested);
        assert!(state.status_message.is_some());
    }
}

/// Handles keys in filter mode.
fn handle_filter_mode(state: &mut AppState, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc => {
            // Cancel filter
            state.input_mode = InputMode::Normal;
            state.filter_input.clear();
            match state.current_tab {
                Tab::Processes => state.process_table.set_filter(None),
                Tab::PostgresActive => state.pg_filter = None,
                Tab::PgStatements => state.pgs_filter = None,
            }
            KeyAction::None
        }
        KeyCode::Enter => {
            // Confirm filter and return to normal mode
            state.input_mode = InputMode::Normal;
            // Filter is already applied in real-time, just switch mode
            KeyAction::None
        }
        KeyCode::Backspace => {
            state.filter_input.pop();
            // Apply filter in real-time
            apply_current_filter(state);
            KeyAction::None
        }
        KeyCode::Char(c) => {
            state.filter_input.push(c);
            // Apply filter in real-time
            apply_current_filter(state);
            KeyAction::None
        }
        _ => KeyAction::None,
    }
}

/// Applies the current filter_input to the appropriate table.
fn apply_current_filter(state: &mut AppState) {
    let filter = if state.filter_input.is_empty() {
        None
    } else {
        Some(state.filter_input.clone())
    };
    match state.current_tab {
        Tab::Processes => state.process_table.set_filter(filter),
        Tab::PostgresActive => state.pg_filter = filter,
        Tab::PgStatements => state.pgs_filter = filter,
    }
}
