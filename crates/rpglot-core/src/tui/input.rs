//! Input handling and keybindings.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::navigable::NavigableTable;
use super::state::{AppState, InputMode, PopupState, ProcessViewMode, Tab};

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

/// Navigation action for unified scroll/selection dispatch.
enum NavAction {
    Up,
    Down,
    PageUp(usize),
    PageDown(usize),
    Home,
    End,
}

/// Dispatches a navigation action to the appropriate popup scroll or tab selection.
fn dispatch_navigation(state: &mut AppState, action: NavAction) {
    match &mut state.popup {
        PopupState::Help { scroll } => match action {
            NavAction::Up => *scroll = scroll.saturating_sub(1),
            NavAction::Down => *scroll = scroll.saturating_add(1),
            NavAction::PageUp(n) => *scroll = scroll.saturating_sub(n),
            NavAction::PageDown(n) => *scroll = scroll.saturating_add(n),
            NavAction::Home => *scroll = 0,
            NavAction::End => {} // no-op for help
        },
        PopupState::ProcessDetail { scroll, .. }
        | PopupState::PgDetail { scroll, .. }
        | PopupState::PgsDetail { scroll, .. }
        | PopupState::PgtDetail { scroll, .. }
        | PopupState::PgiDetail { scroll, .. }
        | PopupState::PgpDetail { scroll, .. }
        | PopupState::PgeDetail { scroll, .. }
        | PopupState::PglDetail { scroll, .. } => match action {
            NavAction::Up => *scroll = scroll.saturating_sub(1),
            NavAction::Down => *scroll = scroll.saturating_add(1),
            NavAction::PageUp(n) => *scroll = scroll.saturating_sub(n),
            NavAction::PageDown(n) => *scroll = scroll.saturating_add(n),
            NavAction::Home => *scroll = 0,
            NavAction::End => {} // no-op for detail popups
        },
        _ => match state.current_tab {
            Tab::Processes => match action {
                NavAction::Up => state.process_table.select_up(),
                NavAction::Down => state.process_table.select_down(),
                NavAction::PageUp(n) => state.process_table.page_up(n),
                NavAction::PageDown(n) => state.process_table.page_down(n),
                NavAction::Home => state.process_table.selected = 0,
                NavAction::End => {
                    let len = state.process_table.filtered_items().len();
                    if len > 0 {
                        state.process_table.selected = len - 1;
                    }
                }
            },
            _ => {
                let nav: &mut dyn NavigableTable = match state.current_tab {
                    Tab::PostgresActive => &mut state.pga,
                    Tab::PgStatements => &mut state.pgs,
                    Tab::PgStorePlans => &mut state.pgp,
                    Tab::PgTables => &mut state.pgt,
                    Tab::PgIndexes => &mut state.pgi,
                    Tab::PgErrors => &mut state.pge,
                    Tab::PgLocks => &mut state.pgl,
                    Tab::Processes => unreachable!(),
                };
                match action {
                    NavAction::Up => nav.select_up(),
                    NavAction::Down => nav.select_down(),
                    NavAction::PageUp(n) => nav.page_up(n),
                    NavAction::PageDown(n) => nav.page_down(n),
                    NavAction::Home => nav.home(),
                    NavAction::End => nav.end(),
                }
            }
        },
    }
}

