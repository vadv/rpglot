//! Main application state.

use ratatui::widgets::TableState as RatatuiTableState;
use std::collections::HashMap;

use crate::storage::Snapshot;

use super::{
    CachedWidths, InputMode, PgActivityTabState, PgErrorsTabState, PgIndexesTabState,
    PgLocksTabState, PgStatementsTabState, PgStorePlansTabState, PgTablesTabState, PopupState,
    ProcessRow, ProcessViewMode, Tab, TableState,
};

/// Main application state.
#[derive(Debug)]
pub struct AppState {
    /// Current active tab.
    pub current_tab: Tab,
    /// Input mode.
    pub input_mode: InputMode,
    /// Filter input buffer.
    pub filter_input: String,
    /// Time jump input buffer (history mode, `b`).
    pub time_jump_input: String,
    /// Last time jump parse/seek error to display in popup.
    pub time_jump_error: Option<String>,
    /// Process table state.
    pub process_table: TableState<ProcessRow>,
    /// Current snapshot.
    pub current_snapshot: Option<Snapshot>,
    /// Previous snapshot for diff.
    pub previous_snapshot: Option<Snapshot>,
    /// Paused state (for history mode).
    pub paused: bool,
    /// History position info (current/total).
    pub history_position: Option<(usize, usize)>,
    /// Is live mode.
    pub is_live: bool,
    /// Process view mode (g/c/m keys).
    pub process_view_mode: ProcessViewMode,
    /// Previous memory values for VGROW/RGROW calculation: pid -> (vsize, rsize).
    pub prev_process_mem: HashMap<u32, (u64, u64)>,
    /// Previous CPU values for CPU% calculation: pid -> (utime, stime).
    pub prev_process_cpu: HashMap<u32, (u64, u64)>,
    /// Previous disk I/O values for rate calculation: pid -> (rsz, wsz, cwsz).
    pub prev_process_dsk: HashMap<u32, (u64, u64, u64)>,
    /// Previous total system CPU time for CPU% normalization.
    pub prev_total_cpu_time: Option<u64>,
    /// Horizontal scroll offset for wide tables.
    pub horizontal_scroll: usize,
    /// Cached column widths (calculated on first snapshot).
    pub cached_widths: Option<CachedWidths>,
    /// Terminal width for cache invalidation on resize.
    pub terminal_width: u16,
    /// Active popup state. Only one popup can be open at a time.
    pub popup: PopupState,
    /// PostgreSQL Activity (PGA) tab state.
    pub pga: PgActivityTabState,
    /// Flag set when user requests drill-down navigation (>/J keys).
    /// Cleared after processing by app.rs.
    pub drill_down_requested: bool,
    /// pg_stat_statements (PGS) tab state.
    pub pgs: PgStatementsTabState,
    /// pg_store_plans (PGP) tab state.
    pub pgp: PgStorePlansTabState,
    /// pg_stat_user_tables (PGT) tab state.
    pub pgt: PgTablesTabState,
    /// pg_stat_user_indexes (PGI) tab state.
    pub pgi: PgIndexesTabState,
    /// PostgreSQL log errors (PGE) tab state.
    pub pge: PgErrorsTabState,
    /// pg_locks tree (PGL) tab state.
    pub pgl: PgLocksTabState,
    /// Temporary status message shown in the header (e.g., why an action was blocked).
    pub status_message: Option<String>,
    /// Ratatui table state for PRC tab (enables auto-scrolling).
    pub prc_ratatui_state: RatatuiTableState,
    /// Whether a popup was open on the previous frame.
    /// Used to force full redraw when popup closes.
    pub popup_was_open: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(true)
    }
}

