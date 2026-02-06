//! Main TUI application.

use std::io;
use std::time::Duration;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::provider::HistoryProvider;
use crate::provider::SnapshotProvider;
use crate::storage::model::Snapshot;
use crate::util::parse_time_with_base;

use super::event::{Event, EventHandler};
use super::input::{KeyAction, handle_key};
use super::render::render;
use super::state::{AppState, InputMode, Tab};
use super::widgets::processes::{
    calculate_cached_widths, extract_processes, get_total_cpu_time, get_total_memory,
    update_prev_cpu, update_prev_dsk, update_prev_mem,
};
use crate::storage::model::DataBlock;

fn elapsed_secs_between_timestamps(current_ts: i64, prev_ts: i64) -> f64 {
    match current_ts.checked_sub(prev_ts) {
        Some(delta) if delta > 0 => delta as f64,
        _ => 0.0,
    }
}

/// Main TUI application.
pub struct App {
    provider: Box<dyn SnapshotProvider>,
    state: AppState,
    should_quit: bool,
}

impl App {
    /// Creates a new App with the given provider.
    pub fn new(provider: Box<dyn SnapshotProvider>) -> Self {
        let is_live = provider.is_live();
        Self {
            provider,
            state: AppState::new(is_live),
            should_quit: false,
        }
    }

    /// Runs the TUI application.
    pub fn run(mut self, tick_rate: Duration) -> io::Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Create event handler
        let events = EventHandler::new(tick_rate);

        // Get initial terminal size for adaptive column widths
        if let Ok(size) = terminal.size() {
            self.state.terminal_width = size.width;
        }

        // Initial data fetch
        self.advance();

        // Main loop
        loop {
            // Draw UI
            let interner = self.provider.interner();
            let timing = self.provider.collector_timing();
            terminal.draw(|frame| render(frame, &mut self.state, interner, timing))?;

            // Handle events
            match events.next() {
                Ok(Event::Tick) => {
                    if !self.state.paused && self.state.is_live {
                        self.advance();
                    }
                }
                Ok(Event::Key(key)) => {
                    let action = handle_key(&mut self.state, key);
                    match action {
                        KeyAction::Quit => self.should_quit = true,
                        KeyAction::Advance => self.advance(),
                        KeyAction::Rewind => self.rewind(),
                        KeyAction::JumpToTime => self.jump_to_time(),
                        KeyAction::None => {}
                    }
                }
                Ok(Event::Resize(width, _)) => {
                    // Recalculate column widths on resize
                    if self.state.terminal_width != width {
                        self.state.terminal_width = width;
                        let items = &self.state.process_table.items;
                        if !items.is_empty() {
                            self.state.cached_widths = Some(calculate_cached_widths(items, width));
                        }
                    }
                }
                Err(_) => {
                    self.should_quit = true;
                }
            }

            // Handle drill-down navigation request
            if self.state.drill_down_requested {
                self.state.drill_down_requested = false;
                self.handle_drill_down();
            }

            if self.should_quit {
                break;
            }
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot) {
        // Get total memory for MEM% calculation
        let total_mem = get_total_memory(&snapshot);

        // Get total CPU time for CPU% calculation
        let current_total_cpu_time = get_total_cpu_time(&snapshot);

        // Calculate elapsed time between snapshots for rate calculation
        let elapsed_secs = self
            .state
            .previous_snapshot
            .as_ref()
            .map(|prev| elapsed_secs_between_timestamps(snapshot.timestamp, prev.timestamp))
            .unwrap_or(1.0);

        // Extract process rows with VGROW/RGROW and CPU% calculation
        let interner = self.provider.interner();
        let user_resolver = self.provider.user_resolver();
        let processes = extract_processes(
            &snapshot,
            interner,
            user_resolver,
            &self.state.prev_process_mem,
            &self.state.prev_process_cpu,
            &self.state.prev_process_dsk,
            self.state.prev_total_cpu_time,
            current_total_cpu_time,
            total_mem,
            elapsed_secs,
        );
        self.state.process_table.update(processes);
        self.state.apply_process_sort();

        // Calculate adaptive column widths on first snapshot
        if self.state.cached_widths.is_none() && self.state.terminal_width > 0 {
            let items = &self.state.process_table.items;
            if !items.is_empty() {
                self.state.cached_widths =
                    Some(calculate_cached_widths(items, self.state.terminal_width));
            }
        }

        // Update prev values for next delta calculation
        update_prev_mem(&snapshot, &mut self.state.prev_process_mem);
        update_prev_cpu(&snapshot, &mut self.state.prev_process_cpu);
        update_prev_dsk(&snapshot, &mut self.state.prev_process_dsk);
        self.state.prev_total_cpu_time = Some(current_total_cpu_time);

        // Update pg_stat_statements rates cache (used by PGS tab).
        self.state.update_pgs_rates_from_snapshot(&snapshot);

        // Update history position for non-live mode
        if !self.state.is_live
            && let Some(hp) = self.provider.as_any().and_then(|a| {
                a.downcast_ref::<crate::provider::HistoryProvider>()
                    .map(|h| (h.position(), h.len()))
            })
        {
            self.state.history_position = Some(hp);
        }

        self.state.current_snapshot = Some(snapshot);
    }

    /// Advances to next snapshot.
    fn advance(&mut self) {
        // Save previous snapshot for diff
        self.state.previous_snapshot = self.state.current_snapshot.take();

        // Update PostgreSQL error status (always, even if snapshot fails)
        self.state.pg_last_error = self.provider.pg_last_error().map(|s| s.to_string());

        let snapshot = self.provider.advance().cloned();
        if let Some(snapshot) = snapshot {
            self.apply_snapshot(snapshot);
        }
    }

