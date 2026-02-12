//! Application state management.

use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};
use ratatui::widgets::TableState as RatatuiTableState;
use std::collections::HashMap;

/// Available tabs in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tab {
    #[default]
    Processes,
    PostgresActive,
    PgStatements,
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[Tab::Processes, Tab::PostgresActive, Tab::PgStatements]
    }
}

/// Per-tab state for filter, sort, and selection.
#[derive(Debug, Clone, Default)]
pub struct TabState {
    /// Filter string.
    pub filter: Option<String>,
    /// Sort column index.
    pub sort_column: usize,
    /// Sort direction (true = ascending).
    pub sort_ascending: bool,
    /// Selected row index.
    pub selected: usize,
}

impl Tab {
    /// Returns the display name of the tab.
    pub fn name(&self) -> &'static str {
        match self {
            Tab::Processes => "PRC",
            Tab::PostgresActive => "PGA",
            Tab::PgStatements => "PGS",
        }
    }

    /// Returns the next tab.
    pub fn next(&self) -> Tab {
        match self {
            Tab::Processes => Tab::PostgresActive,
            Tab::PostgresActive => Tab::PgStatements,
            Tab::PgStatements => Tab::Processes,
        }
    }

    /// Returns the previous tab.
    pub fn prev(&self) -> Tab {
        match self {
            Tab::Processes => Tab::PgStatements,
            Tab::PostgresActive => Tab::Processes,
            Tab::PgStatements => Tab::PostgresActive,
        }
    }
}

/// Process table view mode (similar to atop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessViewMode {
    /// Generic view: PID SYSCPU USRCPU RDELAY VGROW RGROW RUID EUID ST EXC THR S CPUNR CPU CMD
    #[default]
    Generic,
    /// Command view: PID TID S MEM COMMAND-LINE
    Command,
    /// Memory view: PID TID MINFLT MAJFLT VSTEXT VSLIBS VDATA VSTACK LOCKSZ VSIZE RSIZE PSIZE VGROW RGROW SWAPSZ RUID EUID MEM CMD
    Memory,
    /// Disk I/O view: PID RDDSK WRDSK WCANCL DSK CMD
    Disk,
}

/// pg_stat_statements table view modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PgStatementsViewMode {
    /// Time view (timing-focused).
    #[default]
    Time,
    /// Calls view (frequency-focused).
    Calls,
    /// I/O view (buffer/cache focused).
    Io,
    /// Temp view (temp blocks / spill focused).
    Temp,
}

/// pg_stat_activity table view modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PgActivityViewMode {
    /// Generic view: PID, CPU%, RSS, DB, USER, STATE, WAIT, QDUR, XDUR, BDUR, BTYPE, QUERY
    #[default]
    Generic,
    /// Stats view: PID, DB, USER, STATE, QDUR, MEAN, MAX, CALL/s, HIT%, QUERY
    /// Shows pg_stat_statements metrics for the current query (linked by query_id).
    Stats,
}

/// Rate metrics for a single `pg_stat_statements` entry.
///
/// Rates are computed from deltas between two **real samples** of statement counters,
/// not between every TUI tick (collector may cache `pg_stat_statements` for ~30s).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PgStatementsRates {
    pub dt_secs: f64,
    pub calls_s: Option<f64>,
    pub rows_s: Option<f64>,
    /// Execution time rate in `ms/s`.
    pub exec_time_ms_s: Option<f64>,

    pub shared_blks_read_s: Option<f64>,
    pub shared_blks_hit_s: Option<f64>,
    pub shared_blks_dirtied_s: Option<f64>,
    pub shared_blks_written_s: Option<f64>,

    pub local_blks_read_s: Option<f64>,
    pub local_blks_written_s: Option<f64>,

    pub temp_blks_read_s: Option<f64>,
    pub temp_blks_written_s: Option<f64>,
    /// Temp I/O rate in `MB/s` (assumes 8 KiB blocks).
    pub temp_mb_s: Option<f64>,
}

/// Input mode for the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Filter,
    TimeJump,
}

/// Sort key types for table columns.
#[derive(Debug, Clone, PartialEq)]
pub enum SortKey {
    Integer(i64),
    Float(f64),
    String(String),
}

impl PartialOrd for SortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (SortKey::Integer(a), SortKey::Integer(b)) => a.partial_cmp(b),
            (SortKey::Float(a), SortKey::Float(b)) => a.partial_cmp(b),
            (SortKey::String(a), SortKey::String(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

/// Diff status for highlighting changes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DiffStatus {
    /// New item (green).
    New,
    /// Modified item with changed column indices (yellow).
    Modified(Vec<usize>),
    /// No changes.
    #[default]
    Unchanged,
}

/// Column type for adaptive width calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    /// Fixed width based on content (PID, S, THR).
    Fixed,
    /// Flexible width that adapts to content (RUID, EUID, CPU%).
    Flexible,
    /// Expandable column that takes remaining space (CMD, CMDLINE).
    Expandable,
}

/// Cached column widths for all view modes.
#[derive(Debug, Clone, Default)]
pub struct CachedWidths {
    /// Widths for Generic view mode.
    pub generic: Vec<u16>,
    /// Widths for Command view mode.
    pub command: Vec<u16>,
    /// Widths for Memory view mode.
    pub memory: Vec<u16>,
    /// Widths for Disk view mode.
    pub disk: Vec<u16>,
}

/// Trait for table row items.
pub trait TableRow: Clone {
    /// Unique identifier for diff tracking.
    fn id(&self) -> u64;

    /// Number of columns.
    fn column_count() -> usize;

    /// Column headers.
    fn headers() -> Vec<&'static str>;

    /// Cell values as strings.
    fn cells(&self) -> Vec<String>;

    /// Sort key for the specified column.
    fn sort_key(&self, column: usize) -> SortKey;

    /// Check if item matches the filter.
    fn matches_filter(&self, filter: &str) -> bool;
}

/// State for a table widget.
#[derive(Debug, Clone)]
pub struct TableState<T: TableRow> {
    /// All items (unfiltered).
    pub items: Vec<T>,
    /// Selected row index (in filtered view).
    pub selected: usize,
    /// Sort column index.
    pub sort_column: usize,
    /// Sort direction (true = ascending).
    pub sort_ascending: bool,
    /// Filter string.
    pub filter: Option<String>,
    /// Scroll offset for large tables.
    pub scroll_offset: usize,
    /// Previous items for diff tracking.
    previous: HashMap<u64, T>,
    /// Diff status for each item.
    pub diff_status: HashMap<u64, DiffStatus>,
}

