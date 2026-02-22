//! Background processing: tick loop, history refresh/navigation, rate seeding, snapshot conversion.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use rpglot_core::api::convert::{ConvertContext, convert, resolve};
use rpglot_core::api::snapshot::{ApiSnapshot, PgStatementsRow, PgStorePlansRow};
use rpglot_core::provider::HistoryProvider;
use rpglot_core::rates;
use rpglot_core::storage::StringInterner;
use rpglot_core::storage::model::{DataBlock, PgStatStatementsInfo, PgStorePlansInfo, Snapshot};

use crate::state::{
    LAST_CLIENT_ACTIVITY, Mode, SharedState, WebAppInner, now_epoch, release_memory_to_os,
};

// ============================================================
// Tick loop (live mode)
// ============================================================

pub(crate) async fn tick_loop(
    state: SharedState,
    tx: broadcast::Sender<Arc<ApiSnapshot>>,
    interval: Duration,
) {
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut snapshot_count: u64 = 0;

    loop {
        tick.tick().await;

        // Run blocking provider.advance() off the async runtime
        let state_clone = state.clone();
        let t0 = Instant::now();
        let result = tokio::task::spawn_blocking(move || {
            let mut inner = state_clone.lock().unwrap();
            advance_and_convert(&mut inner);
            inner.current_snapshot.clone()
        })
        .await;

        let elapsed = t0.elapsed();

        let snapshot = match result {
            Ok(snap) => snap,
            Err(e) => {
                error!(error = %e, "tick panicked in spawn_blocking");
                continue;
            }
        };

        if let Some(ref snap) = snapshot {
            snapshot_count += 1;
            if snapshot_count == 1 {
                info!(
                    duration_ms = elapsed.as_millis() as u64,
                    timestamp = snap.timestamp,
                    "first snapshot collected"
                );
            } else {
                debug!(
                    duration_ms = elapsed.as_millis() as u64,
                    timestamp = snap.timestamp,
                    snapshot_count,
                    "tick completed"
                );
            }
        } else {
            warn!(
                duration_ms = elapsed.as_millis() as u64,
                "tick produced no snapshot"
            );
        }

        if elapsed > interval / 2 {
            warn!(
                duration_ms = elapsed.as_millis() as u64,
                interval_ms = interval.as_millis() as u64,
                "tick exceeded 50% of interval"
            );
        }

        if let Some(snap) = snapshot {
            let _ = tx.send(snap);
        }
    }
}

// ============================================================
// History refresh loop
// ============================================================

/// Background loop: idle eviction + refresh history snapshots from disk.
///
/// Does NO work until a client has connected at least once. After client leaves,
/// evicts all data after IDLE_EVICT_SECS. Only refreshes during active use.
pub(crate) async fn history_refresh_loop(state: SharedState, path: PathBuf) {
    const IDLE_EVICT_SECS: i64 = 60;

    let mut tick = tokio::time::interval(Duration::from_secs(30));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tick.tick().await;

        // Quick check outside lock: no client ever connected → skip entirely
        let last_activity = LAST_CLIENT_ACTIVITY.load(Ordering::Relaxed);
        if last_activity == 0 {
            continue;
        }

        let now = now_epoch();
        let is_idle = (now - last_activity) > IDLE_EVICT_SECS;

        let state_clone = state.clone();
        let path_clone = path.clone();
        let t0 = Instant::now();
        let result = tokio::task::spawn_blocking(move || {
            let mut inner = state_clone.lock().unwrap();

            // Idle eviction: free ALL cached data (including chunk index)
            if is_idle {
                let has_data = inner.current_snapshot.is_some();
                let hp_initialized = inner
                    .provider
                    .as_any()
                    .and_then(|a| a.downcast_ref::<HistoryProvider>())
                    .is_some_and(|hp| hp.is_initialized());
                if has_data || hp_initialized {
                    evict_caches(&mut inner);
                    release_memory_to_os();
                    info!("idle eviction: all caches cleared");
                }
                return Ok::<(usize, usize), rpglot_core::provider::ProviderError>((0, 0));
            }

            // Active client — refresh from disk (only if initialized)
            let hp = inner
                .provider
                .as_any_mut()
                .and_then(|a| a.downcast_mut::<HistoryProvider>());
            let Some(hp) = hp else {
                return Ok((0usize, 0usize));
            };
            if !hp.is_initialized() {
                return Ok((0, 0));
            }
            let added = hp.refresh(&path_clone)?;
            let total = hp.len();
            if added > 0 {
                let (start, end) = hp.timestamp_range();
                inner.total_snapshots = Some(total);
                inner.history_start = Some(start);
                inner.history_end = Some(end);
                // Invalidate today's heatmap cache (new data may have been added)
                let today_days = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    / 86400;
                let d = chrono_free_date(today_days);
                let today_key = format!("{:04}-{:02}-{:02}", d.0, d.1, d.2);
                inner.heatmap_cache.remove(&today_key);
            }
            release_memory_to_os(); // free any chunk decompression buffers
            Ok::<(usize, usize), rpglot_core::provider::ProviderError>((added, total))
        })
        .await;

        let elapsed = t0.elapsed();
        match result {
            Ok(Ok((added, total))) if added > 0 => {
                info!(
                    added,
                    total,
                    duration_ms = elapsed.as_millis() as u64,
                    "history refreshed"
                );
            }
            Ok(Err(e)) => {
                warn!(
                    error = %e,
                    duration_ms = elapsed.as_millis() as u64,
                    "history refresh failed"
                );
            }
            _ => {}
        }
    }
}

