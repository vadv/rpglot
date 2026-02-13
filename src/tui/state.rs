//! Application state management.

use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};
use ratatui::widgets::TableState as RatatuiTableState;
use std::collections::HashMap;

// Re-export table and models types so existing `use super::state::*` paths keep working.
pub use super::models::*;
pub use super::table::*;

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

/// Input mode for the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Filter,
    TimeJump,
}

/// Active popup state. Only one popup can be open at a time.
#[derive(Debug, Clone, PartialEq)]
pub enum PopupState {
    /// No popup is open.
    None,
    /// Help popup with scroll offset.
    Help { scroll: usize },
    /// Quit confirmation dialog.
    QuitConfirm,
    /// Debug/timing popup (live mode only).
    Debug,
    /// Process detail popup (PRC tab).
    ProcessDetail { pid: u32, scroll: usize },
    /// PostgreSQL session detail popup (PGA tab).
    PgDetail { pid: i32, scroll: usize },
    /// pg_stat_statements detail popup (PGS tab).
    PgsDetail { queryid: i64, scroll: usize },
}

impl Default for PopupState {
    fn default() -> Self {
        Self::None
    }
}

impl PopupState {
    /// Returns true if any popup is open (excluding None).
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns true if a detail popup (ProcessDetail, PgDetail, PgsDetail) is open.
    pub fn is_detail_open(&self) -> bool {
        matches!(
            self,
            Self::ProcessDetail { .. } | Self::PgDetail { .. } | Self::PgsDetail { .. }
        )
    }
}

/// State for the PostgreSQL Activity (PGA) tab.
#[derive(Debug)]
pub struct PgActivityTabState {
    pub selected: usize,
    pub filter: Option<String>,
    pub sort_column: usize,
    pub sort_ascending: bool,
    pub hide_idle: bool,
    pub view_mode: PgActivityViewMode,
    pub navigate_to_pid: Option<i32>,
    pub tracked_pid: Option<i32>,
    pub ratatui_state: RatatuiTableState,
    pub last_error: Option<String>,
}

impl Default for PgActivityTabState {
    fn default() -> Self {
        Self {
            selected: 0,
            filter: None,
            sort_column: PgActivityViewMode::Generic.default_sort_column(),
            sort_ascending: false,
            hide_idle: false,
            view_mode: PgActivityViewMode::Generic,
            navigate_to_pid: None,
            tracked_pid: None,
            ratatui_state: RatatuiTableState::default(),
            last_error: None,
        }
    }
}

impl PgActivityTabState {
    pub fn next_sort_column(&mut self) {
        self.sort_column = (self.sort_column + 1) % self.view_mode.column_count();
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_ascending = !self.sort_ascending;
    }

    pub fn select_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.tracked_pid = None;
    }

    pub fn select_down(&mut self) {
        self.selected = self.selected.saturating_add(1);
        self.tracked_pid = None;
    }

    pub fn page_up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
        self.tracked_pid = None;
    }

    pub fn page_down(&mut self, n: usize) {
        self.selected = self.selected.saturating_add(n);
        self.tracked_pid = None;
    }

    pub fn home(&mut self) {
        self.selected = 0;
        self.tracked_pid = None;
    }

    pub fn end(&mut self) {
        self.selected = usize::MAX;
        self.tracked_pid = None;
    }

    /// Resolves selection after filtering/sorting: consumes navigate_to,
    /// applies tracked PID, clamps selected index, and syncs ratatui state.
    /// `row_pids` is the ordered list of PIDs in the current filtered/sorted view.
    pub fn resolve_selection(&mut self, row_pids: &[i32]) {
        // Consume navigate_to_pid
        if let Some(target_pid) = self.navigate_to_pid.take() {
            self.tracked_pid = Some(target_pid);
        }

        // If tracked PID is set, find and select the row with that PID
        if let Some(tracked_pid) = self.tracked_pid {
            if let Some(idx) = row_pids.iter().position(|&pid| pid == tracked_pid) {
                self.selected = idx;
            } else {
                self.tracked_pid = None;
            }
        }

        // Clamp selected index and update tracked PID
        if !row_pids.is_empty() {
            self.selected = self.selected.min(row_pids.len() - 1);
            self.tracked_pid = Some(row_pids[self.selected]);
        } else {
            self.selected = 0;
            self.tracked_pid = None;
        }

        // Sync ratatui TableState for auto-scrolling
        self.ratatui_state.select(Some(self.selected));
    }
}