impl<T: TableRow> Default for TableState<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: TableRow> TableState<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            sort_column: 0,
            sort_ascending: false, // Default descending (highest first)
            filter: None,
            scroll_offset: 0,
            previous: HashMap::new(),
            diff_status: HashMap::new(),
        }
    }

    /// Updates items and computes diff status.
    pub fn update(&mut self, new_items: Vec<T>) {
        // Compute diff status
        self.diff_status.clear();
        for item in &new_items {
            let id = item.id();
            let status = if let Some(prev) = self.previous.get(&id) {
                // Check which cells changed
                let prev_cells = prev.cells();
                let new_cells = item.cells();
                let changed: Vec<usize> = prev_cells
                    .iter()
                    .zip(new_cells.iter())
                    .enumerate()
                    .filter(|(_, (p, n))| p != n)
                    .map(|(i, _)| i)
                    .collect();

                if changed.is_empty() {
                    DiffStatus::Unchanged
                } else {
                    DiffStatus::Modified(changed)
                }
            } else {
                DiffStatus::New
            };
            self.diff_status.insert(id, status);
        }

        // Save current as previous for next update
        self.previous.clear();
        for item in &new_items {
            self.previous.insert(item.id(), item.clone());
        }

        self.items = new_items;
        self.apply_sort();

        // Adjust selection if needed
        let filtered_len = self.filtered_items().len();
        if self.selected >= filtered_len && filtered_len > 0 {
            self.selected = filtered_len - 1;
        }
    }

    /// Returns filtered and sorted items.
    pub fn filtered_items(&self) -> Vec<&T> {
        self.items
            .iter()
            .filter(|item| {
                self.filter
                    .as_ref()
                    .map(|f| item.matches_filter(f))
                    .unwrap_or(true)
            })
            .collect()
    }

    /// Applies current sort to items.
    fn apply_sort(&mut self) {
        let col = self.sort_column;
        let asc = self.sort_ascending;

        self.items.sort_by(|a, b| {
            let key_a = a.sort_key(col);
            let key_b = b.sort_key(col);
            let cmp = key_a
                .partial_cmp(&key_b)
                .unwrap_or(std::cmp::Ordering::Equal);
            if asc { cmp } else { cmp.reverse() }
        });
    }

    /// Cycles to next sort column.
    pub fn next_sort_column(&mut self) {
        self.sort_column = (self.sort_column + 1) % T::column_count();
        self.apply_sort();
    }

    /// Toggles sort direction.
    pub fn toggle_sort_direction(&mut self) {
        self.sort_ascending = !self.sort_ascending;
        self.apply_sort();
    }

    /// Sets filter string.
    pub fn set_filter(&mut self, filter: Option<String>) {
        self.filter = filter;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Moves selection up.
    pub fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Moves selection down.
    pub fn select_down(&mut self) {
        let max = self.filtered_items().len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
        }
    }

    /// Moves selection up by a page.
    pub fn page_up(&mut self, page_size: usize) {
        self.selected = self.selected.saturating_sub(page_size);
    }

    /// Moves selection down by a page.
    pub fn page_down(&mut self, page_size: usize) {
        let max = self.filtered_items().len().saturating_sub(1);
        self.selected = (self.selected + page_size).min(max);
    }
}

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
    /// Show help popup.
    pub show_help: bool,
    /// Show debug popup (collector timing, rates state).
    pub show_debug_popup: bool,
    /// Show quit confirmation popup.
    pub show_quit_confirm: bool,
    /// Scroll offset for help popup.
    pub help_scroll: usize,
    /// Show process detail popup (Enter on PRC tab).
    pub show_process_detail: bool,
    /// PID of the process shown in detail popup (to keep tracking it after sort).
    pub process_detail_pid: Option<u32>,
    /// Scroll offset for process detail popup.
    pub process_detail_scroll: usize,
    /// Per-tab state for filter, sort, and selection.
    pub tab_states: HashMap<Tab, TabState>,
    /// Last PostgreSQL collector error (for PGA tab display).
    pub pg_last_error: Option<String>,
    /// Selected row index for PGA tab.
    pub pg_selected: usize,
    /// Filter string for PGA tab.
    pub pg_filter: Option<String>,
    /// Sort column index for PGA tab.
    pub pg_sort_column: usize,
    /// Sort direction for PGA tab.
    pub pg_sort_ascending: bool,
    /// Hide idle sessions in PGA tab.
    pub pg_hide_idle: bool,
    /// Show PostgreSQL session detail popup (Enter on PGA tab).
    pub show_pg_detail: bool,
    /// PID of the PostgreSQL session shown in detail popup.
    pub pg_detail_pid: Option<i32>,
    /// Scroll offset for PostgreSQL detail popup.
    pub pg_detail_scroll: usize,
    /// Current view mode for PGA tab (g/s).
    pub pga_view_mode: PgActivityViewMode,
    /// Flag set when user requests drill-down navigation (>/J keys).
    /// Cleared after processing by app.rs.
    pub drill_down_requested: bool,
    /// PID to navigate to in PGA tab (after drill-down from PRC).
    /// Cleared after render finds and selects the row.
    pub pga_navigate_to_pid: Option<i32>,
    /// Tracked PID for persistent selection in PGA tab.
    /// Used for drill-down navigation and popup detail.
    pub pg_tracked_pid: Option<i32>,

    // ===== pg_stat_statements (PGS tab) =====
    /// Current view mode for PGS tab (t/c/i/e).
    pub pgs_view_mode: PgStatementsViewMode,
    /// Selected row index for PGS tab.
    pub pgs_selected: usize,
    /// Filter string for PGS tab.
    pub pgs_filter: Option<String>,
    /// Sort column index for PGS tab (within current view mode).
    pub pgs_sort_column: usize,
    /// Sort direction for PGS tab.
    pub pgs_sort_ascending: bool,
    /// Show pg_stat_statements detail popup (Enter on PGS tab).
    pub show_pgs_detail: bool,
    /// queryid of the statement shown in detail popup.
    pub pgs_detail_queryid: Option<i64>,
    /// Scroll offset for pg_stat_statements detail popup.
    pub pgs_detail_scroll: usize,
    /// query_id to navigate to in PGS tab (after drill-down from PGA).
    /// Cleared after render finds and selects the row.
    pub pgs_navigate_to_queryid: Option<i64>,
    /// Tracked queryid for persistent selection in PGS tab.
    /// Used for drill-down navigation and popup detail.
    pub pgs_tracked_queryid: Option<i64>,

    /// Cached per-statement rate metrics (`/s`) computed from deltas between two real samples.
    pub pgs_rates: HashMap<i64, PgStatementsRates>,
    /// Timestamp (seconds) of the previous real sample used as the baseline for rate computation.
    pub pgs_prev_sample_ts: Option<i64>,
    /// Previous real sample: `queryid -> statement counters`.
    pub pgs_prev_sample: HashMap<i64, PgStatStatementsInfo>,
    /// Timestamp (seconds) of the last real pg_stat_statements update (for age display).
    pub pgs_last_real_update_ts: Option<i64>,
    /// Sample interval (seconds) used for the last rate computation.
    pub pgs_dt_secs: Option<f64>,
    /// Current `collected_at` timestamp from the latest snapshot (for debugging).
    pub pgs_current_collected_at: Option<i64>,

    // ===== Ratatui TableState for scrolling =====
    /// Ratatui table state for PRC tab (enables auto-scrolling).
    pub prc_ratatui_state: RatatuiTableState,
    /// Ratatui table state for PGA tab (enables auto-scrolling).
    pub pga_ratatui_state: RatatuiTableState,
    /// Ratatui table state for PGS tab (enables auto-scrolling).
    pub pgs_ratatui_state: RatatuiTableState,
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
            show_help: false,
            show_debug_popup: false,
            show_quit_confirm: false,
            help_scroll: 0,
            show_process_detail: false,
            process_detail_pid: None,
            process_detail_scroll: 0,
            tab_states: HashMap::new(),
            pg_last_error: None,
            pg_selected: 0,
            pg_filter: None,
            pg_sort_column: 7,        // Default: sort by QDUR
            pg_sort_ascending: false, // Descending (longest first)
            pg_hide_idle: false,
            show_pg_detail: false,
            pg_detail_pid: None,
            pg_detail_scroll: 0,
            pga_view_mode: PgActivityViewMode::Generic,
            drill_down_requested: false,
            pga_navigate_to_pid: None,
            pg_tracked_pid: None,

            pgs_view_mode: PgStatementsViewMode::Time,
            pgs_selected: 0,
            pgs_filter: None,
            pgs_sort_column: 1,        // Default (Time view): TIME/s
            pgs_sort_ascending: false, // Descending (largest first)
            show_pgs_detail: false,
            pgs_detail_queryid: None,
            pgs_detail_scroll: 0,
            pgs_navigate_to_queryid: None,
            pgs_tracked_queryid: None,

            pgs_rates: HashMap::new(),
            pgs_prev_sample_ts: None,
            pgs_prev_sample: HashMap::new(),
            pgs_last_real_update_ts: None,
            pgs_dt_secs: None,
            pgs_current_collected_at: None,

            prc_ratatui_state: RatatuiTableState::default(),
            pga_ratatui_state: RatatuiTableState::default(),
            pgs_ratatui_state: RatatuiTableState::default(),
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
                filter: self.pg_filter.clone(),
                sort_column: self.pg_sort_column,
                sort_ascending: self.pg_sort_ascending,
                selected: self.pg_selected,
            },
            Tab::PgStatements => TabState {
                filter: self.pgs_filter.clone(),
                sort_column: self.pgs_sort_column,
                sort_ascending: self.pgs_sort_ascending,
                selected: self.pgs_selected,
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
                    self.pg_filter = state.filter.clone();
                    self.filter_input = state.filter.clone().unwrap_or_default();
                    self.pg_sort_column = state.sort_column;
                    self.pg_sort_ascending = state.sort_ascending;
                    self.pg_selected = state.selected;
                }
                Tab::PgStatements => {
                    self.pgs_filter = state.filter.clone();
                    self.filter_input = state.filter.clone().unwrap_or_default();
                    self.pgs_sort_column = state.sort_column;
                    self.pgs_sort_ascending = state.sort_ascending;
                    self.pgs_selected = state.selected;
                }
            }
        }
    }

    /// Returns true if any detail popup is currently open.
    pub fn any_popup_open(&self) -> bool {
        self.show_process_detail || self.show_pg_detail || self.show_pgs_detail
    }

    /// Switches to a new tab, saving current and restoring target state.
    pub fn switch_tab(&mut self, new_tab: Tab) {
        if self.current_tab != new_tab {
            // Reset tracked state when leaving a tab
            match self.current_tab {
                Tab::PostgresActive => {
                    self.pg_tracked_pid = None;
                }
                Tab::PgStatements => {
                    self.pgs_tracked_queryid = None;
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

    /// Cycles to next sort column for PGA tab.
    pub fn next_pg_sort_column(&mut self) {
        // 12 columns: PID CPU% RSS DB USER STATE WAIT QDUR XDUR BDUR BTYPE QUERY
        self.pg_sort_column = (self.pg_sort_column + 1) % 12;
    }

    fn pgs_column_count(&self) -> usize {
        match self.pgs_view_mode {
            PgStatementsViewMode::Time => 7, // CALLS/s TIME/s MEAN ROWS/s DB USER QUERY
            PgStatementsViewMode::Calls => 7, // CALLS/s ROWS/s R/CALL MEAN DB USER QUERY
            PgStatementsViewMode::Io => 8, // CALLS/s BLK_RD/s BLK_HIT/s HIT% BLK_DIRT/s BLK_WR/s DB QUERY
            PgStatementsViewMode::Temp => 8, // CALLS/s TMP_RD/s TMP_WR/s TMP_MB/s LOC_RD/s LOC_WR/s DB QUERY
        }
    }

    /// Cycles to next sort column for PGS tab.
    pub fn next_pgs_sort_column(&mut self) {
        let count = self.pgs_column_count();
        if count == 0 {
            return;
        }
        self.pgs_sort_column = (self.pgs_sort_column + 1) % count;
    }

    /// Toggles sort direction for PGA tab.
    pub fn toggle_pg_sort_direction(&mut self) {
        self.pg_sort_ascending = !self.pg_sort_ascending;
    }

    /// Toggles sort direction for PGS tab.
    pub fn toggle_pgs_sort_direction(&mut self) {
        self.pgs_sort_ascending = !self.pgs_sort_ascending;
    }

    /// Toggles sort direction for network tab.
    pub fn toggle_net_sort_direction(&mut self) {
        self.net_sort_ascending = !self.net_sort_ascending;
    }

    pub fn reset_pgs_rate_state(&mut self) {
        self.pgs_rates.clear();
        self.pgs_prev_sample_ts = None;
        self.pgs_prev_sample.clear();
        self.pgs_last_real_update_ts = None;
        self.pgs_dt_secs = None;
    }

    /// Updates cached PGS rate metrics from the given snapshot.
    ///
    /// Important: `pg_stat_statements` may be cached by the collector for ~30s.
    /// We use `collected_at` timestamps from the data to compute accurate rates
    /// regardless of TUI update interval.
    pub fn update_pgs_rates_from_snapshot(&mut self, snapshot: &Snapshot) {
        let Some(current) = snapshot.blocks.iter().find_map(|b| {
            if let DataBlock::PgStatStatements(v) = b {
                Some(v)
            } else {
                None
            }
        }) else {
            self.reset_pgs_rate_state();
            return;
        };

        if current.is_empty() {
            self.reset_pgs_rate_state();
            return;
        }

        // Use collected_at from statements (all have same value within one collection).
        // Fall back to snapshot.timestamp for old data without collected_at.
        let now_ts = current
            .first()
            .map(|s| s.collected_at)
            .filter(|&t| t > 0)
            .unwrap_or(snapshot.timestamp);

        // Store current collected_at for debugging (shows what timestamp is in the data).
        self.pgs_current_collected_at = Some(now_ts);

        // Update last real update timestamp for age display.
        self.pgs_last_real_update_ts = Some(now_ts);

        // If we moved backwards in time (history rewind / jump), reset baseline.
        if let Some(prev_ts) = self.pgs_prev_sample_ts
            && now_ts < prev_ts
        {
            self.pgs_rates.clear();
            self.pgs_prev_sample_ts = Some(now_ts);
            self.pgs_prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
            return;
        }

        // First sample: store baseline only.
        let Some(prev_ts) = self.pgs_prev_sample_ts else {
            self.pgs_prev_sample_ts = Some(now_ts);
            self.pgs_prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
            self.pgs_rates.clear();
            return;
        };

        // Check if this is actually new data (collected_at changed).
        // With caching, multiple snapshots may have the same collected_at.
        if now_ts == prev_ts {
            // Same collected_at means cached data - keep existing rates.
            return;
        }

        let dt = now_ts.saturating_sub(prev_ts) as f64;
        if dt <= 0.0 {
            self.pgs_prev_sample_ts = Some(now_ts);
            self.pgs_prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
            self.pgs_rates.clear();
            return;
        }

        fn delta_i64(curr: i64, prev: i64) -> Option<i64> {
            (curr >= prev).then_some(curr - prev)
        }

        fn delta_f64(curr: f64, prev: f64) -> Option<f64> {
            (curr >= prev).then_some(curr - prev)
        }

        let mut rates = HashMap::with_capacity(current.len());
        for s in current {
            let prev = self.pgs_prev_sample.get(&s.queryid);
            let mut r = PgStatementsRates {
                dt_secs: dt,
                ..PgStatementsRates::default()
            };

            if let Some(prev) = prev {
                r.calls_s = delta_i64(s.calls, prev.calls).map(|d| d as f64 / dt);
                r.rows_s = delta_i64(s.rows, prev.rows).map(|d| d as f64 / dt);
                r.exec_time_ms_s =
                    delta_f64(s.total_exec_time, prev.total_exec_time).map(|d| d / dt);

                r.shared_blks_read_s =
                    delta_i64(s.shared_blks_read, prev.shared_blks_read).map(|d| d as f64 / dt);
                r.shared_blks_hit_s =
                    delta_i64(s.shared_blks_hit, prev.shared_blks_hit).map(|d| d as f64 / dt);
                r.shared_blks_dirtied_s =
                    delta_i64(s.shared_blks_dirtied, prev.shared_blks_dirtied)
                        .map(|d| d as f64 / dt);
                r.shared_blks_written_s =
                    delta_i64(s.shared_blks_written, prev.shared_blks_written)
                        .map(|d| d as f64 / dt);

                r.local_blks_read_s =
                    delta_i64(s.local_blks_read, prev.local_blks_read).map(|d| d as f64 / dt);
                r.local_blks_written_s =
                    delta_i64(s.local_blks_written, prev.local_blks_written).map(|d| d as f64 / dt);

                r.temp_blks_read_s =
                    delta_i64(s.temp_blks_read, prev.temp_blks_read).map(|d| d as f64 / dt);
                r.temp_blks_written_s =
                    delta_i64(s.temp_blks_written, prev.temp_blks_written).map(|d| d as f64 / dt);

                // Temp MB/s based on combined temp blocks delta (assumes 8 KiB blocks).
                if let (Some(dr), Some(dw)) = (
                    delta_i64(s.temp_blks_read, prev.temp_blks_read),
                    delta_i64(s.temp_blks_written, prev.temp_blks_written),
                ) {
                    let blocks = (dr + dw) as f64;
                    let mb = (blocks * 8.0) / 1024.0;
                    r.temp_mb_s = Some(mb / dt);
                }
            }

            rates.insert(s.queryid, r);
        }

        self.pgs_rates = rates;
        self.pgs_prev_sample_ts = Some(now_ts);
        self.pgs_prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
        self.pgs_dt_secs = Some(dt);
    }
}

/// Process row for the process table.
/// Contains all fields needed for different view modes (Generic/Command/Memory).
#[derive(Debug, Clone, Default)]
pub struct ProcessRow {
    // Identity
    pub pid: u32,
    pub tid: u32,
    pub name: String,
    pub cmdline: String,

    // CPU metrics
    pub syscpu: u64, // stime - kernel time (ticks)
    pub usrcpu: u64, // utime - user time (ticks)
    pub cpu_percent: f64,
    pub rdelay: u64, // rundelay - time waiting for CPU (ns)
    pub cpunr: i32,  // current CPU number

    // Memory metrics (all in KB)
    pub minflt: u64,
    pub majflt: u64,
    pub vstext: u64,      // vexec - executable code
    pub vslibs: u64,      // shared libraries
    pub vdata: u64,       // data segment
    pub vstack: u64,      // stack
    pub vlock: u64,       // locked memory (LOCKSZ)
    pub vsize: u64,       // total virtual memory
    pub rsize: u64,       // resident memory
    pub psize: u64,       // proportional set size
    pub vswap: u64,       // swap usage (SWAPSZ)
    pub mem_percent: f64, // MEM%

    // Deltas (require previous snapshot)
    pub vgrow: i64, // delta vsize
    pub rgrow: i64, // delta rsize

    // User identification
    pub ruid: u32,
    pub euid: u32,
    pub ruser: String, // Resolved username from ruid
    pub euser: String, // Resolved username from euid

    // State
    pub state: String,    // S (R/S/D/Z/T)
    pub exit_code: i32,   // EXC
    pub num_threads: u32, // THR
    pub btime: u32,       // Start time (unix timestamp)

    // PostgreSQL integration
    pub query: Option<String>, // Query from pg_stat_activity if PID matches
    pub backend_type: Option<String>, // Backend type from pg_stat_activity if PID matches

    // Disk I/O metrics (rates in bytes per second)
    pub rddsk: i64,       // Read bytes/s (delta from rsz)
    pub wrdsk: i64,       // Write bytes/s (delta from wsz)
    pub wcancl: i64,      // Cancelled write bytes/s (delta from cwsz)
    pub dsk_percent: f64, // % of total system disk I/O
}

impl ProcessRow {
    /// Returns headers for Generic view mode.
    pub fn headers_generic() -> Vec<&'static str> {
        vec![
            "PID", "SYSCPU", "USRCPU", "RDELAY", "VGROW", "RGROW", "RUID", "EUID", "ST", "EXC",
            "THR", "S", "CPUNR", "CPU", "CMD",
        ]
    }

    /// Returns headers for Command view mode.
    pub fn headers_command() -> Vec<&'static str> {
        vec!["PID", "TID", "S", "CPU", "MEM", "COMMAND-LINE"]
    }

    /// Returns headers for Memory view mode.
    pub fn headers_memory() -> Vec<&'static str> {
        vec![
            "PID", "TID", "MINFLT", "MAJFLT", "VSTEXT", "VSLIBS", "VDATA", "VSTACK", "LOCKSZ",
            "VSIZE", "RSIZE", "PSIZE", "VGROW", "RGROW", "SWAPSZ", "RUID", "EUID", "MEM", "CMD",
        ]
    }

    /// Returns column widths for Generic view mode.
    pub fn widths_generic() -> Vec<u16> {
        vec![8, 8, 8, 8, 8, 8, 6, 6, 3, 4, 4, 2, 5, 5, 20]
    }

    /// Returns column widths for Command view mode.
    pub fn widths_command() -> Vec<u16> {
        vec![8, 8, 2, 6, 8, 60]
    }

    /// Returns column widths for Memory view mode.
    pub fn widths_memory() -> Vec<u16> {
        vec![8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 6, 6, 5, 20]
    }

    /// Returns headers for Disk view mode.
    pub fn headers_disk() -> Vec<&'static str> {
        vec!["PID", "RDDSK", "WRDSK", "WCANCL", "DSK", "CMD"]
    }

    /// Returns column widths for Disk view mode.
    pub fn widths_disk() -> Vec<u16> {
        vec![8, 10, 10, 10, 6, 20]
    }

    /// Returns cells for Generic view mode.
    pub fn cells_generic(&self) -> Vec<String> {
        vec![
            self.pid.to_string(),
            format_ticks(self.syscpu),
            format_ticks(self.usrcpu),
            format_delay(self.rdelay),
            format_size_delta(self.vgrow),
            format_size_delta(self.rgrow),
            self.ruser.clone(),
            self.euser.clone(),
            format_start_time(self.btime),
            self.exit_code.to_string(),
            self.num_threads.to_string(),
            self.state.clone(),
            self.cpunr.to_string(),
            format!("{:.1}%", self.cpu_percent),
            self.format_cmd_with_query(),
        ]
    }

    /// Returns cells for Command view mode.
    pub fn cells_command(&self) -> Vec<String> {
        vec![
            self.pid.to_string(),
            self.tid.to_string(),
            self.state.clone(),
            format!("{:.1}%", self.cpu_percent),
            format_memory(self.rsize),
            self.format_cmdline_with_query(),
        ]
    }

    /// Returns cells for Memory view mode.
    pub fn cells_memory(&self) -> Vec<String> {
        vec![
            self.pid.to_string(),
            self.tid.to_string(),
            self.minflt.to_string(),
            self.majflt.to_string(),
            format_memory(self.vstext),
            format_memory(self.vslibs),
            format_memory(self.vdata),
            format_memory(self.vstack),
            format_memory(self.vlock),
            format_memory(self.vsize),
            format_memory(self.rsize),
            format_memory(self.psize),
            format_size_delta(self.vgrow),
            format_size_delta(self.rgrow),
            format_memory(self.vswap),
            self.ruser.clone(),
            self.euser.clone(),
            format!("{:.1}%", self.mem_percent),
            self.format_cmd_with_query(),
        ]
    }

    /// Returns cells for Disk view mode.
    pub fn cells_disk(&self) -> Vec<String> {
        vec![
            self.pid.to_string(),
            format_bytes_rate(self.rddsk),
            format_bytes_rate(self.wrdsk),
            format_bytes_rate(self.wcancl),
            format!("{:.1}%", self.dsk_percent),
            self.format_cmd_with_query(),
        ]
    }

    /// Formats CMD column with optional query or backend_type from pg_stat_activity.
    /// Returns "name [query]" if query is present and non-empty,
    /// "name [backend_type]" if only backend_type is present,
    /// otherwise just "name".
    fn format_cmd_with_query(&self) -> String {
        match &self.query {
            Some(q) if !q.is_empty() => format!("{} [{}]", self.name, q),
            _ => match &self.backend_type {
                Some(bt) if !bt.is_empty() => format!("{} [{}]", self.name, bt),
                _ => self.name.clone(),
            },
        }
    }

    /// Formats COMMAND-LINE column with optional query or backend_type from pg_stat_activity.
    /// Returns "cmdline [query]" if query is present and non-empty,
    /// "cmdline [backend_type]" if only backend_type is present,
    /// otherwise just cmdline (or name if cmdline is empty).
    fn format_cmdline_with_query(&self) -> String {
        let base = if self.cmdline.is_empty() {
            &self.name
        } else {
            &self.cmdline
        };
        match &self.query {
            Some(q) if !q.is_empty() => format!("{} [{}]", base, q),
            _ => match &self.backend_type {
                Some(bt) if !bt.is_empty() => format!("{} [{}]", base, bt),
                _ => base.clone(),
            },
        }
    }

    /// Returns sort key for the specified column and view mode.
    pub fn sort_key_for_mode(&self, column: usize, mode: ProcessViewMode) -> SortKey {
        match mode {
            ProcessViewMode::Generic => {
                // PID SYSCPU USRCPU RDELAY VGROW RGROW RUID EUID ST EXC THR S CPUNR CPU CMD
                match column {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Integer(self.syscpu as i64),
                    2 => SortKey::Integer(self.usrcpu as i64),
                    3 => SortKey::Integer(self.rdelay as i64),
                    4 => SortKey::Integer(self.vgrow),
                    5 => SortKey::Integer(self.rgrow),
                    6 => SortKey::Integer(self.ruid as i64),
                    7 => SortKey::Integer(self.euid as i64),
                    8 => SortKey::String(String::new()), // ST placeholder
                    9 => SortKey::Integer(self.exit_code as i64),
                    10 => SortKey::Integer(self.num_threads as i64),
                    11 => SortKey::String(self.state.clone()),
                    12 => SortKey::Integer(self.cpunr as i64),
                    13 => SortKey::Float(self.cpu_percent),
                    14 => SortKey::String(self.name.clone()),
                    _ => SortKey::Integer(0),
                }
            }
            ProcessViewMode::Command => {
                // PID TID S CPU MEM COMMAND-LINE
                match column {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Integer(self.tid as i64),
                    2 => SortKey::String(self.state.clone()),
                    3 => SortKey::Float(self.cpu_percent),
                    4 => SortKey::Integer(self.rsize as i64), // MEM = rsize
                    5 => SortKey::String(if self.cmdline.is_empty() {
                        self.name.clone()
                    } else {
                        self.cmdline.clone()
                    }),
                    _ => SortKey::Integer(0),
                }
            }
            ProcessViewMode::Memory => {
                // PID TID MINFLT MAJFLT VSTEXT VSLIBS VDATA VSTACK LOCKSZ VSIZE RSIZE PSIZE VGROW RGROW SWAPSZ RUID EUID MEM CMD
                match column {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Integer(self.tid as i64),
                    2 => SortKey::Integer(self.minflt as i64),
                    3 => SortKey::Integer(self.majflt as i64),
                    4 => SortKey::Integer(self.vstext as i64),
                    5 => SortKey::Integer(self.vslibs as i64),
                    6 => SortKey::Integer(self.vdata as i64),
                    7 => SortKey::Integer(self.vstack as i64),
                    8 => SortKey::Integer(self.vlock as i64),
                    9 => SortKey::Integer(self.vsize as i64),
                    10 => SortKey::Integer(self.rsize as i64),
                    11 => SortKey::Integer(self.psize as i64),
                    12 => SortKey::Integer(self.vgrow),
                    13 => SortKey::Integer(self.rgrow),
                    14 => SortKey::Integer(self.vswap as i64),
                    15 => SortKey::Integer(self.ruid as i64),
                    16 => SortKey::Integer(self.euid as i64),
                    17 => SortKey::Float(self.mem_percent),
                    18 => SortKey::String(self.name.clone()),
                    _ => SortKey::Integer(0),
                }
            }
            ProcessViewMode::Disk => {
                // PID RDDSK WRDSK WCANCL DSK CMD
                match column {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Integer(self.rddsk),
                    2 => SortKey::Integer(self.wrdsk),
                    3 => SortKey::Integer(self.wcancl),
                    4 => SortKey::Float(self.dsk_percent),
                    5 => SortKey::String(self.name.clone()),
                    _ => SortKey::Integer(0),
                }
            }
        }
    }

    /// Returns headers for the specified view mode.
    pub fn headers_for_mode(mode: ProcessViewMode) -> Vec<&'static str> {
        match mode {
            ProcessViewMode::Generic => Self::headers_generic(),
            ProcessViewMode::Command => Self::headers_command(),
            ProcessViewMode::Memory => Self::headers_memory(),
            ProcessViewMode::Disk => Self::headers_disk(),
        }
    }

    /// Returns cells for the specified view mode.
    pub fn cells_for_mode(&self, mode: ProcessViewMode) -> Vec<String> {
        match mode {
            ProcessViewMode::Generic => self.cells_generic(),
            ProcessViewMode::Command => self.cells_command(),
            ProcessViewMode::Memory => self.cells_memory(),
            ProcessViewMode::Disk => self.cells_disk(),
        }
    }

    /// Returns column widths for the specified view mode.
    pub fn widths_for_mode(mode: ProcessViewMode) -> Vec<u16> {
        match mode {
            ProcessViewMode::Generic => Self::widths_generic(),
            ProcessViewMode::Command => Self::widths_command(),
            ProcessViewMode::Memory => Self::widths_memory(),
            ProcessViewMode::Disk => Self::widths_disk(),
        }
    }

    /// Returns minimum column widths for Generic view mode.
    pub fn min_widths_generic() -> Vec<u16> {
        // PID SYSCPU USRCPU RDELAY VGROW RGROW RUID EUID ST EXC THR S CPUNR CPU CMD
        vec![5, 7, 7, 7, 7, 7, 4, 4, 2, 3, 3, 1, 3, 6, 8]
    }

    /// Returns minimum column widths for Command view mode.
    pub fn min_widths_command() -> Vec<u16> {
        // PID TID S CPU MEM COMMAND-LINE
        vec![5, 5, 1, 4, 4, 10]
    }

    /// Returns minimum column widths for Memory view mode.
    pub fn min_widths_memory() -> Vec<u16> {
        // PID TID MINFLT MAJFLT VSTEXT VSLIBS VDATA VSTACK LOCKSZ VSIZE RSIZE PSIZE VGROW RGROW SWAPSZ RUID EUID MEM CMD
        vec![5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 4, 4, 8]
    }

    /// Returns minimum column widths for Disk view mode.
    pub fn min_widths_disk() -> Vec<u16> {
        // PID RDDSK WRDSK WCANCL DSK CMD
        vec![5, 6, 6, 6, 4, 8]
    }

    /// Returns minimum column widths for the specified view mode.
    pub fn min_widths_for_mode(mode: ProcessViewMode) -> Vec<u16> {
        match mode {
            ProcessViewMode::Generic => Self::min_widths_generic(),
            ProcessViewMode::Command => Self::min_widths_command(),
            ProcessViewMode::Memory => Self::min_widths_memory(),
            ProcessViewMode::Disk => Self::min_widths_disk(),
        }
    }

    /// Returns column types for Generic view mode.
    pub fn column_types_generic() -> Vec<ColumnType> {
        use ColumnType::*;
        // PID SYSCPU USRCPU RDELAY VGROW RGROW RUID EUID ST EXC THR S CPUNR CPU CMD
        vec![
            Fixed,      // PID
            Flexible,   // SYSCPU
            Flexible,   // USRCPU
            Flexible,   // RDELAY
            Flexible,   // VGROW
            Flexible,   // RGROW
            Flexible,   // RUID
            Flexible,   // EUID
            Fixed,      // ST
            Fixed,      // EXC
            Fixed,      // THR
            Fixed,      // S
            Fixed,      // CPUNR
            Flexible,   // CPU
            Expandable, // CMD
        ]
    }

    /// Returns column types for Command view mode.
    pub fn column_types_command() -> Vec<ColumnType> {
        use ColumnType::*;
        // PID TID S CPU MEM COMMAND-LINE
        vec![
            Fixed,      // PID
            Fixed,      // TID
            Fixed,      // S
            Flexible,   // CPU
            Flexible,   // MEM
            Expandable, // COMMAND-LINE
        ]
    }

    /// Returns column types for Memory view mode.
    pub fn column_types_memory() -> Vec<ColumnType> {
        use ColumnType::*;
        // PID TID MINFLT MAJFLT VSTEXT VSLIBS VDATA VSTACK LOCKSZ VSIZE RSIZE PSIZE VGROW RGROW SWAPSZ RUID EUID MEM CMD
        vec![
            Fixed,      // PID
            Fixed,      // TID
            Flexible,   // MINFLT
            Flexible,   // MAJFLT
            Flexible,   // VSTEXT
            Flexible,   // VSLIBS
            Flexible,   // VDATA
            Flexible,   // VSTACK
            Flexible,   // LOCKSZ
            Flexible,   // VSIZE
            Flexible,   // RSIZE
            Flexible,   // PSIZE
            Flexible,   // VGROW
            Flexible,   // RGROW
            Flexible,   // SWAPSZ
            Flexible,   // RUID
            Flexible,   // EUID
            Flexible,   // MEM
            Expandable, // CMD
        ]
    }

    /// Returns column types for Disk view mode.
    pub fn column_types_disk() -> Vec<ColumnType> {
        use ColumnType::*;
        // PID RDDSK WRDSK WCANCL DSK CMD
        vec![
            Fixed,      // PID
            Flexible,   // RDDSK
            Flexible,   // WRDSK
            Flexible,   // WCANCL
            Flexible,   // DSK
            Expandable, // CMD
        ]
    }

    /// Returns column types for the specified view mode.
    pub fn column_types_for_mode(mode: ProcessViewMode) -> Vec<ColumnType> {
        match mode {
            ProcessViewMode::Generic => Self::column_types_generic(),
            ProcessViewMode::Command => Self::column_types_command(),
            ProcessViewMode::Memory => Self::column_types_memory(),
            ProcessViewMode::Disk => Self::column_types_disk(),
        }
    }
}

