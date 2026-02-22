//! Per-tab state: PGA (pg_stat_activity), PGS (pg_stat_statements),
//! PGT (pg_stat_user_tables), PGI (pg_stat_user_indexes).

use crate::storage::model::{DataBlock, PgLogSeverity, Snapshot};
use ratatui::widgets::TableState as RatatuiTableState;

use super::{
    PgActivityViewMode, PgIndexesViewMode, PgStatementsViewMode, PgStorePlansViewMode,
    PgTablesViewMode,
};
use crate::tui::navigable::NavigableTable;

/// Generic selection resolution: tracks entity by ID across sort/filter changes.
///
/// - Consumes `navigate_to` (drill-down target) if set.
/// - Finds `tracked` entity in `row_ids`, updating `selected` index.
/// - If entity disappeared, clears `tracked`.
/// - Clamps `selected` to valid bounds and syncs ratatui state.
pub fn resolve_selection_by_id<K: Eq + Copy>(
    selected: &mut usize,
    tracked: &mut Option<K>,
    navigate_to: &mut Option<K>,
    ratatui_state: &mut RatatuiTableState,
    row_ids: &[K],
) {
    if let Some(target) = navigate_to.take() {
        *tracked = Some(target);
    }

    if let Some(t) = *tracked {
        if let Some(idx) = row_ids.iter().position(|id| *id == t) {
            *selected = idx;
        } else {
            *tracked = None;
        }
    }

    if !row_ids.is_empty() {
        *selected = (*selected).min(row_ids.len() - 1);
        *tracked = Some(row_ids[*selected]);
    } else {
        *selected = 0;
        *tracked = None;
    }

    ratatui_state.select(Some(*selected));
}

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

impl NavigableTable for PgLocksTabState {
    fn selected(&self) -> usize {
        self.selected
    }
    fn selected_mut(&mut self) -> &mut usize {
        &mut self.selected
    }
    fn clear_tracked(&mut self) {
        self.tracked_pid = None;
    }
}

impl PgLocksTabState {
    pub fn resolve_selection(&mut self, row_pids: &[i32]) {
        resolve_selection_by_id(
            &mut self.selected,
            &mut self.tracked_pid,
            &mut None,
            &mut self.ratatui_state,
            row_pids,
        );
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

impl NavigableTable for PgErrorsTabState {
    fn selected(&self) -> usize {
        self.selected
    }
    fn selected_mut(&mut self) -> &mut usize {
        &mut self.selected
    }
    fn clear_tracked(&mut self) {
        self.tracked_pattern_hash = None;
    }
}

impl PgErrorsTabState {
    pub fn next_sort_column(&mut self) {
        // 4 columns: SEVERITY, COUNT, PATTERN, SAMPLE
        self.sort_column = (self.sort_column + 1) % 4;
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_ascending = !self.sort_ascending;
    }

    pub fn resolve_selection(&mut self, row_hashes: &[u64]) {
        resolve_selection_by_id(
            &mut self.selected,
            &mut self.tracked_pattern_hash,
            &mut None,
            &mut self.ratatui_state,
            row_hashes,
        );
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
    pub hide_system: bool,
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
            hide_system: true,
            view_mode: PgActivityViewMode::Generic,
            navigate_to_pid: None,
            tracked_pid: None,
            ratatui_state: RatatuiTableState::default(),
            last_error: None,
        }
    }
}

impl NavigableTable for PgActivityTabState {
    fn selected(&self) -> usize {
        self.selected
    }
    fn selected_mut(&mut self) -> &mut usize {
        &mut self.selected
    }
    fn clear_tracked(&mut self) {
        self.tracked_pid = None;
    }
}

impl PgActivityTabState {
    pub fn next_sort_column(&mut self) {
        self.sort_column = (self.sort_column + 1) % self.view_mode.column_count();
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_ascending = !self.sort_ascending;
    }

