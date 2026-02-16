//! Per-tab state: PGA (pg_stat_activity), PGS (pg_stat_statements),
//! PGT (pg_stat_user_tables), PGI (pg_stat_user_indexes).

use crate::storage::model::{
    DataBlock, PgLogSeverity, PgStatStatementsInfo, PgStatUserIndexesInfo, PgStatUserTablesInfo,
    Snapshot,
};
use ratatui::widgets::TableState as RatatuiTableState;
use std::collections::HashMap;
use std::mem;

use super::{
    PgActivityViewMode, PgIndexesRates, PgIndexesViewMode, PgStatementsRates, PgStatementsViewMode,
    PgTablesRates, PgTablesViewMode,
};

// ===========================================================================
// PGL (pg_locks tree) tab state
// ===========================================================================

/// State for the PostgreSQL Locks (PGL) tab.
#[derive(Debug, Default)]
pub struct PgLocksTabState {
    pub selected: usize,
    pub filter: Option<String>,
    pub tracked_pid: Option<i32>,
    pub ratatui_state: RatatuiTableState,
}

impl PgLocksTabState {
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

    /// Resolves selection after filtering: applies tracked PID,
    /// clamps selected index, and syncs ratatui state.
    pub fn resolve_selection(&mut self, row_pids: &[i32]) {
        if let Some(tracked) = self.tracked_pid {
            if let Some(idx) = row_pids.iter().position(|&pid| pid == tracked) {
                self.selected = idx;
            } else {
                self.tracked_pid = None;
            }
        }

        if !row_pids.is_empty() {
            self.selected = self.selected.min(row_pids.len() - 1);
            self.tracked_pid = Some(row_pids[self.selected]);
        } else {
            self.selected = 0;
            self.tracked_pid = None;
        }

        self.ratatui_state.select(Some(self.selected));
    }
}

// ===========================================================================
// PGE (pg_log_errors) tab state
// ===========================================================================

/// Accumulated error pattern within the current hour.
#[derive(Debug, Clone)]
pub struct AccumulatedError {
    pub pattern_hash: u64,
    pub severity: PgLogSeverity,
    pub count: u32,
    pub sample_hash: u64,
    /// Timestamp of last occurrence.
    pub last_seen: i64,
}

/// State for the PostgreSQL Errors (PGE) tab.
#[derive(Debug, Default)]
pub struct PgErrorsTabState {
    pub selected: usize,
    pub filter: Option<String>,
    pub sort_column: usize,
    pub sort_ascending: bool,
    pub tracked_pattern_hash: Option<u64>,
    pub ratatui_state: RatatuiTableState,
    /// Accumulated errors within the current hour.
    pub accumulated: Vec<AccumulatedError>,
    /// Hour boundary (epoch of hour start) for reset detection.
    pub current_hour_start: i64,
}

impl PgErrorsTabState {
    pub fn next_sort_column(&mut self) {
        // 4 columns: SEVERITY, COUNT, PATTERN, SAMPLE
        self.sort_column = (self.sort_column + 1) % 4;
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_ascending = !self.sort_ascending;
    }