/// Evict all cached data from WebAppInner to free memory during idle periods.
/// Keeps only lightweight metadata (total_snapshots, history_start/end, instance_info).
fn evict_caches(inner: &mut WebAppInner) {
    inner.current_snapshot = None;
    inner.raw_snapshot = None;
    inner.prev_snapshot = None;
    inner.pgs_rate.reset();
    inner.pgs_rate.shrink_to_fit();
    inner.pgp_rate.reset();
    inner.pgp_rate.shrink_to_fit();
    inner.pgt_rate.reset();
    inner.pgt_rate.shrink_to_fit();
    inner.pgi_rate.reset();
    inner.pgi_rate.shrink_to_fit();
    inner.heatmap_cache.clear();
    inner.heatmap_cache.shrink_to_fit();
    // Full eviction: drop chunk index, timestamps, WAL — back to uninitialized
    if let Some(hp) = inner
        .provider
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<HistoryProvider>())
    {
        hp.evict_all();
    }
}

/// Ensure HistoryProvider is initialized (build chunk index from disk if needed).
/// Updates cached metadata in WebAppInner on first init.
/// Returns false on error (provider could not be initialized).
pub(crate) fn ensure_history_ready(inner: &mut WebAppInner) -> bool {
    let hp = inner
        .provider
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<HistoryProvider>());
    let Some(hp) = hp else { return false };

    if hp.is_initialized() {
        return true;
    }

    if let Err(e) = hp.ensure_initialized() {
        warn!(error = %e, "failed to initialize history provider");
        return false;
    }

    let total = hp.len();
    let (start, end) = hp.timestamp_range();
    inner.total_snapshots = Some(total);
    inner.history_start = Some(start);
    inner.history_end = Some(end);

    info!(
        snapshots = total,
        "history provider initialized on first request"
    );
    true
}

// ============================================================
// Stale row helpers
// ============================================================

/// Create a stale `PgStatementsRow` from a `PgStatStatementsInfo` (all rates = None).
fn pgs_info_to_stale_row(s: &PgStatStatementsInfo, interner: &StringInterner) -> PgStatementsRow {
    let int = Some(interner);
    let total_blks = s.shared_blks_hit + s.shared_blks_read;
    let hit_pct = if total_blks > 0 {
        Some(s.shared_blks_hit as f64 * 100.0 / total_blks as f64)
    } else {
        None
    };
    let rows_per_call = if s.calls > 0 {
        Some(s.rows as f64 / s.calls as f64)
    } else {
        None
    };
    PgStatementsRow {
        queryid: s.queryid,
        database: resolve(int, s.datname_hash),
        user: resolve(int, s.usename_hash),
        query: resolve(int, s.query_hash),
        calls: s.calls,
        rows: s.rows,
        mean_exec_time_ms: s.mean_exec_time,
        min_exec_time_ms: s.min_exec_time,
        max_exec_time_ms: s.max_exec_time,
        stddev_exec_time_ms: s.stddev_exec_time,
        calls_s: None,
        rows_s: None,
        exec_time_ms_s: None,
        shared_blks_read_s: None,
        shared_blks_hit_s: None,
        shared_blks_dirtied_s: None,
        shared_blks_written_s: None,
        local_blks_read_s: None,
        local_blks_written_s: None,
        temp_blks_read_s: None,
        temp_blks_written_s: None,
        temp_mb_s: None,
        rows_per_call,
        hit_pct,
        total_plan_time: s.total_plan_time,
        wal_records: s.wal_records,
        wal_bytes: s.wal_bytes,
        total_exec_time: s.total_exec_time,
        stale: true,
    }
}

