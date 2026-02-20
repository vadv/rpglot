//! Shared rate computation for pg_stat_statements, pg_store_plans,
//! pg_stat_user_tables, and pg_stat_user_indexes.
//!
//! This module is the **single source of truth** for rate computation logic.
//! Both the TUI and Web frontends delegate to these functions.

use std::collections::HashMap;

use crate::models::{PgIndexesRates, PgStatementsRates, PgStorePlansRates, PgTablesRates};
use crate::storage::model::{
    DataBlock, PgStatStatementsInfo, PgStatUserIndexesInfo, PgStatUserTablesInfo, PgStorePlansInfo,
    Snapshot,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum dt (seconds) for PGS/PGT/PGI rate computation (~30s collection cache).
/// Allows up to ~20 missed collection cycles (20 × 30s) + 5s tolerance.
pub const MAX_RATE_DT_SECS: f64 = 605.0;

/// Maximum dt (seconds) for PGP rate computation (pg_store_plans, 300s collection interval).
/// Allows up to two missed collection cycles (3 × 300s + 5s tolerance).
pub const MAX_PGP_RATE_DT_SECS: f64 = 905.0;

/// Maximum age (seconds) for stale PGS entries in prev_sample (10× 30s cache).
pub const MAX_PGS_STALE_SECS: i64 = 300;

/// Maximum age (seconds) for stale PGP entries in prev_sample (3× 300s cache).
pub const MAX_PGP_STALE_SECS: i64 = 900;

// ---------------------------------------------------------------------------
// Delta helpers
// ---------------------------------------------------------------------------

/// Compute i64 delta, returning `None` on counter regression (stats reset).
pub fn di64(curr: i64, prev: i64) -> Option<i64> {
    (curr >= prev).then_some(curr - prev)
}

/// Compute f64 delta, returning `None` on counter regression (stats reset).
pub fn df64(curr: f64, prev: f64) -> Option<f64> {
    (curr >= prev).then_some(curr - prev)
}

// ---------------------------------------------------------------------------
// Rate state structs
// ---------------------------------------------------------------------------

/// Rate tracking state for pg_stat_statements.
#[derive(Debug, Default)]
pub struct PgsRateState {
    pub rates: HashMap<i64, PgStatementsRates>,
    pub prev_sample: HashMap<i64, PgStatStatementsInfo>,
    pub prev_ts: Option<i64>,
}

impl PgsRateState {
    pub fn reset(&mut self) {
        self.rates.clear();
        self.prev_sample.clear();
        self.prev_ts = None;
    }

    pub fn shrink_to_fit(&mut self) {
        self.rates.shrink_to_fit();
        self.prev_sample.shrink_to_fit();
    }
}

/// Rate tracking state for pg_store_plans.
#[derive(Debug, Default)]
pub struct PgpRateState {
    pub rates: HashMap<i64, PgStorePlansRates>,
    pub prev_sample: HashMap<i64, PgStorePlansInfo>,
    pub prev_ts: Option<i64>,
}

impl PgpRateState {
    pub fn reset(&mut self) {
        self.rates.clear();
        self.prev_sample.clear();
        self.prev_ts = None;
    }

    pub fn shrink_to_fit(&mut self) {
        self.rates.shrink_to_fit();
        self.prev_sample.shrink_to_fit();
    }
}

/// Rate tracking state for pg_stat_user_tables.
#[derive(Debug, Default)]
pub struct PgtRateState {
    pub rates: HashMap<u32, PgTablesRates>,
    pub prev_sample: HashMap<u32, PgStatUserTablesInfo>,
    pub prev_ts: Option<i64>,
}

impl PgtRateState {
    pub fn reset(&mut self) {
        self.rates.clear();
        self.prev_sample.clear();
        self.prev_ts = None;
    }

    pub fn shrink_to_fit(&mut self) {
        self.rates.shrink_to_fit();
        self.prev_sample.shrink_to_fit();
    }
}

/// Rate tracking state for pg_stat_user_indexes.
#[derive(Debug, Default)]
pub struct PgiRateState {
    pub rates: HashMap<u32, PgIndexesRates>,
    pub prev_sample: HashMap<u32, PgStatUserIndexesInfo>,
    pub prev_ts: Option<i64>,
}

impl PgiRateState {
    pub fn reset(&mut self) {
        self.rates.clear();
        self.prev_sample.clear();
        self.prev_ts = None;
    }

    pub fn shrink_to_fit(&mut self) {
        self.rates.shrink_to_fit();
        self.prev_sample.shrink_to_fit();
    }
}

// ---------------------------------------------------------------------------
// PGS rate computation
// ---------------------------------------------------------------------------

/// Update PGS (pg_stat_statements) rates from a snapshot.
///
/// Uses merge-based prev_sample update with stale eviction.
/// Caps dt at [`MAX_RATE_DT_SECS`] to prevent garbage rates after long gaps.
pub fn update_pgs_rates(state: &mut PgsRateState, snapshot: &Snapshot) {
    let Some(stmts) = snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatStatements(v) = b {
            Some(v)
        } else {
            None
        }
    }) else {
        state.rates.clear();
        return;
    };

    if stmts.is_empty() {
        state.rates.clear();
        return;
    }

    let now_ts = stmts
        .first()
        .map(|s| s.collected_at)
        .filter(|&t| t > 0)
        .unwrap_or(snapshot.timestamp);

    let Some(prev_ts) = state.prev_ts else {
        state.prev_ts = Some(now_ts);
        state.prev_sample = stmts.iter().map(|s| (s.queryid, s.clone())).collect();
        state.rates.clear();
        return;
    };

    if now_ts == prev_ts {
        return;
    }

    if now_ts < prev_ts {
        state.prev_ts = Some(now_ts);
        state.prev_sample = stmts.iter().map(|s| (s.queryid, s.clone())).collect();
        state.rates.clear();
        return;
    }

    let dt = (now_ts - prev_ts) as f64;

    if dt > MAX_RATE_DT_SECS {
        state.prev_ts = Some(now_ts);
        state.prev_sample = stmts.iter().map(|s| (s.queryid, s.clone())).collect();
        state.rates.clear();
        return;
    }

    let mut rates = HashMap::with_capacity(stmts.len());
    for s in stmts {
        let mut r = PgStatementsRates {
            dt_secs: dt,
            ..Default::default()
        };
        if let Some(prev) = state.prev_sample.get(&s.queryid) {
            r.calls_s = di64(s.calls, prev.calls).map(|d| d as f64 / dt);
            r.rows_s = di64(s.rows, prev.rows).map(|d| d as f64 / dt);
            r.exec_time_ms_s = df64(s.total_exec_time, prev.total_exec_time).map(|d| d / dt);
            r.shared_blks_read_s =
                di64(s.shared_blks_read, prev.shared_blks_read).map(|d| d as f64 / dt);
            r.shared_blks_hit_s =
                di64(s.shared_blks_hit, prev.shared_blks_hit).map(|d| d as f64 / dt);
            r.shared_blks_dirtied_s =
                di64(s.shared_blks_dirtied, prev.shared_blks_dirtied).map(|d| d as f64 / dt);
            r.shared_blks_written_s =
                di64(s.shared_blks_written, prev.shared_blks_written).map(|d| d as f64 / dt);
            r.local_blks_read_s =
                di64(s.local_blks_read, prev.local_blks_read).map(|d| d as f64 / dt);
            r.local_blks_written_s =
                di64(s.local_blks_written, prev.local_blks_written).map(|d| d as f64 / dt);
            r.temp_blks_read_s = di64(s.temp_blks_read, prev.temp_blks_read).map(|d| d as f64 / dt);
            r.temp_blks_written_s =
                di64(s.temp_blks_written, prev.temp_blks_written).map(|d| d as f64 / dt);
            if let (Some(dr), Some(dw)) = (
                di64(s.temp_blks_read, prev.temp_blks_read),
                di64(s.temp_blks_written, prev.temp_blks_written),
            ) {
                r.temp_mb_s = Some(((dr + dw) as f64 * 8.0 / 1024.0) / dt);
            }
        }
        rates.insert(s.queryid, r);
    }

    state.rates = rates;
    state.prev_ts = Some(now_ts);
    // Merge instead of full replace — keep stale entries for display
    for s in stmts {
        state.prev_sample.insert(s.queryid, s.clone());
    }
    // Evict entries older than MAX_PGS_STALE_SECS
    state
        .prev_sample
        .retain(|_, s| s.collected_at >= now_ts - MAX_PGS_STALE_SECS);
}