    pub fn select_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.tracked_pattern_hash = None;
    }

    pub fn select_down(&mut self) {
        self.selected = self.selected.saturating_add(1);
        self.tracked_pattern_hash = None;
    }

    pub fn page_up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
        self.tracked_pattern_hash = None;
    }

    pub fn page_down(&mut self, n: usize) {
        self.selected = self.selected.saturating_add(n);
        self.tracked_pattern_hash = None;
    }

    pub fn home(&mut self) {
        self.selected = 0;
        self.tracked_pattern_hash = None;
    }

    pub fn end(&mut self) {
        self.selected = usize::MAX;
        self.tracked_pattern_hash = None;
    }

    /// Resolves selection after filtering: applies tracked pattern_hash,
    /// clamps selected index, and syncs ratatui state.
    pub fn resolve_selection(&mut self, row_hashes: &[u64]) {
        if let Some(tracked) = self.tracked_pattern_hash {
            if let Some(idx) = row_hashes.iter().position(|&h| h == tracked) {
                self.selected = idx;
            } else {
                self.tracked_pattern_hash = None;
            }
        }

        if !row_hashes.is_empty() {
            self.selected = self.selected.min(row_hashes.len() - 1);
            self.tracked_pattern_hash = Some(row_hashes[self.selected]);
        } else {
            self.selected = 0;
            self.tracked_pattern_hash = None;
        }

        self.ratatui_state.select(Some(self.selected));
    }

    /// Accumulate errors from a snapshot into the current hour buffer.
    /// Resets accumulator when the hour boundary changes.
    pub fn accumulate_from_snapshot(&mut self, snapshot: &Snapshot) {
        let hour_start = (snapshot.timestamp / 3600) * 3600;
        if hour_start != self.current_hour_start {
            self.accumulated.clear();
            self.current_hour_start = hour_start;
        }

        let entries = snapshot.blocks.iter().find_map(|b| {
            if let DataBlock::PgLogErrors(v) = b {
                Some(v.as_slice())
            } else {
                None
            }
        });

        let Some(entries) = entries else { return };

        for entry in entries {
            if let Some(acc) = self
                .accumulated
                .iter_mut()
                .find(|a| a.pattern_hash == entry.pattern_hash)
            {
                acc.count += entry.count;
                acc.sample_hash = entry.sample_hash;
                acc.last_seen = snapshot.timestamp;
            } else {
                self.accumulated.push(AccumulatedError {
                    pattern_hash: entry.pattern_hash,
                    severity: entry.severity,
                    count: entry.count,
                    sample_hash: entry.sample_hash,
                    last_seen: snapshot.timestamp,
                });
            }
        }
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
    /// Previous-previous sample used for delta display in detail popup.
    /// Contains the sample that was `prev_sample` before the last rate computation.
    pub delta_base: HashMap<i64, PgStatStatementsInfo>,
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
            delta_base: HashMap::new(),
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
        self.delta_base.clear();
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
            self.delta_base = mem::take(&mut self.prev_sample);
            self.prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
            return;
        }

        let Some(prev_ts) = self.prev_sample_ts else {
            self.prev_sample_ts = Some(now_ts);
            self.delta_base = mem::take(&mut self.prev_sample);
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
        self.delta_base = mem::take(&mut self.prev_sample);
        self.prev_sample = current.iter().map(|s| (s.queryid, s.clone())).collect();
        self.dt_secs = Some(dt);
    }
}

#[cfg(test)]
mod pgs_rates_tests {
    use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};
    use crate::tui::state::AppState;

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

// ===========================================================================
// PGT (pg_stat_user_tables) tab state
// ===========================================================================

/// State for the PostgreSQL Tables (PGT) tab.
#[derive(Debug)]
pub struct PgTablesTabState {
    pub selected: usize,
    pub filter: Option<String>,
    pub sort_column: usize,
    pub sort_ascending: bool,
    pub view_mode: PgTablesViewMode,
    pub tracked_relid: Option<u32>,
    pub ratatui_state: RatatuiTableState,
    // Rate computation state
    pub rates: HashMap<u32, PgTablesRates>,
    pub prev_sample_ts: Option<i64>,
    pub prev_sample: HashMap<u32, PgStatUserTablesInfo>,
    /// Contains the sample that was `prev_sample` before the last rate computation.
    /// Used by detail popup to compute deltas (since `prev_sample` is already overwritten
    /// with current data by the time render happens).
    pub delta_base: HashMap<u32, PgStatUserTablesInfo>,
    pub dt_secs: Option<f64>,
}

impl Default for PgTablesTabState {
    fn default() -> Self {
        Self {
            selected: 0,
            filter: None,
            sort_column: PgTablesViewMode::Io.default_sort_column(),
            sort_ascending: false,
            view_mode: PgTablesViewMode::Io,
            tracked_relid: None,
            ratatui_state: RatatuiTableState::default(),
            rates: HashMap::new(),
            prev_sample_ts: None,
            prev_sample: HashMap::new(),
            delta_base: HashMap::new(),
            dt_secs: None,
        }
    }
}