/// Create a stale `PgStorePlansRow` from a `PgStorePlansInfo` (all rates = None).
fn pgp_info_to_stale_row(
    p: &PgStorePlansInfo,
    interner: &StringInterner,
    query: &str,
) -> PgStorePlansRow {
    let int = Some(interner);
    let total_blks = p.shared_blks_hit + p.shared_blks_read;
    let hit_pct = if total_blks > 0 {
        Some(p.shared_blks_hit as f64 * 100.0 / total_blks as f64)
    } else {
        None
    };
    let rows_per_call = if p.calls > 0 {
        Some(p.rows as f64 / p.calls as f64)
    } else {
        None
    };
    PgStorePlansRow {
        planid: p.planid,
        stmt_queryid: p.stmt_queryid,
        database: resolve(int, p.datname_hash),
        user: resolve(int, p.usename_hash),
        query: query.to_string(),
        plan: resolve(int, p.plan_hash),
        calls: p.calls,
        rows: p.rows,
        mean_time_ms: p.mean_time,
        min_time_ms: p.min_time,
        max_time_ms: p.max_time,
        total_time_ms: p.total_time,
        first_call: p.first_call,
        last_call: p.last_call,
        calls_s: None,
        rows_s: None,
        exec_time_ms_s: None,
        shared_blks_read_s: None,
        shared_blks_hit_s: None,
        shared_blks_dirtied_s: None,
        shared_blks_written_s: None,
        temp_blks_read_s: None,
        temp_blks_written_s: None,
        rows_per_call,
        hit_pct,
        stale: true,
    }
}

// ============================================================
// Advance & convert (live mode)
// ============================================================

/// Advance provider, compute rates, convert to ApiSnapshot.
fn advance_and_convert(inner: &mut WebAppInner) {
    // Advance provider to get next snapshot
    let snapshot = match inner.provider.advance() {
        Some(s) => s.clone(),
        None => {
            if let Some(e) = inner.provider.last_error() {
                warn!(error = %e, "failed to collect snapshot");
            } else {
                warn!("advance() returned None with no error (snapshot buffer empty?)");
            }
            return;
        }
    };

    // Cache instance metadata; update is_in_recovery every tick (may change on failover)
    let is_in_recovery = inner.provider.is_in_recovery();
    match &mut inner.instance_info {
        Some(info) => {
            info.2 = is_in_recovery;
        }
        None => {
            if let Some((db, ver)) = inner.provider.instance_info() {
                inner.instance_info = Some((db, ver, is_in_recovery));
            }
        }
    }

    // Update rates (must happen before borrowing interner)
    rates::update_pgs_rates(&mut inner.pgs_rate, &snapshot);
    rates::update_pgp_rates(&mut inner.pgp_rate, &snapshot);
    rates::update_pgt_rates(&mut inner.pgt_rate, &snapshot);
    rates::update_pgi_rates(&mut inner.pgi_rate, &snapshot);

    // Convert to API snapshot (interner borrowed here, after rates are done)
    // For history mode, extract prev/next timestamps for navigation
    let (prev_ts, next_ts) = if inner.mode == Mode::History {
        let hp = inner
            .provider
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<HistoryProvider>());
        hp.map(|hp| (hp.prev_timestamp(), hp.next_timestamp()))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    let ctx = ConvertContext {
        snapshot: &snapshot,
        prev_snapshot: inner.prev_snapshot.as_ref(),
        interner: inner.provider.interner(),
        pgs_rates: &inner.pgs_rate.rates,
        pgp_rates: &inner.pgp_rate.rates,
        pgt_rates: &inner.pgt_rate.rates,
        pgi_rates: &inner.pgi_rate.rates,
    };
    let mut api_snapshot = convert(&ctx);
    api_snapshot.prev_timestamp = prev_ts;
    api_snapshot.next_timestamp = next_ts;

    // Merge stale PGS entries from prev_sample
    if let Some(interner) = inner.provider.interner() {
        let pgs_ids: HashSet<i64> = api_snapshot.pgs.iter().map(|r| r.queryid).collect();
        for info in inner.pgs_rate.prev_sample.values() {
            if !pgs_ids.contains(&info.queryid) {
                api_snapshot.pgs.push(pgs_info_to_stale_row(info, interner));
            }
        }

        // Merge stale PGP entries from prev_sample
        let pgp_ids: HashSet<i64> = api_snapshot.pgp.iter().map(|r| r.planid).collect();
        for info in inner.pgp_rate.prev_sample.values() {
            if !pgp_ids.contains(&info.planid) {
                let query = inner
                    .pgs_rate
                    .prev_sample
                    .get(&info.stmt_queryid)
                    .map(|s| resolve(Some(interner), s.query_hash))
                    .unwrap_or_default();
                api_snapshot
                    .pgp
                    .push(pgp_info_to_stale_row(info, interner, &query));
            }
        }
    }

    // Rotate snapshots
    inner.prev_snapshot = inner.raw_snapshot.take();
    inner.raw_snapshot = Some(snapshot);
    inner.current_snapshot = Some(Arc::new(api_snapshot));
}