    pub fn resolve_selection(&mut self, row_pids: &[i32]) {
        resolve_selection_by_id(
            &mut self.selected,
            &mut self.tracked_pid,
            &mut self.navigate_to_pid,
            &mut self.ratatui_state,
            row_pids,
        );
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
    pub rate_state: crate::rates::PgsRateState,
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
            rate_state: crate::rates::PgsRateState::default(),
        }
    }
}

impl NavigableTable for PgStatementsTabState {
    fn selected(&self) -> usize {
        self.selected
    }
    fn selected_mut(&mut self) -> &mut usize {
        &mut self.selected
    }
    fn clear_tracked(&mut self) {
        self.tracked_queryid = None;
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

    pub fn resolve_selection(&mut self, row_queryids: &[i64]) {
        resolve_selection_by_id(
            &mut self.selected,
            &mut self.tracked_queryid,
            &mut self.navigate_to_queryid,
            &mut self.ratatui_state,
            row_queryids,
        );
    }
}

// ===========================================================================
// PGP (pg_store_plans) tab state
// ===========================================================================

/// State for the PostgreSQL Store Plans (PGP) tab.
#[derive(Debug)]
pub struct PgStorePlansTabState {
    pub selected: usize,
    pub filter: Option<String>,
    pub sort_column: usize,
    pub sort_ascending: bool,
    pub view_mode: PgStorePlansViewMode,
    pub tracked_planid: Option<i64>,
    pub ratatui_state: RatatuiTableState,
    pub rate_state: crate::rates::PgpRateState,
}

impl Default for PgStorePlansTabState {
    fn default() -> Self {
        Self {
            selected: 0,
            filter: None,
            sort_column: PgStorePlansViewMode::Time.default_sort_column(),
            sort_ascending: false,
            view_mode: PgStorePlansViewMode::Time,
            tracked_planid: None,
            ratatui_state: RatatuiTableState::default(),
            rate_state: crate::rates::PgpRateState::default(),
        }
    }
}

impl NavigableTable for PgStorePlansTabState {
    fn selected(&self) -> usize {
        self.selected
    }
    fn selected_mut(&mut self) -> &mut usize {
        &mut self.selected
    }
    fn clear_tracked(&mut self) {
        self.tracked_planid = None;
    }
}

impl PgStorePlansTabState {
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

    pub fn resolve_selection(&mut self, row_planids: &[i64]) {
        resolve_selection_by_id(
            &mut self.selected,
            &mut self.tracked_planid,
            &mut None,
            &mut self.ratatui_state,
            row_planids,
        );
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
    pub rate_state: crate::rates::PgtRateState,
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
            rate_state: crate::rates::PgtRateState::default(),
        }
    }
}

impl NavigableTable for PgTablesTabState {
    fn selected(&self) -> usize {
        self.selected
    }
    fn selected_mut(&mut self) -> &mut usize {
        &mut self.selected
    }
    fn clear_tracked(&mut self) {
        self.tracked_relid = None;
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

    pub fn resolve_selection(&mut self, row_relids: &[u32]) {
        resolve_selection_by_id(
            &mut self.selected,
            &mut self.tracked_relid,
            &mut None,
            &mut self.ratatui_state,
            row_relids,
        );
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
    pub rate_state: crate::rates::PgiRateState,
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
            rate_state: crate::rates::PgiRateState::default(),
        }
    }
}

impl NavigableTable for PgIndexesTabState {
    fn selected(&self) -> usize {
        self.selected
    }
    fn selected_mut(&mut self) -> &mut usize {
        &mut self.selected
    }
    fn clear_tracked(&mut self) {
        self.tracked_indexrelid = None;
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

    pub fn resolve_selection(&mut self, row_indexrelids: &[u32]) {
        resolve_selection_by_id(
            &mut self.selected,
            &mut self.tracked_indexrelid,
            &mut self.navigate_to_indexrelid,
            &mut self.ratatui_state,
            row_indexrelids,
        );
    }
}