// ---------------------------------------------------------------------------
// PGP rate computation
// ---------------------------------------------------------------------------

/// Update PGP (pg_store_plans) rates from a snapshot.
///
/// Uses merge-based prev_sample update with stale eviction.
/// Caps dt at [`MAX_PGP_RATE_DT_SECS`].
pub fn update_pgp_rates(state: &mut PgpRateState, snapshot: &Snapshot) {
    let Some(plans) = snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStorePlans(v) = b {
            Some(v)
        } else {
            None
        }
    }) else {
        state.rates.clear();
        return;
    };

    if plans.is_empty() {
        state.rates.clear();
        return;
    }

    let now_ts = plans
        .first()
        .map(|p| p.collected_at)
        .filter(|&t| t > 0)
        .unwrap_or(snapshot.timestamp);

    let Some(prev_ts) = state.prev_ts else {
        state.prev_ts = Some(now_ts);
        state.prev_sample = plans.iter().map(|p| (p.planid, p.clone())).collect();
        state.rates.clear();
        return;
    };

    if now_ts == prev_ts {
        return;
    }

    if now_ts < prev_ts {
        state.prev_ts = Some(now_ts);
        state.prev_sample = plans.iter().map(|p| (p.planid, p.clone())).collect();
        state.rates.clear();
        return;
    }

    let dt = (now_ts - prev_ts) as f64;

    if dt > MAX_PGP_RATE_DT_SECS {
        state.prev_ts = Some(now_ts);
        state.prev_sample = plans.iter().map(|p| (p.planid, p.clone())).collect();
        state.rates.clear();
        return;
    }

    let mut rates = HashMap::with_capacity(plans.len());
    for p in plans {
        let mut r = PgStorePlansRates {
            dt_secs: dt,
            ..Default::default()
        };
        if let Some(prev) = state.prev_sample.get(&p.planid) {
            r.calls_s = di64(p.calls, prev.calls).map(|d| d as f64 / dt);
            r.rows_s = di64(p.rows, prev.rows).map(|d| d as f64 / dt);
            r.exec_time_ms_s = df64(p.total_time, prev.total_time).map(|d| d / dt);
            r.shared_blks_read_s =
                di64(p.shared_blks_read, prev.shared_blks_read).map(|d| d as f64 / dt);
            r.shared_blks_hit_s =
                di64(p.shared_blks_hit, prev.shared_blks_hit).map(|d| d as f64 / dt);
            r.shared_blks_dirtied_s =
                di64(p.shared_blks_dirtied, prev.shared_blks_dirtied).map(|d| d as f64 / dt);
            r.shared_blks_written_s =
                di64(p.shared_blks_written, prev.shared_blks_written).map(|d| d as f64 / dt);
            r.temp_blks_read_s = di64(p.temp_blks_read, prev.temp_blks_read).map(|d| d as f64 / dt);
            r.temp_blks_written_s =
                di64(p.temp_blks_written, prev.temp_blks_written).map(|d| d as f64 / dt);
        }
        rates.insert(p.planid, r);
    }

    state.rates = rates;
    state.prev_ts = Some(now_ts);
    // Merge instead of full replace — keep stale entries for display
    for p in plans {
        state.prev_sample.insert(p.planid, p.clone());
    }
    // Evict entries older than MAX_PGP_STALE_SECS
    state
        .prev_sample
        .retain(|_, p| p.collected_at >= now_ts - MAX_PGP_STALE_SECS);
}