// ============================================================
// History navigation
// ============================================================

/// Navigate history provider to timestamp and reconvert.
pub(crate) fn history_jump_to_timestamp(
    inner: &mut WebAppInner,
    timestamp: i64,
    ceil: bool,
) -> bool {
    let provider = inner
        .provider
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<HistoryProvider>());
    let found = if let Some(hp) = provider {
        if ceil {
            hp.jump_to_timestamp_ceil(timestamp).is_some()
        } else {
            hp.jump_to_timestamp_floor(timestamp).is_some()
        }
    } else {
        false
    };
    if found {
        reconvert_current(inner);
        info!(timestamp, ceil, "history: jumped to timestamp");
        true
    } else {
        warn!(timestamp, "history: invalid timestamp");
        false
    }
}

/// Extract collected_at timestamp from PgStatStatements block.
fn extract_pgs_collected_at(snapshot: &Snapshot) -> Option<i64> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatStatements(stmts) = b {
            stmts.first().map(|s| s.collected_at).filter(|&t| t > 0)
        } else {
            None
        }
    })
}

fn extract_pgp_collected_at(snapshot: &Snapshot) -> Option<i64> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStorePlans(plans) = b {
            plans.first().map(|p| p.collected_at).filter(|&t| t > 0)
        } else {
            None
        }
    })
}

fn extract_pgt_collected_at(snapshot: &Snapshot) -> Option<i64> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserTables(tables) = b {
            tables.first().map(|t| t.collected_at).filter(|&t| t > 0)
        } else {
            None
        }
    })
}

fn extract_pgi_collected_at(snapshot: &Snapshot) -> Option<i64> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserIndexes(indexes) = b {
            indexes.first().map(|i| i.collected_at).filter(|&t| t > 0)
        } else {
            None
        }
    })
}

/// Find the nearest previous snapshot with a DIFFERENT PGS collected_at.
/// Daemon caches pg_stat_statements for ~30s, so adjacent snapshots often
/// have the same collected_at. We look further back to find a snapshot
/// with different data for accurate rate computation.
fn find_pgs_prev_snapshot(
    hp: &mut HistoryProvider,
    pos: usize,
    current_collected_at: i64,
) -> Option<Snapshot> {
    let max_lookback = 30; // PGS cached ~30s, need ~300s / 5min lookback
    let start = pos.saturating_sub(max_lookback);
    for p in (start..pos).rev() {
        if let Some(snap) = hp.snapshot_at(p)
            && let Some(ts) = extract_pgs_collected_at(&snap)
            && ts != current_collected_at
        {
            return Some(snap);
        }
    }
    None
}

fn find_pgp_prev_snapshot(
    hp: &mut HistoryProvider,
    pos: usize,
    current_collected_at: i64,
) -> Option<Snapshot> {
    // pg_store_plans is collected every 5 min; with 10s snapshots that's ~30
    // snapshots with the same collected_at.  Need to look back far enough.
    let max_lookback = 40;
    let start = pos.saturating_sub(max_lookback);
    for p in (start..pos).rev() {
        if let Some(snap) = hp.snapshot_at(p)
            && let Some(ts) = extract_pgp_collected_at(&snap)
            && ts != current_collected_at
        {
            return Some(snap);
        }
    }
    None
}