/// Format CPU ticks as human-readable time.
fn format_ticks(ticks: u64) -> String {
    // Assuming 100 ticks per second (standard Linux)
    let seconds = ticks / 100;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    if hours > 0 {
        format!("{}h{}m", hours, minutes % 60)
    } else if minutes > 0 {
        format!("{}m{}s", minutes, seconds % 60)
    } else {
        format!("{}s", seconds)
    }
}

/// Format nanoseconds delay as human-readable.
fn format_delay(ns: u64) -> String {
    let us = ns / 1000;
    let ms = us / 1000;
    let seconds = ms / 1000;
    if seconds > 0 {
        format!("{}s", seconds)
    } else if ms > 0 {
        format!("{}ms", ms)
    } else if us > 0 {
        format!("{}us", us)
    } else {
        "0".to_string()
    }
}

/// Format memory size (KB) as human-readable.
fn format_memory(kb: u64) -> String {
    if kb >= 1024 * 1024 {
        format!("{}G", kb / (1024 * 1024))
    } else if kb >= 1024 {
        format!("{}M", kb / 1024)
    } else {
        format!("{}K", kb)
    }
}

/// Format size delta (KB) with sign.
fn format_size_delta(delta: i64) -> String {
    if delta == 0 {
        "0".to_string()
    } else if delta > 0 {
        format!(
            "{}+{}",
            if delta >= 1024 {
                format!("{}M", delta / 1024)
            } else {
                format!("{}K", delta)
            },
            ""
        )
        .trim_end_matches('+')
        .to_string()
    } else {
        let abs_delta = delta.unsigned_abs() as i64;
        if abs_delta >= 1024 {
            format!("-{}M", abs_delta / 1024)
        } else {
            format!("-{}K", abs_delta)
        }
    }
}