    /// Rewinds to previous snapshot (history mode only).
    fn rewind(&mut self) {
        if self.provider.can_rewind() {
            self.state.previous_snapshot = self.state.current_snapshot.take();

            let snapshot = self.provider.rewind().cloned();
            if let Some(snapshot) = snapshot {
                self.apply_snapshot(snapshot);
            }
        }
    }

    fn jump_to_time(&mut self) {
        if self.state.is_live {
            return;
        }

        // Keep PostgreSQL error status consistent with other navigation actions
        self.state.pg_last_error = self.provider.pg_last_error().map(|s| s.to_string());

        let input = self.state.time_jump_input.trim();
        if input.is_empty() {
            self.state.time_jump_error = Some("Empty input".to_string());
            return;
        }

        let base_ts = self
            .state
            .current_snapshot
            .as_ref()
            .map(|s| s.timestamp)
            .unwrap_or(0);

        let target_ts = match parse_time_with_base(input, base_ts) {
            Ok(ts) => ts,
            Err(e) => {
                self.state.time_jump_error = Some(e.to_string());
                return;
            }
        };

        let history = self
            .provider
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<HistoryProvider>());

        let Some(history) = history else {
            self.state.time_jump_error = Some("History provider is not available".to_string());
            return;
        };

        history.jump_to_timestamp_floor(target_ts);
        let pos = history.position();

        let current = history.current().cloned();
        let prev = pos
            .checked_sub(1)
            .and_then(|p| history.snapshot_at(p).cloned());

        // Reset the diff baseline to the snapshot before the target (if any).
        self.state.previous_snapshot = prev;
        self.state.current_snapshot = None;

        // Rebuild prev maps from the baseline snapshot so deltas are correct after jump.
        self.state.prev_process_mem.clear();
        self.state.prev_process_cpu.clear();
        self.state.prev_process_dsk.clear();
        self.state.prev_total_cpu_time = None;
        if let Some(prev_snapshot) = self.state.previous_snapshot.as_ref() {
            update_prev_mem(prev_snapshot, &mut self.state.prev_process_mem);
            update_prev_cpu(prev_snapshot, &mut self.state.prev_process_cpu);
            update_prev_dsk(prev_snapshot, &mut self.state.prev_process_dsk);
            self.state.prev_total_cpu_time = Some(get_total_cpu_time(prev_snapshot));
        }

        if let Some(snapshot) = current {
            self.apply_snapshot(snapshot);
            self.state.input_mode = InputMode::Normal;
            self.state.time_jump_error = None;
            self.state.time_jump_input.clear();
        } else {
            self.state.time_jump_error = Some("No snapshot at target".to_string());
        }
    }

    /// Handles drill-down navigation between tabs.
    /// PRC -> PGA: Navigate to PostgreSQL session by selected process PID.
    /// PGA -> PGS: Navigate to statement stats by query_id.
    fn handle_drill_down(&mut self) {
        let snapshot = match self.state.current_snapshot.as_ref() {
            Some(s) => s,
            None => return,
        };

        match self.state.current_tab {
            Tab::Processes => {
                // PRC -> PGA: Check if selected process is a PostgreSQL backend
                let selected = self.state.process_table.selected;
                if selected >= self.state.process_table.items.len() {
                    return;
                }
                let pid = self.state.process_table.items[selected].pid;

                // Check if this PID exists in pg_stat_activity
                let pg_backend_exists = snapshot.blocks.iter().any(|block| {
                    if let DataBlock::PgStatActivity(activities) = block {
                        activities.iter().any(|a| a.pid == pid as i32)
                    } else {
                        false
                    }
                });

                if pg_backend_exists {
                    // Switch to PGA and navigate to this PID (render will find and select the row)
                    self.state.current_tab = Tab::PostgresActive;
                    self.state.pga_navigate_to_pid = Some(pid as i32);
                }
                // If not a PostgreSQL backend, do nothing (could show message in future)
            }
            Tab::PostgresActive => {
                // PGA -> PGS: Get query_id from selected session using pg_detail_pid
                // (which is set by render based on pg_selected)
                let query_id = self.state.pg_detail_pid.and_then(|pid| {
                    snapshot
                        .blocks
                        .iter()
                        .filter_map(|block| {
                            if let DataBlock::PgStatActivity(activities) = block {
                                Some(activities.iter())
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .find(|a| a.pid == pid)
                        .map(|a| a.query_id)
                });

                if let Some(qid) = query_id
                    && qid != 0
                {
                    // Check if this query_id exists in pg_stat_statements
                    let pgs_exists = snapshot.blocks.iter().any(|block| {
                        if let DataBlock::PgStatStatements(statements) = block {
                            statements.iter().any(|s| s.queryid == qid)
                        } else {
                            false
                        }
                    });

                    if pgs_exists {
                        // Switch to PGS and navigate to this query_id (render will find and select the row)
                        self.state.current_tab = Tab::PgStatements;
                        self.state.pgs_navigate_to_queryid = Some(qid);
                    }
                }
                // If query_id is 0 or not found, do nothing
            }
            Tab::PgStatements => {
                // No further drill-down from PGS
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::elapsed_secs_between_timestamps;

    #[test]
    fn test_elapsed_secs_between_timestamps_uses_seconds() {
        // Snapshot.timestamp is seconds since epoch.
        assert_eq!(
            elapsed_secs_between_timestamps(1_700_000_000, 1_699_999_999),
            1.0
        );

        // saturating behavior
        assert_eq!(elapsed_secs_between_timestamps(100, 100), 0.0);
        assert_eq!(elapsed_secs_between_timestamps(99, 100), 0.0);
    }
}