/// Handles key input and updates state.
pub fn handle_key(state: &mut AppState, key: KeyEvent) -> KeyAction {
    if matches!(state.popup, super::state::PopupState::QuitConfirm) {
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
            state.popup = super::state::PopupState::None;
            KeyAction::Quit
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.popup = super::state::PopupState::None;
            KeyAction::Quit
        }
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            state.popup = super::state::PopupState::None;
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
            state.popup = super::state::PopupState::QuitConfirm;
            KeyAction::None
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,

        // Tab navigation (blocked when a detail popup is open)
        KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Char('1')
        | KeyCode::Char('2')
        | KeyCode::Char('3')
        | KeyCode::Char('4')
        | KeyCode::Char('5')
        | KeyCode::Char('6')
        | KeyCode::Char('7')
        | KeyCode::Char('8')
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
        KeyCode::Char('4') => {
            state.switch_tab(Tab::PgStorePlans);
            KeyAction::None
        }
        KeyCode::Char('5') => {
            state.switch_tab(Tab::PgTables);
            KeyAction::None
        }
        KeyCode::Char('6') => {
            state.switch_tab(Tab::PgIndexes);
            KeyAction::None
        }
        KeyCode::Char('7') => {
            state.switch_tab(Tab::PgErrors);
            KeyAction::None
        }
        KeyCode::Char('8') => {
            state.switch_tab(Tab::PgLocks);
            KeyAction::None
        }

        // Row navigation (or popup scroll if popup is open)
        KeyCode::Up | KeyCode::Char('k') => {
            dispatch_navigation(state, NavAction::Up);
            KeyAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            dispatch_navigation(state, NavAction::Down);
            KeyAction::None
        }
        KeyCode::PageUp => {
            dispatch_navigation(state, NavAction::PageUp(20));
            KeyAction::None
        }
        KeyCode::PageDown => {
            dispatch_navigation(state, NavAction::PageDown(20));
            KeyAction::None
        }
        KeyCode::Home => {
            dispatch_navigation(state, NavAction::Home);
            KeyAction::None
        }
        KeyCode::End => {
            dispatch_navigation(state, NavAction::End);
            KeyAction::None
        }

        // Sorting
        KeyCode::Char('s') | KeyCode::Char('S') => {
            match state.current_tab {
                Tab::Processes => state.next_process_sort_column(),
                Tab::PostgresActive => state.pga.next_sort_column(),
                Tab::PgStatements => state.pgs.next_sort_column(),
                Tab::PgStorePlans => state.pgp.next_sort_column(),
                Tab::PgTables => state.pgt.next_sort_column(),
                Tab::PgIndexes => state.pgi.next_sort_column(),
                Tab::PgErrors => state.pge.next_sort_column(),
                Tab::PgLocks => {} // tree order, no sorting
            }
            KeyAction::None
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            match state.current_tab {
                Tab::Processes => state.toggle_process_sort_direction(),
                Tab::PostgresActive => state.pga.toggle_sort_direction(),
                Tab::PgStatements => state.pgs.toggle_sort_direction(),
                Tab::PgStorePlans => state.pgp.toggle_sort_direction(),
                Tab::PgTables => state.pgt.toggle_sort_direction(),
                Tab::PgIndexes => state.pgi.toggle_sort_direction(),
                Tab::PgErrors => state.pge.toggle_sort_direction(),
                Tab::PgLocks => {} // tree order, no sorting
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
                state.pgs.view_mode = super::state::PgStatementsViewMode::Time;
                state.pgs.selected = 0;
                state.pgs.sort_column =
                    super::state::PgStatementsViewMode::Time.default_sort_column();
                state.pgs.sort_ascending = false;
                KeyAction::None
            } else if state.current_tab == Tab::PgStorePlans {
                state.pgp.view_mode = super::state::PgStorePlansViewMode::Time;
                state.pgp.selected = 0;
                state.pgp.sort_column =
                    super::state::PgStorePlansViewMode::Time.default_sort_column();
                state.pgp.sort_ascending = false;
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
                state.pga.view_mode = super::state::PgActivityViewMode::Generic;
                state.pga.sort_column =
                    super::state::PgActivityViewMode::Generic.default_sort_column();
                state.pga.sort_ascending = false;
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
                state.pgs.view_mode = super::state::PgStatementsViewMode::Calls;
                state.pgs.selected = 0;
                state.pgs.sort_column =
                    super::state::PgStatementsViewMode::Calls.default_sort_column();
                state.pgs.sort_ascending = false;
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
                state.pga.view_mode = match state.pga.view_mode {
                    super::state::PgActivityViewMode::Generic => {
                        super::state::PgActivityViewMode::Stats
                    }
                    super::state::PgActivityViewMode::Stats => {
                        super::state::PgActivityViewMode::Generic
                    }
                };
                state.pga.sort_column = state.pga.view_mode.default_sort_column();
                state.pga.sort_ascending = false;
            }
            KeyAction::None
        }

        // PGA hide idle / PGS IO view
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if state.current_tab == Tab::PostgresActive {
                state.pga.hide_idle = !state.pga.hide_idle;
                state.pga.selected = 0; // Reset selection when toggling
            } else if state.current_tab == Tab::PgStatements {
                state.pgs.view_mode = super::state::PgStatementsViewMode::Io;
                state.pgs.selected = 0;
                state.pgs.sort_column =
                    super::state::PgStatementsViewMode::Io.default_sort_column();
                state.pgs.sort_ascending = false;
            } else if state.current_tab == Tab::PgStorePlans {
                state.pgp.view_mode = super::state::PgStorePlansViewMode::Io;
                state.pgp.selected = 0;
                state.pgp.sort_column =
                    super::state::PgStorePlansViewMode::Io.default_sort_column();
                state.pgp.sort_ascending = false;
            } else if state.current_tab == Tab::PgTables {
                state.pgt.view_mode = super::state::PgTablesViewMode::Io;
                state.pgt.selected = 0;
                state.pgt.sort_column = super::state::PgTablesViewMode::Io.default_sort_column();
                state.pgt.sort_ascending = false;
            } else if state.current_tab == Tab::PgIndexes {
                state.pgi.view_mode = super::state::PgIndexesViewMode::Io;
                state.pgi.selected = 0;
                state.pgi.sort_column = super::state::PgIndexesViewMode::Io.default_sort_column();
                state.pgi.sort_ascending = false;
            }
            KeyAction::None
        }

        // PGS Temp view
        KeyCode::Char('e') | KeyCode::Char('E') => {
            if state.current_tab == Tab::PgStatements {
                state.pgs.view_mode = super::state::PgStatementsViewMode::Temp;
                state.pgs.selected = 0;
                state.pgs.sort_column =
                    super::state::PgStatementsViewMode::Temp.default_sort_column();
                state.pgs.sort_ascending = false;
            } else if state.current_tab == Tab::PgStorePlans {
                state.pgp.view_mode = super::state::PgStorePlansViewMode::Regression;
                state.pgp.selected = 0;
                state.pgp.sort_column =
                    super::state::PgStorePlansViewMode::Regression.default_sort_column();
                state.pgp.sort_ascending = false;
            }
            KeyAction::None
        }

        // PGT view modes: a=Reads, w=Writes, x=Scans, n=Maintenance
        KeyCode::Char('a') | KeyCode::Char('A') => {
            if state.current_tab == Tab::PgTables {
                state.pgt.view_mode = super::state::PgTablesViewMode::Reads;
                state.pgt.selected = 0;
                state.pgt.sort_column = super::state::PgTablesViewMode::Reads.default_sort_column();
                state.pgt.sort_ascending = false;
            }
            KeyAction::None
        }
        KeyCode::Char('x') | KeyCode::Char('X') => {
            if state.current_tab == Tab::PostgresActive {
                state.pga.hide_system = !state.pga.hide_system;
                state.pga.selected = 0;
            } else if state.current_tab == Tab::PgTables {
                state.pgt.view_mode = super::state::PgTablesViewMode::Scans;
                state.pgt.selected = 0;
                state.pgt.sort_column = super::state::PgTablesViewMode::Scans.default_sort_column();
                state.pgt.sort_ascending = false;
            }
            KeyAction::None
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            if state.current_tab == Tab::PgTables {
                state.pgt.view_mode = super::state::PgTablesViewMode::Maintenance;
                state.pgt.selected = 0;
                state.pgt.sort_column =
                    super::state::PgTablesViewMode::Maintenance.default_sort_column();
                state.pgt.sort_ascending = false;
            }
            KeyAction::None
        }

        // PGI view modes: u=Usage, w=Unused
        KeyCode::Char('u') | KeyCode::Char('U') => {
            if state.current_tab == Tab::PgIndexes {
                state.pgi.view_mode = super::state::PgIndexesViewMode::Usage;
                state.pgi.selected = 0;
                state.pgi.sort_column =
                    super::state::PgIndexesViewMode::Usage.default_sort_column();
                state.pgi.sort_ascending = false;
            }
            KeyAction::None
        }
        KeyCode::Char('w') | KeyCode::Char('W') => {
            if state.current_tab == Tab::PgTables {
                state.pgt.view_mode = super::state::PgTablesViewMode::Writes;
                state.pgt.selected = 0;
                state.pgt.sort_column =
                    super::state::PgTablesViewMode::Writes.default_sort_column();
                state.pgt.sort_ascending = false;
            } else if state.current_tab == Tab::PgIndexes {
                state.pgi.view_mode = super::state::PgIndexesViewMode::Unused;
                state.pgi.selected = 0;
                state.pgi.sort_column =
                    super::state::PgIndexesViewMode::Unused.default_sort_column();
                state.pgi.sort_ascending =
                    super::state::PgIndexesViewMode::Unused.default_sort_ascending();
            }
            KeyAction::None
        }

        // Help: toggle inline help in detail popups, or open/close global help
        KeyCode::Char('?') | KeyCode::Char('H') => {
            match &mut state.popup {
                PopupState::ProcessDetail { show_help, .. }
                | PopupState::PgDetail { show_help, .. }
                | PopupState::PgsDetail { show_help, .. }
                | PopupState::PgpDetail { show_help, .. }
                | PopupState::PgtDetail { show_help, .. }
                | PopupState::PgiDetail { show_help, .. }
                | PopupState::PgeDetail { show_help, .. }
                | PopupState::PglDetail { show_help, .. } => {
                    *show_help = !*show_help;
                }
                PopupState::Help { .. } => {
                    state.popup = PopupState::None;
                }
                _ => {
                    state.popup = PopupState::Help { scroll: 0 };
                }
            }
            KeyAction::None
        }

        // Debug popup (collector timing, rates state) - live mode only
        KeyCode::Char('!') => {
            if state.is_live {
                state.popup = match state.popup {
                    PopupState::Debug => PopupState::None,
                    _ => PopupState::Debug,
                };
            }
            KeyAction::None
        }

        // Detail popup (Enter on PRC or PGA tab)
        KeyCode::Enter => {
            if state.current_tab == Tab::Processes && !state.process_table.items.is_empty() {
                state.popup = match state.popup {
                    PopupState::ProcessDetail { .. } => PopupState::None,
                    _ => {
                        if let Some(tid) = state.process_table.tracked_id {
                            PopupState::ProcessDetail {
                                pid: tid as u32,
                                scroll: 0,
                                show_help: false,
                            }
                        } else {
                            PopupState::None
                        }
                    }
                };
            } else if state.current_tab == Tab::PostgresActive {
                state.popup = match state.popup {
                    PopupState::PgDetail { .. } => PopupState::None,
                    _ => {
                        if let Some(pid) = state.pga.tracked_pid {
                            PopupState::PgDetail {
                                pid,
                                scroll: 0,
                                show_help: false,
                            }
                        } else {
                            PopupState::None
                        }
                    }
                };
            } else if state.current_tab == Tab::PgStatements {
                state.popup = match state.popup {
                    PopupState::PgsDetail { .. } => PopupState::None,
                    _ => {
                        if let Some(queryid) = state.pgs.tracked_queryid {
                            PopupState::PgsDetail {
                                queryid,
                                scroll: 0,
                                show_help: false,
                            }
                        } else {
                            PopupState::None
                        }
                    }
                };
            } else if state.current_tab == Tab::PgStorePlans {
                state.popup = match state.popup {
                    PopupState::PgpDetail { .. } => PopupState::None,
                    _ => {
                        if let Some(planid) = state.pgp.tracked_planid {
                            PopupState::PgpDetail {
                                planid,
                                scroll: 0,
                                show_help: false,
                            }
                        } else {
                            PopupState::None
                        }
                    }
                };
            } else if state.current_tab == Tab::PgTables {
                state.popup = match state.popup {
                    PopupState::PgtDetail { .. } => PopupState::None,
                    _ => {
                        if let Some(relid) = state.pgt.tracked_relid {
                            PopupState::PgtDetail {
                                relid,
                                scroll: 0,
                                show_help: false,
                            }
                        } else {
                            PopupState::None
                        }
                    }
                };
            } else if state.current_tab == Tab::PgIndexes {
                state.popup = match state.popup {
                    PopupState::PgiDetail { .. } => PopupState::None,
                    _ => {
                        if let Some(indexrelid) = state.pgi.tracked_indexrelid {
                            PopupState::PgiDetail {
                                indexrelid,
                                scroll: 0,
                                show_help: false,
                            }
                        } else {
                            PopupState::None
                        }
                    }
                };
            } else if state.current_tab == Tab::PgErrors {
                state.popup = match state.popup {
                    PopupState::PgeDetail { .. } => PopupState::None,
                    _ => {
                        if let Some(pattern_hash) = state.pge.tracked_pattern_hash {
                            PopupState::PgeDetail {
                                pattern_hash,
                                scroll: 0,
                                show_help: false,
                            }
                        } else {
                            PopupState::None
                        }
                    }
                };
            } else if state.current_tab == Tab::PgLocks {
                state.popup = match state.popup {
                    PopupState::PglDetail { .. } => PopupState::None,
                    _ => {
                        if let Some(pid) = state.pgl.tracked_pid {
                            PopupState::PglDetail {
                                pid,
                                scroll: 0,
                                show_help: false,
                            }
                        } else {
                            PopupState::None
                        }
                    }
                };
            }
            KeyAction::None
        }

        // Drill-down navigation (PRC -> PGA -> PGS, PGT -> PGI)
        KeyCode::Char('>') | KeyCode::Char('J') => {
            if state.popup.is_open() {
                state.status_message = Some("Close popup (Esc) before drill-down".to_string());
            } else if state.current_tab == Tab::Processes
                || state.current_tab == Tab::PostgresActive
                || state.current_tab == Tab::PgTables
                || state.current_tab == Tab::PgLocks
            {
                state.drill_down_requested = true;
            }
            KeyAction::None
        }

        // Close popups with Escape
        KeyCode::Esc => {
            state.status_message = None;
            if state.popup.is_open() {
                state.popup = PopupState::None;
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
        assert_eq!(state.pgs.view_mode, PgStatementsViewMode::Calls);
        assert_eq!(state.pgs.sort_column, 0);
        assert!(!state.pgs.sort_ascending);

        let _ = handle_key(&mut state, key(KeyCode::Char('i')));
        assert_eq!(state.pgs.view_mode, PgStatementsViewMode::Io);
        assert_eq!(state.pgs.sort_column, 1);

        let _ = handle_key(&mut state, key(KeyCode::Char('e')));
        assert_eq!(state.pgs.view_mode, PgStatementsViewMode::Temp);
        assert_eq!(state.pgs.sort_column, 3);

        let _ = handle_key(&mut state, key(KeyCode::Char('t')));
        assert_eq!(state.pgs.view_mode, PgStatementsViewMode::Time);
        assert_eq!(state.pgs.sort_column, 1);
    }

    #[test]
    fn filter_mode_applies_to_pgs_filter() {
        let mut state = AppState::new(true);
        state.current_tab = Tab::PgStatements;

        // Enter filter mode
        let _ = handle_key(&mut state, key(KeyCode::Char('/')));
        assert_eq!(state.input_mode, InputMode::Filter);
        assert_eq!(state.pgs.filter, None);

        // Type a filter string
        let _ = handle_key(&mut state, key(KeyCode::Char('a')));
        assert_eq!(state.pgs.filter.as_deref(), Some("a"));

        // Cancel
        let _ = handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.pgs.filter, None);
    }

    #[test]
    fn quit_requires_confirmation_and_quits_on_qq() {
        let mut state = AppState::new(true);

        let action = handle_key(&mut state, key(KeyCode::Char('q')));
        assert_eq!(action, KeyAction::None);
        assert!(matches!(state.popup, PopupState::QuitConfirm));

        let action = handle_key(&mut state, key(KeyCode::Char('q')));
        assert_eq!(action, KeyAction::Quit);
        assert!(matches!(state.popup, PopupState::None));
    }

    #[test]
    fn quit_confirmation_cancels_on_esc() {
        let mut state = AppState::new(true);

        let _ = handle_key(&mut state, key(KeyCode::Char('q')));
        assert!(matches!(state.popup, PopupState::QuitConfirm));

        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(action, KeyAction::None);
        assert!(matches!(state.popup, PopupState::None));
    }

    #[test]
    fn quit_confirmation_quits_on_enter() {
        let mut state = AppState::new(true);

        let _ = handle_key(&mut state, key(KeyCode::Char('q')));
        assert!(matches!(state.popup, PopupState::QuitConfirm));

        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(action, KeyAction::Quit);
        assert!(matches!(state.popup, PopupState::None));
    }

    #[test]
    fn tab_switch_blocked_when_popup_open() {
        let mut state = AppState::new(true);
        state.popup = PopupState::ProcessDetail {
            pid: 1,
            scroll: 0,
            show_help: false,
        };

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
        assert!(matches!(state.popup, PopupState::None));
        assert!(state.status_message.is_none());

        let _ = handle_key(&mut state, key(KeyCode::Char('2')));
        assert_eq!(state.current_tab, Tab::PostgresActive);
    }

    #[test]
    fn drill_down_blocked_when_popup_open() {
        let mut state = AppState::new(true);
        state.popup = PopupState::ProcessDetail {
            pid: 1,
            scroll: 0,
            show_help: false,
        };

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
                Tab::PostgresActive => state.pga.filter = None,
                Tab::PgStatements => state.pgs.filter = None,
                Tab::PgStorePlans => state.pgp.filter = None,
                Tab::PgTables => state.pgt.filter = None,
                Tab::PgIndexes => state.pgi.filter = None,
                Tab::PgErrors => state.pge.filter = None,
                Tab::PgLocks => state.pgl.filter = None,
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
        Tab::PostgresActive => state.pga.filter = filter,
        Tab::PgStatements => state.pgs.filter = filter,
        Tab::PgStorePlans => state.pgp.filter = filter,
        Tab::PgTables => state.pgt.filter = filter,
        Tab::PgIndexes => state.pgi.filter = filter,
        Tab::PgErrors => state.pge.filter = filter,
        Tab::PgLocks => state.pgl.filter = filter,
    }
}