/// Format bytes rate (bytes per second) as human-readable with auto units.
fn format_bytes_rate(rate: i64) -> String {
    let abs_rate = rate.unsigned_abs();
    let sign = if rate < 0 { "-" } else { "" };
    if abs_rate >= 1024 * 1024 * 1024 {
        format!("{}{}G/s", sign, abs_rate / (1024 * 1024 * 1024))
    } else if abs_rate >= 1024 * 1024 {
        format!("{}{}M/s", sign, abs_rate / (1024 * 1024))
    } else if abs_rate >= 1024 {
        format!("{}{}K/s", sign, abs_rate / 1024)
    } else if abs_rate > 0 {
        format!("{}{}B/s", sign, abs_rate)
    } else {
        "0".to_string()
    }
}

/// Format process start time (unix timestamp) as HH:MM or date.
fn format_start_time(btime: u32) -> String {
    use std::time::{Duration, UNIX_EPOCH};

    if btime == 0 {
        return "--".to_string();
    }

    let start = UNIX_EPOCH + Duration::from_secs(btime as u64);
    let now = std::time::SystemTime::now();

    if let Ok(duration) = now.duration_since(start) {
        let secs = duration.as_secs();
        // If started today (less than 24 hours ago), show HH:MM
        if secs < 24 * 3600 {
            // Calculate time of day
            if let Ok(epoch_secs) = start.duration_since(UNIX_EPOCH) {
                let total_secs = epoch_secs.as_secs();
                let hours = (total_secs / 3600) % 24;
                let minutes = (total_secs / 60) % 60;
                return format!("{:02}:{:02}", hours, minutes);
            }
        }
        // Otherwise show date
        let days = secs / (24 * 3600);
        if days < 365 {
            // Show month/day
            if let Ok(epoch_secs) = start.duration_since(UNIX_EPOCH) {
                // Simple approximation: calculate day of year
                let total_days = epoch_secs.as_secs() / (24 * 3600);
                let day_of_year = total_days % 365;
                let month = day_of_year / 30 + 1;
                let day = day_of_year % 30 + 1;
                return format!("{:02}/{:02}", month, day);
            }
        }
    }
    "--".to_string()
}