impl PgTablesTabState {
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
        self.tracked_relid = None;
    }

    pub fn select_down(&mut self) {
        self.selected = self.selected.saturating_add(1);
        self.tracked_relid = None;
    }

    pub fn page_up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
        self.tracked_relid = None;
    }

    pub fn page_down(&mut self, n: usize) {
        self.selected = self.selected.saturating_add(n);
        self.tracked_relid = None;
    }

    pub fn home(&mut self) {
        self.selected = 0;
        self.tracked_relid = None;
    }

    pub fn end(&mut self) {
        self.selected = usize::MAX;
        self.tracked_relid = None;
    }

    /// Resolves selection after filtering/sorting.
    pub fn resolve_selection(&mut self, row_relids: &[u32]) {
        if let Some(tracked) = self.tracked_relid {
            if let Some(idx) = row_relids.iter().position(|&r| r == tracked) {
                self.selected = idx;
            } else {
                self.tracked_relid = None;
            }
        }

        if !row_relids.is_empty() {
            self.selected = self.selected.min(row_relids.len() - 1);
            self.tracked_relid = Some(row_relids[self.selected]);
        } else {
            self.selected = 0;
            self.tracked_relid = None;
        }

        self.ratatui_state.select(Some(self.selected));
    }

    pub fn reset_rate_state(&mut self) {
        self.rates.clear();
        self.prev_sample_ts = None;
        self.prev_sample.clear();
        self.delta_base.clear();
        self.dt_secs = None;
    }

    pub fn update_rates_from_snapshot(&mut self, snapshot: &Snapshot) {
        let Some(current) = snapshot.blocks.iter().find_map(|b| {
            if let DataBlock::PgStatUserTables(v) = b {
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

        // Use collected_at from data (not snapshot.timestamp) to compute
        // accurate rates when collector caches pg_stat_user_tables.
        let now_ts = current
            .first()
            .map(|t| t.collected_at)
            .filter(|&t| t > 0)
            .unwrap_or(snapshot.timestamp);

        if let Some(prev_ts) = self.prev_sample_ts {
            if now_ts < prev_ts {
                // Time went backwards — reset baseline
                self.rates.clear();
                self.prev_sample_ts = Some(now_ts);
                self.delta_base = mem::take(&mut self.prev_sample);
                self.prev_sample = current.iter().map(|t| (t.relid, t.clone())).collect();
                return;
            }
            if now_ts == prev_ts {
                return; // Same collected_at, data unchanged — keep existing rates
            }
        }

        let Some(prev_ts) = self.prev_sample_ts else {
            // First sample — just store baseline
            self.prev_sample_ts = Some(now_ts);
            self.delta_base = mem::take(&mut self.prev_sample);
            self.prev_sample = current.iter().map(|t| (t.relid, t.clone())).collect();
            self.rates.clear();
            return;
        };

        let dt = (now_ts - prev_ts) as f64;
        if dt <= 0.0 {
            return;
        }

        fn delta(curr: i64, prev: i64) -> Option<i64> {
            (curr >= prev).then_some(curr - prev)
        }

        let mut rates = HashMap::with_capacity(current.len());
        for t in current {
            let prev = self.prev_sample.get(&t.relid);
            let mut r = PgTablesRates {
                dt_secs: dt,
                ..Default::default()
            };
            if let Some(p) = prev {
                r.seq_scan_s = delta(t.seq_scan, p.seq_scan).map(|d| d as f64 / dt);
                r.seq_tup_read_s = delta(t.seq_tup_read, p.seq_tup_read).map(|d| d as f64 / dt);
                r.idx_scan_s = delta(t.idx_scan, p.idx_scan).map(|d| d as f64 / dt);
                r.idx_tup_fetch_s = delta(t.idx_tup_fetch, p.idx_tup_fetch).map(|d| d as f64 / dt);
                r.n_tup_ins_s = delta(t.n_tup_ins, p.n_tup_ins).map(|d| d as f64 / dt);
                r.n_tup_upd_s = delta(t.n_tup_upd, p.n_tup_upd).map(|d| d as f64 / dt);
                r.n_tup_del_s = delta(t.n_tup_del, p.n_tup_del).map(|d| d as f64 / dt);
                r.n_tup_hot_upd_s = delta(t.n_tup_hot_upd, p.n_tup_hot_upd).map(|d| d as f64 / dt);
                r.vacuum_count_s = delta(t.vacuum_count, p.vacuum_count).map(|d| d as f64 / dt);
                r.autovacuum_count_s =
                    delta(t.autovacuum_count, p.autovacuum_count).map(|d| d as f64 / dt);
                r.heap_blks_read_s =
                    delta(t.heap_blks_read, p.heap_blks_read).map(|d| d as f64 / dt);
                r.heap_blks_hit_s = delta(t.heap_blks_hit, p.heap_blks_hit).map(|d| d as f64 / dt);
                r.idx_blks_read_s = delta(t.idx_blks_read, p.idx_blks_read).map(|d| d as f64 / dt);
                r.idx_blks_hit_s = delta(t.idx_blks_hit, p.idx_blks_hit).map(|d| d as f64 / dt);
            }
            rates.insert(t.relid, r);
        }

        self.rates = rates;
        self.prev_sample_ts = Some(now_ts);
        self.delta_base = mem::take(&mut self.prev_sample);
        self.prev_sample = current.iter().map(|t| (t.relid, t.clone())).collect();
        self.dt_secs = Some(dt);
    }
}

// ===========================================================================
// PGI (pg_stat_user_indexes) tab state
// ===========================================================================

/// State for the PostgreSQL Indexes (PGI) tab.
#[derive(Debug)]
pub struct PgIndexesTabState {
    pub selected: usize,
    pub filter: Option<String>,
    /// When set, only show indexes belonging to this table (drill-down from PGT).
    pub filter_relid: Option<u32>,
    pub sort_column: usize,
    pub sort_ascending: bool,
    pub view_mode: PgIndexesViewMode,
    pub tracked_indexrelid: Option<u32>,
    pub navigate_to_indexrelid: Option<u32>,
    pub ratatui_state: RatatuiTableState,
    // Rate computation state
    pub rates: HashMap<u32, PgIndexesRates>,
    pub prev_sample_ts: Option<i64>,
    pub prev_sample: HashMap<u32, PgStatUserIndexesInfo>,
    /// Contains the sample that was `prev_sample` before the last rate computation.
    /// Used by detail popup to compute deltas.
    pub delta_base: HashMap<u32, PgStatUserIndexesInfo>,
    pub dt_secs: Option<f64>,
}

impl Default for PgIndexesTabState {
    fn default() -> Self {
        Self {
            selected: 0,
            filter: None,
            filter_relid: None,
            sort_column: PgIndexesViewMode::Io.default_sort_column(),
            sort_ascending: false,
            view_mode: PgIndexesViewMode::Io,
            tracked_indexrelid: None,
            navigate_to_indexrelid: None,
            ratatui_state: RatatuiTableState::default(),
            rates: HashMap::new(),
            prev_sample_ts: None,
            prev_sample: HashMap::new(),
            delta_base: HashMap::new(),
            dt_secs: None,
        }
    }
}

impl PgIndexesTabState {
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
        self.tracked_indexrelid = None;
    }

    pub fn select_down(&mut self) {
        self.selected = self.selected.saturating_add(1);
        self.tracked_indexrelid = None;
    }

    pub fn page_up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
        self.tracked_indexrelid = None;
    }

    pub fn page_down(&mut self, n: usize) {
        self.selected = self.selected.saturating_add(n);
        self.tracked_indexrelid = None;
    }

    pub fn home(&mut self) {
        self.selected = 0;
        self.tracked_indexrelid = None;
    }

    pub fn end(&mut self) {
        self.selected = usize::MAX;
        self.tracked_indexrelid = None;
    }

    /// Resolves selection after filtering/sorting.
    pub fn resolve_selection(&mut self, row_indexrelids: &[u32]) {
        // Consume navigate_to
        if let Some(target) = self.navigate_to_indexrelid.take() {
            self.tracked_indexrelid = Some(target);
        }

        if let Some(tracked) = self.tracked_indexrelid {
            if let Some(idx) = row_indexrelids.iter().position(|&r| r == tracked) {
                self.selected = idx;
            } else {
                self.tracked_indexrelid = None;
            }
        }

        if !row_indexrelids.is_empty() {
            self.selected = self.selected.min(row_indexrelids.len() - 1);
            self.tracked_indexrelid = Some(row_indexrelids[self.selected]);
        } else {
            self.selected = 0;
            self.tracked_indexrelid = None;
        }

        self.ratatui_state.select(Some(self.selected));
    }

    pub fn reset_rate_state(&mut self) {
        self.rates.clear();
        self.prev_sample_ts = None;
        self.prev_sample.clear();
        self.delta_base.clear();
        self.dt_secs = None;
    }

    pub fn update_rates_from_snapshot(&mut self, snapshot: &Snapshot) {
        let Some(current) = snapshot.blocks.iter().find_map(|b| {
            if let DataBlock::PgStatUserIndexes(v) = b {
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

        // Use collected_at from data (not snapshot.timestamp) to compute
        // accurate rates when collector caches pg_stat_user_indexes.
        let now_ts = current
            .first()
            .map(|i| i.collected_at)
            .filter(|&t| t > 0)
            .unwrap_or(snapshot.timestamp);

        if let Some(prev_ts) = self.prev_sample_ts {
            if now_ts < prev_ts {
                self.rates.clear();
                self.prev_sample_ts = Some(now_ts);
                self.delta_base = mem::take(&mut self.prev_sample);
                self.prev_sample = current.iter().map(|i| (i.indexrelid, i.clone())).collect();
                return;
            }
            if now_ts == prev_ts {
                return; // Same collected_at, data unchanged — keep existing rates
            }
        }

        let Some(prev_ts) = self.prev_sample_ts else {
            self.prev_sample_ts = Some(now_ts);
            self.delta_base = mem::take(&mut self.prev_sample);
            self.prev_sample = current.iter().map(|i| (i.indexrelid, i.clone())).collect();
            self.rates.clear();
            return;
        };

        let dt = (now_ts - prev_ts) as f64;
        if dt <= 0.0 {
            return;
        }

        fn delta(curr: i64, prev: i64) -> Option<i64> {
            (curr >= prev).then_some(curr - prev)
        }

        let mut rates = HashMap::with_capacity(current.len());
        for idx in current {
            let prev = self.prev_sample.get(&idx.indexrelid);
            let mut r = PgIndexesRates {
                dt_secs: dt,
                ..Default::default()
            };
            if let Some(p) = prev {
                r.idx_scan_s = delta(idx.idx_scan, p.idx_scan).map(|d| d as f64 / dt);
                r.idx_tup_read_s = delta(idx.idx_tup_read, p.idx_tup_read).map(|d| d as f64 / dt);
                r.idx_tup_fetch_s =
                    delta(idx.idx_tup_fetch, p.idx_tup_fetch).map(|d| d as f64 / dt);
                r.idx_blks_read_s =
                    delta(idx.idx_blks_read, p.idx_blks_read).map(|d| d as f64 / dt);
                r.idx_blks_hit_s = delta(idx.idx_blks_hit, p.idx_blks_hit).map(|d| d as f64 / dt);
            }
            rates.insert(idx.indexrelid, r);
        }

        self.rates = rates;
        self.prev_sample_ts = Some(now_ts);
        self.delta_base = mem::take(&mut self.prev_sample);
        self.prev_sample = current.iter().map(|i| (i.indexrelid, i.clone())).collect();
        self.dt_secs = Some(dt);
    }
}
