//! Main application state.

use ratatui::widgets::TableState as RatatuiTableState;
use std::collections::HashMap;

use crate::storage::Snapshot;

use super::{
    CachedWidths, InputMode, PgActivityTabState, PgIndexesTabState, PgLocksTabState,
    PgStatementsTabState, PgTablesTabState, PopupState, ProcessRow, ProcessViewMode, Tab, TabState,
    TableState,
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
    /// Disk filter (for Disk tab).
    pub disk_filter: Option<String>,
    /// Disk sort column index.
    pub disk_sort_column: usize,
    /// Disk sort direction.
    pub disk_sort_ascending: bool,
    /// Network filter (for Network tab).
    pub net_filter: Option<String>,
    /// Network sort column index.
    pub net_sort_column: usize,
    /// Network sort direction.
    pub net_sort_ascending: bool,
    /// Active popup state. Only one popup can be open at a time.
    pub popup: PopupState,
    /// Per-tab state for filter, sort, and selection.
    pub tab_states: HashMap<Tab, TabState>,
    /// PostgreSQL Activity (PGA) tab state.
    pub pga: PgActivityTabState,
    /// Flag set when user requests drill-down navigation (>/J keys).
    /// Cleared after processing by app.rs.
    pub drill_down_requested: bool,

    // ===== pg_stat_statements (PGS tab) =====
    /// pg_stat_statements (PGS) tab state.
    pub pgs: PgStatementsTabState,

    // ===== pg_stat_user_tables (PGT tab) =====
    /// pg_stat_user_tables (PGT) tab state.
    pub pgt: PgTablesTabState,

    // ===== pg_stat_user_indexes (PGI tab) =====
    /// pg_stat_user_indexes (PGI) tab state.
    pub pgi: PgIndexesTabState,

    // ===== pg_locks tree (PGL tab) =====
    /// pg_locks tree (PGL) tab state.
    pub pgl: PgLocksTabState,

    // ===== Status message =====
    /// Temporary status message shown in the header (e.g., why an action was blocked).
    pub status_message: Option<String>,

    // ===== Ratatui TableState for scrolling =====
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
            disk_filter: None,
            disk_sort_column: 0,
            disk_sort_ascending: true,
            net_filter: None,
            net_sort_column: 0,
            net_sort_ascending: true,
            popup: PopupState::None,
            tab_states: HashMap::new(),
            pga: PgActivityTabState::default(),
            drill_down_requested: false,
            pgs: PgStatementsTabState::default(),
            pgt: PgTablesTabState::default(),
            pgi: PgIndexesTabState::default(),
            pgl: PgLocksTabState::default(),

            status_message: None,

            prc_ratatui_state: RatatuiTableState::default(),

            popup_was_open: false,
        }
    }

    /// Saves the current tab state before switching.
    pub fn save_current_tab_state(&mut self) {
        let state = match self.current_tab {
            Tab::Processes => TabState {
                filter: self.process_table.filter.clone(),
                sort_column: self.process_table.sort_column,
                sort_ascending: self.process_table.sort_ascending,
                selected: self.process_table.selected,
            },
            Tab::PostgresActive => TabState {
                filter: self.pga.filter.clone(),
                sort_column: self.pga.sort_column,
                sort_ascending: self.pga.sort_ascending,
                selected: self.pga.selected,
            },
            Tab::PgStatements => TabState {
                filter: self.pgs.filter.clone(),
                sort_column: self.pgs.sort_column,
                sort_ascending: self.pgs.sort_ascending,
                selected: self.pgs.selected,
            },
            Tab::PgTables => TabState {
                filter: self.pgt.filter.clone(),
                sort_column: self.pgt.sort_column,
                sort_ascending: self.pgt.sort_ascending,
                selected: self.pgt.selected,
            },
            Tab::PgIndexes => TabState {
                filter: self.pgi.filter.clone(),
                sort_column: self.pgi.sort_column,
                sort_ascending: self.pgi.sort_ascending,
                selected: self.pgi.selected,
            },
            Tab::PgLocks => TabState {
                filter: self.pgl.filter.clone(),
                sort_column: 0,
                sort_ascending: false,
                selected: self.pgl.selected,
            },
        };
        self.tab_states.insert(self.current_tab, state);
    }

    /// Restores tab state for the given tab.
    pub fn restore_tab_state(&mut self, tab: Tab) {
        if let Some(state) = self.tab_states.get(&tab) {
            match tab {
                Tab::Processes => {
                    self.process_table.filter = state.filter.clone();
                    self.filter_input = state.filter.clone().unwrap_or_default();
                    self.process_table.sort_column = state.sort_column;
                    self.process_table.sort_ascending = state.sort_ascending;
                    self.process_table.selected = state.selected;
                }
                Tab::PostgresActive => {
                    self.pga.filter = state.filter.clone();
                    self.filter_input = state.filter.clone().unwrap_or_default();
                    self.pga.sort_column = state.sort_column;
                    self.pga.sort_ascending = state.sort_ascending;
                    self.pga.selected = state.selected;
                }
                Tab::PgStatements => {
                    self.pgs.filter = state.filter.clone();
                    self.filter_input = state.filter.clone().unwrap_or_default();
                    self.pgs.sort_column = state.sort_column;
                    self.pgs.sort_ascending = state.sort_ascending;
                    self.pgs.selected = state.selected;
                }
                Tab::PgTables => {
                    self.pgt.filter = state.filter.clone();
                    self.filter_input = state.filter.clone().unwrap_or_default();
                    self.pgt.sort_column = state.sort_column;
                    self.pgt.sort_ascending = state.sort_ascending;
                    self.pgt.selected = state.selected;
                }
                Tab::PgIndexes => {
                    self.pgi.filter = state.filter.clone();
                    self.filter_input = state.filter.clone().unwrap_or_default();
                    self.pgi.sort_column = state.sort_column;
                    self.pgi.sort_ascending = state.sort_ascending;
                    self.pgi.selected = state.selected;
                }
                Tab::PgLocks => {
                    self.pgl.filter = state.filter.clone();
                    self.filter_input = state.filter.clone().unwrap_or_default();
                    self.pgl.selected = state.selected;
                }
            }
        }
    }

    /// Returns true if any detail popup is currently open.
    pub fn any_popup_open(&self) -> bool {
        self.popup.is_detail_open()
    }

    /// Switches to a new tab, saving current and restoring target state.
    pub fn switch_tab(&mut self, new_tab: Tab) {
        if self.current_tab != new_tab {
            match self.current_tab {
                Tab::PostgresActive => {
                    self.pga.tracked_pid = None;
                }
                Tab::PgStatements => {
                    self.pgs.tracked_queryid = None;
                }
                Tab::PgTables => {
                    self.pgt.tracked_relid = None;
                }
                Tab::PgIndexes => {
                    self.pgi.tracked_indexrelid = None;
                    self.pgi.filter_relid = None; // clear drill-down filter on manual tab switch
                }
                Tab::PgLocks => {
                    self.pgl.tracked_pid = None;
                }
                Tab::Processes => {}
            }
            self.save_current_tab_state();
            self.current_tab = new_tab;
            self.restore_tab_state(new_tab);
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

    /// Cycles to next sort column for disk tab.
    pub fn next_disk_sort_column(&mut self) {
        // 10 columns: Device r/s rMB/s rrqm/s r_await w/s wMB/s wrqm/s w_await %util
        self.disk_sort_column = (self.disk_sort_column + 1) % 10;
    }

    /// Toggles sort direction for disk tab.
    pub fn toggle_disk_sort_direction(&mut self) {
        self.disk_sort_ascending = !self.disk_sort_ascending;
    }

    /// Cycles to next sort column for network tab.
    pub fn next_net_sort_column(&mut self) {
        // 9 columns: Interface rxMB/s rxPkt/s rxErr/s rxDrp/s txMB/s txPkt/s txErr/s txDrp/s
        self.net_sort_column = (self.net_sort_column + 1) % 9;
    }

    /// Toggles sort direction for network tab.
    pub fn toggle_net_sort_direction(&mut self) {
        self.net_sort_ascending = !self.net_sort_ascending;
    }
}