impl TableRow for ProcessRow {
    fn id(&self) -> u64 {
        self.pid as u64
    }

    fn column_count() -> usize {
        // Default to Generic view column count
        15
    }

    fn headers() -> Vec<&'static str> {
        // Default to Generic view
        Self::headers_generic()
    }

    fn cells(&self) -> Vec<String> {
        // Default to Generic view
        self.cells_generic()
    }

    fn sort_key(&self, column: usize) -> SortKey {
        // Generic view columns for sorting
        match column {
            0 => SortKey::Integer(self.pid as i64),
            1 => SortKey::Integer(self.syscpu as i64),
            2 => SortKey::Integer(self.usrcpu as i64),
            3 => SortKey::Integer(self.rdelay as i64),
            4 => SortKey::Integer(self.vgrow),
            5 => SortKey::Integer(self.rgrow),
            6 => SortKey::Integer(self.ruid as i64),
            7 => SortKey::Integer(self.euid as i64),
            10 => SortKey::Integer(self.num_threads as i64),
            11 => SortKey::String(self.state.clone()),
            12 => SortKey::Integer(self.cpunr as i64),
            13 => SortKey::Float(self.cpu_percent),
            14 => SortKey::String(self.name.clone()),
            _ => SortKey::Integer(0),
        }
    }

    fn matches_filter(&self, filter: &str) -> bool {
        let filter_lower = filter.to_lowercase();
        self.name.to_lowercase().contains(&filter_lower)
            || self.cmdline.to_lowercase().contains(&filter_lower)
            || self.pid.to_string().contains(&filter_lower)
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    #[test]
    fn test_process_filter_substring() {
        let row = ProcessRow {
            name: "rpglot".to_string(),
            cmdline: "/usr/bin/rpglot --proc-path ./mock_proc".to_string(),
            pid: 12345,
            ..Default::default()
        };

        // Exact match
        assert!(
            row.matches_filter("rpglot"),
            "Should match exact name 'rpglot'"
        );

        // Substring matches
        assert!(row.matches_filter("rpg"), "Should match substring 'rpg'");
        assert!(row.matches_filter("lot"), "Should match substring 'lot'");
        assert!(row.matches_filter("glot"), "Should match substring 'glot'");

        // Case insensitive
        assert!(
            row.matches_filter("RPG"),
            "Should match case-insensitive 'RPG'"
        );
        assert!(
            row.matches_filter("RPGLOT"),
            "Should match case-insensitive 'RPGLOT'"
        );
    }

    #[test]
    fn test_table_state_filtered_items_substring() {
        let mut table: TableState<ProcessRow> = TableState::new();

        let rows = vec![
            ProcessRow {
                name: "rpglot".to_string(),
                cmdline: "/usr/bin/rpglot".to_string(),
                pid: 1001,
                ..Default::default()
            },
            ProcessRow {
                name: "bash".to_string(),
                cmdline: "/bin/bash".to_string(),
                pid: 1002,
                ..Default::default()
            },
            ProcessRow {
                name: "systemd".to_string(),
                cmdline: "/lib/systemd/systemd".to_string(),
                pid: 1,
                ..Default::default()
            },
        ];

        table.update(rows);

        // No filter - all items
        assert_eq!(table.filtered_items().len(), 3);

        // Exact filter
        table.set_filter(Some("rpglot".to_string()));
        assert_eq!(
            table.filtered_items().len(),
            1,
            "Should find rpglot with exact filter"
        );

        // Substring filter - THIS IS THE BUG CASE
        table.set_filter(Some("rpg".to_string()));
        let filtered = table.filtered_items();
        assert_eq!(
            filtered.len(),
            1,
            "Should find rpglot with substring filter 'rpg'"
        );
        assert_eq!(filtered[0].name, "rpglot");

        // Another substring
        table.set_filter(Some("sys".to_string()));
        assert_eq!(
            table.filtered_items().len(),
            1,
            "Should find systemd with substring filter 'sys'"
        );
    }
}