fn find_pgt_prev_snapshot(
    hp: &mut HistoryProvider,
    pos: usize,
    current_collected_at: i64,
) -> Option<Snapshot> {
    let max_lookback = 30;
    let start = pos.saturating_sub(max_lookback);
    for p in (start..pos).rev() {
        if let Some(snap) = hp.snapshot_at(p)
            && let Some(ts) = extract_pgt_collected_at(&snap)
            && ts != current_collected_at
        {
            return Some(snap);
        }
    }
    None
}

fn find_pgi_prev_snapshot(
    hp: &mut HistoryProvider,
    pos: usize,
    current_collected_at: i64,
) -> Option<Snapshot> {
    let max_lookback = 30;
    let start = pos.saturating_sub(max_lookback);
    for p in (start..pos).rev() {
        if let Some(snap) = hp.snapshot_at(p)
            && let Some(ts) = extract_pgi_collected_at(&snap)
            && ts != current_collected_at
        {
            return Some(snap);
        }
    }
    None
}

// ============================================================
// Reconvert current snapshot
// ============================================================

/// Reconvert current provider snapshot to ApiSnapshot (after history jump).
/// Uses the adjacent previous snapshot (position-1) to compute rates and system deltas.
pub(crate) fn reconvert_current(inner: &mut WebAppInner) {
    // Extract snapshots from provider (mutable borrow for lazy loading)
    let (snapshot, prev_adjacent, position, prev_ts, next_ts) = {
        let provider = inner
            .provider
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<HistoryProvider>());
        let Some(hp) = provider else { return };
        let pos = hp.position();
        let snap = hp.snapshot_at(pos);
        let prev = if pos > 0 {
            hp.snapshot_at(pos - 1)
        } else {
            None
        };
        let prev_ts = hp.prev_timestamp();
        let next_ts = hp.next_timestamp();
        (snap, prev, pos, prev_ts, next_ts)
    };

    let Some(snapshot) = snapshot else {
        warn!(
            position,
            "reconvert_current: failed to load snapshot at position"
        );
        return;
    };

    // Reset rate tracking state
    inner.pgs_rate.reset();
    inner.pgp_rate.reset();
    inner.pgt_rate.reset();
    inner.pgi_rate.reset();

    // Seed prev_samples and compute rates.
    // All pg_stat_* data is cached ~30s by the collector while snapshots are
    // written every ~10s.  Adjacent snapshot (pos-1) may have the same
    // collected_at → rates would be empty.  Look back further to find a
    // snapshot with different collected_at for each data source.
    if let Some(ref prev) = prev_adjacent {
        // Helper: get mutable HistoryProvider ref (borrows inner.provider)
        macro_rules! hp_mut {
            ($inner:expr) => {
                $inner
                    .provider
                    .as_any_mut()
                    .and_then(|a| a.downcast_mut::<HistoryProvider>())
            };
        }

        // PGT
        let pgt_seed = extract_pgt_collected_at(&snapshot).and_then(|curr_ts| {
            hp_mut!(inner).and_then(|hp| find_pgt_prev_snapshot(hp, position, curr_ts))
        });
        let pgt_prev = pgt_seed.as_ref().unwrap_or(prev);
        seed_pgt_prev(inner, pgt_prev);
        rates::update_pgt_rates(&mut inner.pgt_rate, &snapshot);

        // PGI
        let pgi_seed = extract_pgi_collected_at(&snapshot).and_then(|curr_ts| {
            hp_mut!(inner).and_then(|hp| find_pgi_prev_snapshot(hp, position, curr_ts))
        });
        let pgi_prev = pgi_seed.as_ref().unwrap_or(prev);
        seed_pgi_prev(inner, pgi_prev);
        rates::update_pgi_rates(&mut inner.pgi_rate, &snapshot);

        // PGS
        let pgs_seed = extract_pgs_collected_at(&snapshot).and_then(|curr_ts| {
            hp_mut!(inner).and_then(|hp| find_pgs_prev_snapshot(hp, position, curr_ts))
        });
        let pgs_prev = pgs_seed.as_ref().unwrap_or(prev);
        seed_pgs_prev(inner, pgs_prev);
        rates::update_pgs_rates(&mut inner.pgs_rate, &snapshot);

        // PGP
        let pgp_seed = extract_pgp_collected_at(&snapshot).and_then(|curr_ts| {
            hp_mut!(inner).and_then(|hp| find_pgp_prev_snapshot(hp, position, curr_ts))
        });
        let pgp_prev = pgp_seed.as_ref().unwrap_or(prev);
        seed_pgp_prev(inner, pgp_prev);
        rates::update_pgp_rates(&mut inner.pgp_rate, &snapshot);
    }

    let interner = inner.provider.interner();
    let ctx = ConvertContext {
        snapshot: &snapshot,
        prev_snapshot: prev_adjacent.as_ref(),
        interner,
        pgs_rates: &inner.pgs_rate.rates,
        pgp_rates: &inner.pgp_rate.rates,
        pgt_rates: &inner.pgt_rate.rates,
        pgi_rates: &inner.pgi_rate.rates,
    };
    let mut api_snapshot = convert(&ctx);
    api_snapshot.prev_timestamp = prev_ts;
    api_snapshot.next_timestamp = next_ts;

    inner.prev_snapshot = prev_adjacent;
    inner.raw_snapshot = Some(snapshot);
    inner.current_snapshot = Some(Arc::new(api_snapshot));
}