/// State for the pg_stat_statements (PGS) tab.
#[derive(Debug)]
pub struct PgStatementsTabState {
    pub selected: usize,
    pub filter: Option<String>,
    pub sort_column: usize,
    pub sort_ascending: bool,
    pub view_mode: PgStatementsViewMode,
    pub navigate_to_queryid: Option<i64>,
    pub tracked_queryid: Option<i64>,
    pub ratatui_state: RatatuiTableState,
    // Rate computation state
    pub rates: HashMap<i64, PgStatementsRates>,
    pub prev_sample_ts: Option<i64>,
    pub prev_sample: HashMap<i64, PgStatStatementsInfo>,
    pub last_real_update_ts: Option<i64>,
    pub dt_secs: Option<f64>,
    pub current_collected_at: Option<i64>,
}

impl Default for PgStatementsTabState {
    fn default() -> Self {
        Self {
            selected: 0,
            filter: None,
            sort_column: PgStatementsViewMode::Time.default_sort_column(),
            sort_ascending: false,
            view_mode: PgStatementsViewMode::Time,
            navigate_to_queryid: None,
            tracked_queryid: None,
            ratatui_state: RatatuiTableState::default(),
            rates: HashMap::new(),
            prev_sample_ts: None,
            prev_sample: HashMap::new(),
            last_real_update_ts: None,
            dt_secs: None,
            current_collected_at: None,
        }
    }
}

impl PgStatementsTabState {
    pub fn next_sort_column(&mut self) {
        let count = self.view_mode.column_count();
        if count == 0 {
            return;
        }
        self.sort_column = (self.sort_column + 1) % count;
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_ascending = !self.sort_ascending;
    }