#[cfg(test)]
mod pgs_rates_tests {
    use super::*;
    use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};

    fn stmt(
        queryid: i64,
        calls: i64,
        total_exec_time: f64,
        rows: i64,
        shared_blks_read: i64,
        shared_blks_hit: i64,
        shared_blks_written: i64,
        shared_blks_dirtied: i64,
        local_blks_read: i64,
        local_blks_written: i64,
        temp_blks_read: i64,
        temp_blks_written: i64,
        collected_at: i64,
    ) -> PgStatStatementsInfo {
        PgStatStatementsInfo {
            queryid,
            calls,
            total_exec_time,
            rows,
            shared_blks_read,
            shared_blks_hit,
            shared_blks_written,
            shared_blks_dirtied,
            local_blks_read,
            local_blks_written,
            temp_blks_read,
            temp_blks_written,
            collected_at,
            ..Default::default()
        }
    }

    fn snapshot(ts: i64, stmts: Vec<PgStatStatementsInfo>) -> Snapshot {
        Snapshot {
            timestamp: ts,
            blocks: vec![DataBlock::PgStatStatements(stmts)],
        }
    }

    #[test]
    fn pgs_rates_computed_on_second_real_sample() {
        let mut state = AppState::new(true);

        // collected_at=100: first real sample
        let s1 = snapshot(
            100,
            vec![stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        state.update_pgs_rates_from_snapshot(&s1);
        assert!(state.pgs_rates.is_empty(), "first sample is baseline only");

        // collected_at=110: second real sample with new data
        let s2 = snapshot(
            110,
            vec![stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        state.update_pgs_rates_from_snapshot(&s2);

        let r = state.pgs_rates.get(&1).expect("rates should be present");
        assert!((r.dt_secs - 10.0).abs() < 1e-9);
        assert!((r.calls_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.rows_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.exec_time_ms_s.unwrap() - 10.0).abs() < 1e-9);
        assert!((r.shared_blks_read_s.unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn pgs_rates_not_recomputed_when_counters_unchanged() {
        let mut state = AppState::new(true);

        // collected_at=100: first real sample
        let s1 = snapshot(
            100,
            vec![stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        state.update_pgs_rates_from_snapshot(&s1);

        // collected_at=110: second real sample
        let s2 = snapshot(
            110,
            vec![stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        state.update_pgs_rates_from_snapshot(&s2);
        let baseline_ts = state.pgs_prev_sample_ts;

        // Same collected_at=110 (cached data), later snapshot.timestamp=120:
        // should NOT update baseline, should keep existing rates.
        let s3 = snapshot(
            120,
            vec![stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        state.update_pgs_rates_from_snapshot(&s3);
        assert_eq!(state.pgs_prev_sample_ts, baseline_ts);
        assert!(state.pgs_rates.contains_key(&1));
    }

    #[test]
    fn pgs_rates_handle_stats_reset() {
        let mut state = AppState::new(true);

        let s1 = snapshot(
            100,
            vec![stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        state.update_pgs_rates_from_snapshot(&s1);

        let s2 = snapshot(
            110,
            vec![stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        state.update_pgs_rates_from_snapshot(&s2);

        // Simulate pg_stat_statements reset: counters go down.
        let s_reset = snapshot(120, vec![stmt(1, 5, 50.0, 3, 2, 10, 0, 0, 0, 0, 0, 0, 120)]);
        state.update_pgs_rates_from_snapshot(&s_reset);
        let r = state.pgs_rates.get(&1).expect("rates entry should exist");
        assert_eq!(r.calls_s, None);
        assert_eq!(r.exec_time_ms_s, None);

        // Next sample after reset: deltas should be computed from reset baseline.
        let s_after = snapshot(130, vec![stmt(1, 7, 70.0, 5, 4, 12, 0, 0, 0, 0, 0, 0, 130)]);
        state.update_pgs_rates_from_snapshot(&s_after);
        let r = state.pgs_rates.get(&1).unwrap();
        assert!((r.calls_s.unwrap() - 0.2).abs() < 1e-9);
        assert!((r.exec_time_ms_s.unwrap() - 2.0).abs() < 1e-9);
    }
}