// ---------------------------------------------------------------------------
// PGT rate computation
// ---------------------------------------------------------------------------

/// Update PGT (pg_stat_user_tables) rates from a snapshot.
///
/// Full prev_sample replacement (no stale tracking).
/// No MAX_RATE_DT cap — tables are refreshed reliably.
pub fn update_pgt_rates(state: &mut PgtRateState, snapshot: &Snapshot) {
    let Some(tables) = snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserTables(v) = b {
            Some(v)
        } else {
            None
        }
    }) else {
        state.rates.clear();
        return;
    };

    let now_ts = tables
        .first()
        .map(|t| t.collected_at)
        .filter(|&t| t > 0)
        .unwrap_or(snapshot.timestamp);

    let Some(prev_ts) = state.prev_ts else {
        state.prev_ts = Some(now_ts);
        state.prev_sample = tables.iter().map(|t| (t.relid, t.clone())).collect();
        state.rates.clear();
        return;
    };

    if now_ts == prev_ts {
        return; // Same collected_at, data unchanged
    }

    let dt = (now_ts - prev_ts) as f64;
    if dt <= 0.0 {
        return;
    }

    let mut rates = HashMap::with_capacity(tables.len());
    for t in tables {
        let mut r = PgTablesRates {
            dt_secs: dt,
            ..Default::default()
        };
        if let Some(prev) = state.prev_sample.get(&t.relid) {
            r.seq_scan_s = di64(t.seq_scan, prev.seq_scan).map(|d| d as f64 / dt);
            r.seq_tup_read_s = di64(t.seq_tup_read, prev.seq_tup_read).map(|d| d as f64 / dt);
            r.idx_scan_s = di64(t.idx_scan, prev.idx_scan).map(|d| d as f64 / dt);
            r.idx_tup_fetch_s = di64(t.idx_tup_fetch, prev.idx_tup_fetch).map(|d| d as f64 / dt);
            r.n_tup_ins_s = di64(t.n_tup_ins, prev.n_tup_ins).map(|d| d as f64 / dt);
            r.n_tup_upd_s = di64(t.n_tup_upd, prev.n_tup_upd).map(|d| d as f64 / dt);
            r.n_tup_del_s = di64(t.n_tup_del, prev.n_tup_del).map(|d| d as f64 / dt);
            r.n_tup_hot_upd_s = di64(t.n_tup_hot_upd, prev.n_tup_hot_upd).map(|d| d as f64 / dt);
            r.vacuum_count_s = di64(t.vacuum_count, prev.vacuum_count).map(|d| d as f64 / dt);
            r.autovacuum_count_s =
                di64(t.autovacuum_count, prev.autovacuum_count).map(|d| d as f64 / dt);
            r.heap_blks_read_s = di64(t.heap_blks_read, prev.heap_blks_read).map(|d| d as f64 / dt);
            r.heap_blks_hit_s = di64(t.heap_blks_hit, prev.heap_blks_hit).map(|d| d as f64 / dt);
            r.idx_blks_read_s = di64(t.idx_blks_read, prev.idx_blks_read).map(|d| d as f64 / dt);
            r.idx_blks_hit_s = di64(t.idx_blks_hit, prev.idx_blks_hit).map(|d| d as f64 / dt);
            r.toast_blks_read_s =
                di64(t.toast_blks_read, prev.toast_blks_read).map(|d| d as f64 / dt);
            r.toast_blks_hit_s = di64(t.toast_blks_hit, prev.toast_blks_hit).map(|d| d as f64 / dt);
            r.tidx_blks_read_s = di64(t.tidx_blks_read, prev.tidx_blks_read).map(|d| d as f64 / dt);
            r.tidx_blks_hit_s = di64(t.tidx_blks_hit, prev.tidx_blks_hit).map(|d| d as f64 / dt);
            r.analyze_count_s = di64(t.analyze_count, prev.analyze_count).map(|d| d as f64 / dt);
            r.autoanalyze_count_s =
                di64(t.autoanalyze_count, prev.autoanalyze_count).map(|d| d as f64 / dt);
        }
        rates.insert(t.relid, r);
    }

    state.rates = rates;
    state.prev_ts = Some(now_ts);
    state.prev_sample = tables.iter().map(|t| (t.relid, t.clone())).collect();
}