// ============================================================
// Rate seeding helpers
// ============================================================

/// Seed PGS prev_sample state from a snapshot (for rate computation after jump).
fn seed_pgs_prev(inner: &mut WebAppInner, prev: &Snapshot) {
    if let Some(stmts) = prev.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatStatements(v) = b {
            Some(v)
        } else {
            None
        }
    }) {
        let ts = stmts
            .first()
            .map(|s| s.collected_at)
            .filter(|&t| t > 0)
            .unwrap_or(prev.timestamp);
        inner.pgs_rate.prev_ts = Some(ts);
        inner.pgs_rate.prev_sample = stmts.iter().map(|s| (s.queryid, s.clone())).collect();
    }
}

/// Seed PGT prev_sample state from a snapshot (for rate computation after jump).
fn seed_pgt_prev(inner: &mut WebAppInner, prev: &Snapshot) {
    if let Some(tables) = prev.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserTables(v) = b {
            Some(v)
        } else {
            None
        }
    }) {
        let ts = tables
            .first()
            .map(|t| t.collected_at)
            .filter(|&t| t > 0)
            .unwrap_or(prev.timestamp);
        inner.pgt_rate.prev_ts = Some(ts);
        inner.pgt_rate.prev_sample = tables.iter().map(|t| (t.relid, t.clone())).collect();
    }
}

/// Seed PGI prev_sample state from a snapshot (for rate computation after jump).
fn seed_pgi_prev(inner: &mut WebAppInner, prev: &Snapshot) {
    if let Some(indexes) = prev.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserIndexes(v) = b {
            Some(v)
        } else {
            None
        }
    }) {
        let ts = indexes
            .first()
            .map(|i| i.collected_at)
            .filter(|&t| t > 0)
            .unwrap_or(prev.timestamp);
        inner.pgi_rate.prev_ts = Some(ts);
        inner.pgi_rate.prev_sample = indexes.iter().map(|i| (i.indexrelid, i.clone())).collect();
    }
}

/// Seed PGP prev_sample state from a snapshot (for rate computation after jump).
fn seed_pgp_prev(inner: &mut WebAppInner, prev: &Snapshot) {
    if let Some(plans) = prev.blocks.iter().find_map(|b| {
        if let DataBlock::PgStorePlans(v) = b {
            Some(v)
        } else {
            None
        }
    }) {
        let ts = plans
            .first()
            .map(|p| p.collected_at)
            .filter(|&t| t > 0)
            .unwrap_or(prev.timestamp);
        inner.pgp_rate.prev_ts = Some(ts);
        inner.pgp_rate.prev_sample = plans.iter().map(|p| (p.planid, p.clone())).collect();
    }
}

// ============================================================
// Date utility
// ============================================================

/// Convert days-since-epoch to (year, month, day) without chrono crate.
pub(crate) fn chrono_free_date(days_since_epoch: i64) -> (i32, u32, u32) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days_since_epoch + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}