    pub fn select_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.tracked_queryid = None;
    }

    pub fn select_down(&mut self) {
        self.selected = self.selected.saturating_add(1);
        self.tracked_queryid = None;
    }

    pub fn page_up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
        self.tracked_queryid = None;
    }

    pub fn page_down(&mut self, n: usize) {
        self.selected = self.selected.saturating_add(n);
        self.tracked_queryid = None;
    }

    pub fn home(&mut self) {
        self.selected = 0;
        self.tracked_queryid = None;
    }

    pub fn end(&mut self) {
        self.selected = usize::MAX;
        self.tracked_queryid = None;
    }

    /// Resolves selection after filtering/sorting: consumes navigate_to,
    /// applies tracked queryid, clamps selected index, and syncs ratatui state.
    /// `row_queryids` is the ordered list of queryids in the current filtered/sorted view.
    pub fn resolve_selection(&mut self, row_queryids: &[i64]) {
        // Consume navigate_to_queryid
        if let Some(target_qid) = self.navigate_to_queryid.take() {
            self.tracked_queryid = Some(target_qid);
        }

        // If tracked queryid is set, find and select the row
        if let Some(tracked_qid) = self.tracked_queryid {
            if let Some(idx) = row_queryids.iter().position(|&qid| qid == tracked_qid) {
                self.selected = idx;
            } else {
                self.tracked_queryid = None;
            }
        }

        // Clamp selection and update tracked queryid
        if !row_queryids.is_empty() {
            self.selected = self.selected.min(row_queryids.len() - 1);
            self.tracked_queryid = Some(row_queryids[self.selected]);
        } else {
            self.selected = 0;
            self.tracked_queryid = None;
        }

        // Sync ratatui TableState for auto-scrolling
        self.ratatui_state.select(Some(self.selected));
    }

    pub fn reset_rate_state(&mut self) {
        self.rates.clear();
        self.prev_sample_ts = None;
        self.prev_sample.clear();
        self.last_real_update_ts = None;
        self.dt_secs = None;
    }

    pub fn update_rates_from_snapshot(&mut self, snapshot: &Snapshot) {
        let Some(current) = snapshot.blocks.iter().find_map(|b| {
            if let DataBlock::PgStatStatements(v) = b {
                Some(v)
            } else {
                None
            }
        }) else {
            self.reset_rate_state();
            return;
        };

        if current.is_empty() {
            self.reset_rate_state();
            return;
        }

        let now_ts = current
            .first()
            .map(|s| s.collected_at)
            .filter(|&t| t > 0)
            .unwrap_or(snapshot.timestamp);

        self.current_collected_at = Some(now_ts);
        self.last_real_update_ts = Some(now_ts);

        if let Some(prev_ts) = self.prev_sample_ts
            && now_ts < prev_ts
        {
            self.rates.clear();
            self.prev_sample_ts = Some(now_ts);
            self.prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
            return;
        }

        let Some(prev_ts) = self.prev_sample_ts else {
            self.prev_sample_ts = Some(now_ts);
            self.prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
            self.rates.clear();
            return;
        };

        if now_ts == prev_ts {
            return;
        }

        let dt = now_ts.saturating_sub(prev_ts) as f64;
        if dt <= 0.0 {
            self.prev_sample_ts = Some(now_ts);
            self.prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
            self.rates.clear();
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
            let prev = self.prev_sample.get(&s.queryid);
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

        self.rates = rates;
        self.prev_sample_ts = Some(now_ts);
        self.prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
        self.dt_secs = Some(dt);
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

    // ===== Status message =====
    /// Temporary status message shown in the header (e.g., why an action was blocked).
    pub status_message: Option<String>,

    // ===== Ratatui TableState for scrolling =====
    /// Ratatui table state for PRC tab (enables auto-scrolling).
    pub prc_ratatui_state: RatatuiTableState,
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

            status_message: None,

            prc_ratatui_state: RatatuiTableState::default(),
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
        state.pgs.update_rates_from_snapshot(&s1);
        assert!(state.pgs.rates.is_empty(), "first sample is baseline only");

        // collected_at=110: second real sample with new data
        let s2 = snapshot(
            110,
            vec![stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        state.pgs.update_rates_from_snapshot(&s2);

        let r = state.pgs.rates.get(&1).expect("rates should be present");
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
        state.pgs.update_rates_from_snapshot(&s1);

        // collected_at=110: second real sample
        let s2 = snapshot(
            110,
            vec![stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        state.pgs.update_rates_from_snapshot(&s2);
        let baseline_ts = state.pgs.prev_sample_ts;

        // Same collected_at=110 (cached data), later snapshot.timestamp=120:
        // should NOT update baseline, should keep existing rates.
        let s3 = snapshot(
            120,
            vec![stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        state.pgs.update_rates_from_snapshot(&s3);
        assert_eq!(state.pgs.prev_sample_ts, baseline_ts);
        assert!(state.pgs.rates.contains_key(&1));
    }

    #[test]
    fn pgs_rates_handle_stats_reset() {
        let mut state = AppState::new(true);

        let s1 = snapshot(
            100,
            vec![stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        state.pgs.update_rates_from_snapshot(&s1);

        let s2 = snapshot(
            110,
            vec![stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        state.pgs.update_rates_from_snapshot(&s2);

        // Simulate pg_stat_statements reset: counters go down.
        let s_reset = snapshot(120, vec![stmt(1, 5, 50.0, 3, 2, 10, 0, 0, 0, 0, 0, 0, 120)]);
        state.pgs.update_rates_from_snapshot(&s_reset);
        let r = state.pgs.rates.get(&1).expect("rates entry should exist");
        assert_eq!(r.calls_s, None);
        assert_eq!(r.exec_time_ms_s, None);

        // Next sample after reset: deltas should be computed from reset baseline.
        let s_after = snapshot(130, vec![stmt(1, 7, 70.0, 5, 4, 12, 0, 0, 0, 0, 0, 0, 130)]);
        state.pgs.update_rates_from_snapshot(&s_after);
        let r = state.pgs.rates.get(&1).unwrap();
        assert!((r.calls_s.unwrap() - 0.2).abs() < 1e-9);
        assert!((r.exec_time_ms_s.unwrap() - 2.0).abs() < 1e-9);
    }
}