// ---------------------------------------------------------------------------
// PGI rate computation
// ---------------------------------------------------------------------------

/// Update PGI (pg_stat_user_indexes) rates from a snapshot.
///
/// Full prev_sample replacement (no stale tracking).
/// No MAX_RATE_DT cap.
pub fn update_pgi_rates(state: &mut PgiRateState, snapshot: &Snapshot) {
    let Some(indexes) = snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserIndexes(v) = b {
            Some(v)
        } else {
            None
        }
    }) else {
        state.rates.clear();
        return;
    };

    let now_ts = indexes
        .first()
        .map(|i| i.collected_at)
        .filter(|&t| t > 0)
        .unwrap_or(snapshot.timestamp);

    let Some(prev_ts) = state.prev_ts else {
        state.prev_ts = Some(now_ts);
        state.prev_sample = indexes.iter().map(|i| (i.indexrelid, i.clone())).collect();
        state.rates.clear();
        return;
    };

    if now_ts == prev_ts {
        return; // Same collected_at, data unchanged
    }

    let dt = (now_ts - prev_ts) as f64;
    if dt <= 0.0 {
        return;
    }

    let mut rates = HashMap::with_capacity(indexes.len());
    for i in indexes {
        let mut r = PgIndexesRates {
            dt_secs: dt,
            ..Default::default()
        };
        if let Some(prev) = state.prev_sample.get(&i.indexrelid) {
            r.idx_scan_s = di64(i.idx_scan, prev.idx_scan).map(|d| d as f64 / dt);
            r.idx_tup_read_s = di64(i.idx_tup_read, prev.idx_tup_read).map(|d| d as f64 / dt);
            r.idx_tup_fetch_s = di64(i.idx_tup_fetch, prev.idx_tup_fetch).map(|d| d as f64 / dt);
            r.idx_blks_read_s = di64(i.idx_blks_read, prev.idx_blks_read).map(|d| d as f64 / dt);
            r.idx_blks_hit_s = di64(i.idx_blks_hit, prev.idx_blks_hit).map(|d| d as f64 / dt);
        }
        rates.insert(i.indexrelid, r);
    }

    state.rates = rates;
    state.prev_ts = Some(now_ts);
    state.prev_sample = indexes.iter().map(|i| (i.indexrelid, i.clone())).collect();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::{
        DataBlock, PgStatStatementsInfo, PgStatUserIndexesInfo, PgStatUserTablesInfo,
        PgStorePlansInfo, Snapshot,
    };

    // -- helpers --

    fn pgs_stmt(
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

    fn pgs_snapshot(ts: i64, stmts: Vec<PgStatStatementsInfo>) -> Snapshot {
        Snapshot {
            timestamp: ts,
            blocks: vec![DataBlock::PgStatStatements(stmts)],
        }
    }

    fn pgp_plan(
        planid: i64,
        calls: i64,
        total_time: f64,
        rows: i64,
        collected_at: i64,
    ) -> PgStorePlansInfo {
        PgStorePlansInfo {
            planid,
            calls,
            total_time,
            rows,
            collected_at,
            ..Default::default()
        }
    }

    fn pgp_snapshot(ts: i64, plans: Vec<PgStorePlansInfo>) -> Snapshot {
        Snapshot {
            timestamp: ts,
            blocks: vec![DataBlock::PgStorePlans(plans)],
        }
    }

    fn pgt_table(
        relid: u32,
        seq_scan: i64,
        idx_scan: i64,
        n_tup_ins: i64,
        heap_blks_read: i64,
        toast_blks_read: i64,
        analyze_count: i64,
        collected_at: i64,
    ) -> PgStatUserTablesInfo {
        PgStatUserTablesInfo {
            relid,
            seq_scan,
            idx_scan,
            n_tup_ins,
            heap_blks_read,
            toast_blks_read,
            analyze_count,
            collected_at,
            ..Default::default()
        }
    }

    fn pgt_snapshot(ts: i64, tables: Vec<PgStatUserTablesInfo>) -> Snapshot {
        Snapshot {
            timestamp: ts,
            blocks: vec![DataBlock::PgStatUserTables(tables)],
        }
    }

    fn pgi_index(
        indexrelid: u32,
        idx_scan: i64,
        idx_tup_read: i64,
        collected_at: i64,
    ) -> PgStatUserIndexesInfo {
        PgStatUserIndexesInfo {
            indexrelid,
            idx_scan,
            idx_tup_read,
            collected_at,
            ..Default::default()
        }
    }

    fn pgi_snapshot(ts: i64, indexes: Vec<PgStatUserIndexesInfo>) -> Snapshot {
        Snapshot {
            timestamp: ts,
            blocks: vec![DataBlock::PgStatUserIndexes(indexes)],
        }
    }

    // ===== PGS tests =====

    #[test]
    fn pgs_first_sample_is_baseline() {
        let mut st = PgsRateState::default();
        let s = pgs_snapshot(
            100,
            vec![pgs_stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        update_pgs_rates(&mut st, &s);
        assert!(st.rates.is_empty());
        assert_eq!(st.prev_ts, Some(100));
    }

    #[test]
    fn pgs_rates_computed_on_second_sample() {
        let mut st = PgsRateState::default();
        let s1 = pgs_snapshot(
            100,
            vec![pgs_stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        update_pgs_rates(&mut st, &s1);

        let s2 = pgs_snapshot(
            110,
            vec![pgs_stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        update_pgs_rates(&mut st, &s2);

        let r = st.rates.get(&1).expect("rates should exist");
        assert!((r.dt_secs - 10.0).abs() < 1e-9);
        assert!((r.calls_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.rows_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.exec_time_ms_s.unwrap() - 10.0).abs() < 1e-9);
        assert!((r.shared_blks_read_s.unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn pgs_same_collected_at_skips_update() {
        let mut st = PgsRateState::default();
        let s1 = pgs_snapshot(
            100,
            vec![pgs_stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        update_pgs_rates(&mut st, &s1);

        let s2 = pgs_snapshot(
            110,
            vec![pgs_stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        update_pgs_rates(&mut st, &s2);
        let prev_ts = st.prev_ts;

        // Same collected_at=110, different snapshot.timestamp=120
        let s3 = pgs_snapshot(
            120,
            vec![pgs_stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        update_pgs_rates(&mut st, &s3);
        assert_eq!(st.prev_ts, prev_ts);
        assert!(st.rates.contains_key(&1));
    }

    #[test]
    fn pgs_counter_regression_yields_none() {
        let mut st = PgsRateState::default();
        let s1 = pgs_snapshot(
            100,
            vec![pgs_stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        update_pgs_rates(&mut st, &s1);

        let s2 = pgs_snapshot(
            110,
            vec![pgs_stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        update_pgs_rates(&mut st, &s2);

        // Counter regression (pg_stat_statements_reset)
        let s3 = pgs_snapshot(
            120,
            vec![pgs_stmt(1, 5, 50.0, 3, 2, 10, 0, 0, 0, 0, 0, 0, 120)],
        );
        update_pgs_rates(&mut st, &s3);
        let r = st.rates.get(&1).expect("entry should exist");
        assert_eq!(r.calls_s, None);
        assert_eq!(r.exec_time_ms_s, None);

        // Recovery after reset
        let s4 = pgs_snapshot(
            130,
            vec![pgs_stmt(1, 7, 70.0, 5, 4, 12, 0, 0, 0, 0, 0, 0, 130)],
        );
        update_pgs_rates(&mut st, &s4);
        let r = st.rates.get(&1).unwrap();
        assert!((r.calls_s.unwrap() - 0.2).abs() < 1e-9);
        assert!((r.exec_time_ms_s.unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn pgs_time_regression_clears_rates() {
        let mut st = PgsRateState::default();
        let s1 = pgs_snapshot(
            100,
            vec![pgs_stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        update_pgs_rates(&mut st, &s1);
        let s2 = pgs_snapshot(
            110,
            vec![pgs_stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        update_pgs_rates(&mut st, &s2);
        assert!(!st.rates.is_empty());

        // Time goes backwards
        let s3 = pgs_snapshot(
            90,
            vec![pgs_stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 90)],
        );
        update_pgs_rates(&mut st, &s3);
        assert!(st.rates.is_empty());
        assert_eq!(st.prev_ts, Some(90));
    }

    #[test]
    fn pgs_max_dt_cap_resets() {
        let mut st = PgsRateState::default();
        let s1 = pgs_snapshot(
            100,
            vec![pgs_stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100)],
        );
        update_pgs_rates(&mut st, &s1);

        // dt = 700s > MAX_RATE_DT_SECS (605s) → reset
        let s2 = pgs_snapshot(
            800,
            vec![pgs_stmt(
                1, 100, 1000.0, 50, 100, 900, 10, 0, 0, 0, 0, 0, 800,
            )],
        );
        update_pgs_rates(&mut st, &s2);
        assert!(st.rates.is_empty());
        assert_eq!(st.prev_ts, Some(800));
    }

    #[test]
    fn pgs_stale_eviction() {
        let mut st = PgsRateState::default();
        // Two entries, both at collected_at=100
        let s1 = pgs_snapshot(
            100,
            vec![
                pgs_stmt(1, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100),
                pgs_stmt(2, 10, 100.0, 5, 10, 90, 1, 0, 0, 0, 0, 0, 100),
            ],
        );
        update_pgs_rates(&mut st, &s1);

        // Only queryid=1 appears in second snapshot
        let s2 = pgs_snapshot(
            110,
            vec![pgs_stmt(1, 20, 200.0, 15, 30, 110, 3, 1, 0, 0, 0, 0, 110)],
        );
        update_pgs_rates(&mut st, &s2);
        // queryid=2 should still be in prev_sample (merged, not evicted yet, age=10s < 300s)
        assert!(st.prev_sample.contains_key(&2));

        // Jump to collected_at=500 (age of queryid=2 entry is now 400s > MAX_PGS_STALE_SECS=300)
        let s3 = pgs_snapshot(
            500,
            vec![pgs_stmt(1, 30, 300.0, 25, 50, 130, 5, 2, 0, 0, 0, 0, 500)],
        );
        update_pgs_rates(&mut st, &s3);
        // queryid=2 should be evicted
        assert!(!st.prev_sample.contains_key(&2));
        // queryid=1 should still be there
        assert!(st.prev_sample.contains_key(&1));
    }

    #[test]
    fn pgs_temp_mb_s_computed() {
        let mut st = PgsRateState::default();
        let s1 = pgs_snapshot(
            100,
            vec![pgs_stmt(1, 10, 100.0, 5, 0, 0, 0, 0, 0, 0, 100, 200, 100)],
        );
        update_pgs_rates(&mut st, &s1);

        let s2 = pgs_snapshot(
            110,
            vec![pgs_stmt(1, 20, 200.0, 15, 0, 0, 0, 0, 0, 0, 200, 400, 110)],
        );
        update_pgs_rates(&mut st, &s2);

        let r = st.rates.get(&1).unwrap();
        // delta_read=100, delta_write=200, total=300 blocks, each 8KB = 2400KB = 2400/1024 MB ≈ 2.34375 MB over 10s
        let expected = (300.0 * 8.0 / 1024.0) / 10.0;
        assert!((r.temp_mb_s.unwrap() - expected).abs() < 1e-9);
    }

    // ===== PGP tests =====

    #[test]
    fn pgp_first_sample_is_baseline() {
        let mut st = PgpRateState::default();
        let s = pgp_snapshot(100, vec![pgp_plan(1, 10, 100.0, 5, 100)]);
        update_pgp_rates(&mut st, &s);
        assert!(st.rates.is_empty());
        assert_eq!(st.prev_ts, Some(100));
    }

    #[test]
    fn pgp_rates_computed() {
        let mut st = PgpRateState::default();
        let s1 = pgp_snapshot(100, vec![pgp_plan(1, 10, 100.0, 5, 100)]);
        update_pgp_rates(&mut st, &s1);

        let s2 = pgp_snapshot(400, vec![pgp_plan(1, 20, 200.0, 15, 400)]);
        update_pgp_rates(&mut st, &s2);

        let r = st.rates.get(&1).unwrap();
        assert!((r.dt_secs - 300.0).abs() < 1e-9);
        assert!((r.calls_s.unwrap() - 10.0 / 300.0).abs() < 1e-9);
    }

    #[test]
    fn pgp_max_dt_cap() {
        let mut st = PgpRateState::default();
        let s1 = pgp_snapshot(100, vec![pgp_plan(1, 10, 100.0, 5, 100)]);
        update_pgp_rates(&mut st, &s1);

        // dt = 1000s > MAX_PGP_RATE_DT_SECS (905s)
        let s2 = pgp_snapshot(1100, vec![pgp_plan(1, 100, 1000.0, 50, 1100)]);
        update_pgp_rates(&mut st, &s2);
        assert!(st.rates.is_empty());
    }

    #[test]
    fn pgp_stale_eviction() {
        let mut st = PgpRateState::default();
        let s1 = pgp_snapshot(
            100,
            vec![
                pgp_plan(1, 10, 100.0, 5, 100),
                pgp_plan(2, 10, 100.0, 5, 100),
            ],
        );
        update_pgp_rates(&mut st, &s1);

        let s2 = pgp_snapshot(400, vec![pgp_plan(1, 20, 200.0, 15, 400)]);
        update_pgp_rates(&mut st, &s2);
        // planid=2 still in prev_sample (age of entry: collected_at=100, now_ts=400 → 300s < MAX_PGP_STALE_SECS=900)
        assert!(st.prev_sample.contains_key(&2));

        // now_ts=1100 (from collected_at), prev_ts=400, dt=700 < MAX_PGP_RATE_DT_SECS=905
        // planid=2 entry has collected_at=100, age=1100-100=1000 > MAX_PGP_STALE_SECS=900 → evicted
        let s3 = pgp_snapshot(1200, vec![pgp_plan(1, 30, 300.0, 25, 1100)]);
        update_pgp_rates(&mut st, &s3);
        assert!(!st.prev_sample.contains_key(&2));
        assert!(st.prev_sample.contains_key(&1));
    }

    // ===== PGT tests =====

    #[test]
    fn pgt_first_sample_is_baseline() {
        let mut st = PgtRateState::default();
        let s = pgt_snapshot(100, vec![pgt_table(1, 10, 5, 100, 50, 10, 2, 100)]);
        update_pgt_rates(&mut st, &s);
        assert!(st.rates.is_empty());
        assert_eq!(st.prev_ts, Some(100));
    }

    #[test]
    fn pgt_rates_all_20_fields() {
        let mut st = PgtRateState::default();
        let t1 = PgStatUserTablesInfo {
            relid: 1,
            seq_scan: 10,
            seq_tup_read: 100,
            idx_scan: 20,
            idx_tup_fetch: 200,
            n_tup_ins: 50,
            n_tup_upd: 30,
            n_tup_del: 10,
            n_tup_hot_upd: 5,
            vacuum_count: 2,
            autovacuum_count: 1,
            heap_blks_read: 100,
            heap_blks_hit: 900,
            idx_blks_read: 50,
            idx_blks_hit: 450,
            toast_blks_read: 10,
            toast_blks_hit: 90,
            tidx_blks_read: 5,
            tidx_blks_hit: 45,
            analyze_count: 3,
            autoanalyze_count: 2,
            collected_at: 100,
            ..Default::default()
        };
        let s1 = pgt_snapshot(100, vec![t1.clone()]);
        update_pgt_rates(&mut st, &s1);

        let mut t2 = t1;
        t2.collected_at = 110;
        t2.seq_scan = 20;
        t2.seq_tup_read = 200;
        t2.idx_scan = 30;
        t2.idx_tup_fetch = 300;
        t2.n_tup_ins = 60;
        t2.n_tup_upd = 40;
        t2.n_tup_del = 20;
        t2.n_tup_hot_upd = 10;
        t2.vacuum_count = 3;
        t2.autovacuum_count = 2;
        t2.heap_blks_read = 110;
        t2.heap_blks_hit = 910;
        t2.idx_blks_read = 55;
        t2.idx_blks_hit = 455;
        t2.toast_blks_read = 12;
        t2.toast_blks_hit = 92;
        t2.tidx_blks_read = 7;
        t2.tidx_blks_hit = 47;
        t2.analyze_count = 4;
        t2.autoanalyze_count = 3;

        let s2 = pgt_snapshot(110, vec![t2]);
        update_pgt_rates(&mut st, &s2);

        let r = st.rates.get(&1).unwrap();
        assert!((r.dt_secs - 10.0).abs() < 1e-9);
        assert!((r.seq_scan_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.seq_tup_read_s.unwrap() - 10.0).abs() < 1e-9);
        assert!((r.idx_scan_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.idx_tup_fetch_s.unwrap() - 10.0).abs() < 1e-9);
        assert!((r.n_tup_ins_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.n_tup_upd_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.n_tup_del_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.n_tup_hot_upd_s.unwrap() - 0.5).abs() < 1e-9);
        assert!((r.vacuum_count_s.unwrap() - 0.1).abs() < 1e-9);
        assert!((r.autovacuum_count_s.unwrap() - 0.1).abs() < 1e-9);
        assert!((r.heap_blks_read_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.heap_blks_hit_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.idx_blks_read_s.unwrap() - 0.5).abs() < 1e-9);
        assert!((r.idx_blks_hit_s.unwrap() - 0.5).abs() < 1e-9);
        // The 6 fields that were MISSING from TUI:
        assert!((r.toast_blks_read_s.unwrap() - 0.2).abs() < 1e-9);
        assert!((r.toast_blks_hit_s.unwrap() - 0.2).abs() < 1e-9);
        assert!((r.tidx_blks_read_s.unwrap() - 0.2).abs() < 1e-9);
        assert!((r.tidx_blks_hit_s.unwrap() - 0.2).abs() < 1e-9);
        assert!((r.analyze_count_s.unwrap() - 0.1).abs() < 1e-9);
        assert!((r.autoanalyze_count_s.unwrap() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn pgt_full_replace_semantics() {
        let mut st = PgtRateState::default();
        let s1 = pgt_snapshot(
            100,
            vec![
                pgt_table(1, 10, 5, 100, 50, 10, 2, 100),
                pgt_table(2, 20, 10, 200, 100, 20, 4, 100),
            ],
        );
        update_pgt_rates(&mut st, &s1);

        // Only relid=1 in second snapshot
        let s2 = pgt_snapshot(110, vec![pgt_table(1, 20, 10, 200, 100, 20, 4, 110)]);
        update_pgt_rates(&mut st, &s2);
        // relid=2 should NOT be in prev_sample (full replace, not merge)
        assert!(!st.prev_sample.contains_key(&2));
    }

    #[test]
    fn pgt_same_collected_at_skips() {
        let mut st = PgtRateState::default();
        let s1 = pgt_snapshot(100, vec![pgt_table(1, 10, 5, 100, 50, 10, 2, 100)]);
        update_pgt_rates(&mut st, &s1);

        let s2 = pgt_snapshot(110, vec![pgt_table(1, 20, 10, 200, 100, 20, 4, 100)]); // same collected_at!
        update_pgt_rates(&mut st, &s2);
        assert!(st.rates.is_empty()); // no rates computed
    }

    // ===== PGI tests =====

    #[test]
    fn pgi_first_sample_is_baseline() {
        let mut st = PgiRateState::default();
        let s = pgi_snapshot(100, vec![pgi_index(1, 10, 100, 100)]);
        update_pgi_rates(&mut st, &s);
        assert!(st.rates.is_empty());
        assert_eq!(st.prev_ts, Some(100));
    }

    #[test]
    fn pgi_rates_computed() {
        let mut st = PgiRateState::default();
        let s1 = pgi_snapshot(100, vec![pgi_index(1, 10, 100, 100)]);
        update_pgi_rates(&mut st, &s1);

        let s2 = pgi_snapshot(110, vec![pgi_index(1, 20, 200, 110)]);
        update_pgi_rates(&mut st, &s2);

        let r = st.rates.get(&1).unwrap();
        assert!((r.dt_secs - 10.0).abs() < 1e-9);
        assert!((r.idx_scan_s.unwrap() - 1.0).abs() < 1e-9);
        assert!((r.idx_tup_read_s.unwrap() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn pgi_full_replace_semantics() {
        let mut st = PgiRateState::default();
        let s1 = pgi_snapshot(
            100,
            vec![pgi_index(1, 10, 100, 100), pgi_index(2, 20, 200, 100)],
        );
        update_pgi_rates(&mut st, &s1);

        let s2 = pgi_snapshot(110, vec![pgi_index(1, 20, 200, 110)]);
        update_pgi_rates(&mut st, &s2);
        assert!(!st.prev_sample.contains_key(&2));
    }
}