impl AppState {
    pub fn new(is_live: bool) -> Self {
        Self {
            current_tab: Tab::Processes,
            input_mode: InputMode::Normal,
            filter_input: String::new(),
            time_jump_input: String::new(),
            time_jump_error: None,
            process_table: TableState::new(),
            current_snapshot: None,
            previous_snapshot: None,
            paused: false,
            history_position: None,
            is_live,
            process_view_mode: ProcessViewMode::Generic,
            prev_process_mem: HashMap::new(),
            prev_process_cpu: HashMap::new(),
            prev_process_dsk: HashMap::new(),
            prev_total_cpu_time: None,
            horizontal_scroll: 0,
            cached_widths: None,
            terminal_width: 0,
            popup: PopupState::None,
            pga: PgActivityTabState::default(),
            drill_down_requested: false,
            pgs: PgStatementsTabState::default(),
            pgp: PgStorePlansTabState::default(),
            pgt: PgTablesTabState::default(),
            pgi: PgIndexesTabState::default(),
            pge: PgErrorsTabState::default(),
            pgl: PgLocksTabState::default(),
            status_message: None,
            prc_ratatui_state: RatatuiTableState::default(),
            popup_was_open: false,
        }
    }

    /// Returns true if any detail popup is currently open.
    pub fn any_popup_open(&self) -> bool {
        self.popup.is_detail_open()
    }

    /// Returns the filter string for the current tab.
    pub fn get_current_filter(&self) -> Option<String> {
        match self.current_tab {
            Tab::Processes => self.process_table.filter.clone(),
            Tab::PostgresActive => self.pga.filter.clone(),
            Tab::PgStatements => self.pgs.filter.clone(),
            Tab::PgStorePlans => self.pgp.filter.clone(),
            Tab::PgTables => self.pgt.filter.clone(),
            Tab::PgIndexes => self.pgi.filter.clone(),
            Tab::PgErrors => self.pge.filter.clone(),
            Tab::PgLocks => self.pgl.filter.clone(),
        }
    }

    /// Switches to a new tab, clearing tracked entities on the old tab
    /// and syncing the filter input buffer from the new tab's filter.
    pub fn switch_tab(&mut self, new_tab: Tab) {
        if self.current_tab != new_tab {
            match self.current_tab {
                Tab::PostgresActive => {
                    self.pga.tracked_pid = None;
                }
                Tab::PgStatements => {
                    self.pgs.tracked_queryid = None;
                }
                Tab::PgStorePlans => {
                    self.pgp.tracked_planid = None;
                }
                Tab::PgTables => {
                    self.pgt.tracked_relid = None;
                }
                Tab::PgIndexes => {
                    self.pgi.tracked_indexrelid = None;
                    self.pgi.filter_relid = None;
                }
                Tab::PgErrors => {
                    self.pge.tracked_pattern_hash = None;
                }
                Tab::PgLocks => {
                    self.pgl.tracked_pid = None;
                }
                Tab::Processes => {}
            }
            self.current_tab = new_tab;
            // Sync filter_input from the new tab's filter
            self.filter_input = self.get_current_filter().unwrap_or_default();
        }
    }

    /// Cycles to next sort column for the current view mode.
    pub fn next_process_sort_column(&mut self) {
        let column_count = ProcessRow::headers_for_mode(self.process_view_mode).len();
        self.process_table.sort_column = (self.process_table.sort_column + 1) % column_count;
        self.apply_process_sort();
    }

    /// Toggles sort direction for the current view mode.
    pub fn toggle_process_sort_direction(&mut self) {
        self.process_table.sort_ascending = !self.process_table.sort_ascending;
        self.apply_process_sort();
    }

    /// Applies sort to process table using the current view mode.
    pub fn apply_process_sort(&mut self) {
        let col = self.process_table.sort_column;
        let asc = self.process_table.sort_ascending;
        let mode = self.process_view_mode;

        self.process_table.items.sort_by(|a, b| {
            let key_a = a.sort_key_for_mode(col, mode);
            let key_b = b.sort_key_for_mode(col, mode);
            let cmp = key_a
                .partial_cmp(&key_b)
                .unwrap_or(std::cmp::Ordering::Equal);
            if asc { cmp } else { cmp.reverse() }
        });
    }
}
