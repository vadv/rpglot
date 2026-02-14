use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use clap::Parser;
use serde::Deserialize;
use std::sync::Mutex;
use tokio::sync::broadcast;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

use axum::body::Body;
use axum::extract::Request;
use axum::http::{Uri, header};
use axum::middleware::Next;
use rpglot_core::api::convert::{ConvertContext, convert};
use rpglot_core::api::schema::{ApiMode, ApiSchema, TimelineInfo};
use rpglot_core::api::snapshot::ApiSnapshot;
#[cfg(target_os = "linux")]
use rpglot_core::collector::RealFs;
#[cfg(not(target_os = "linux"))]
use rpglot_core::collector::mock::MockFs;
use rpglot_core::collector::{Collector, PostgresCollector};
use rpglot_core::models::{PgIndexesRates, PgStatementsRates, PgTablesRates};
use rpglot_core::provider::{HistoryProvider, LiveProvider, SnapshotProvider};
use rpglot_core::storage::model::{DataBlock, Snapshot};
use rust_embed::Embed;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

// ============================================================
// Embedded frontend assets
// ============================================================

#[derive(Embed)]
#[folder = "frontend/dist"]
struct FrontendAssets;

// ============================================================
// CLI
// ============================================================

#[derive(Parser)]
#[command(name = "rpglot-web", about = "rpglot web API server")]
struct Args {
    /// Listen address.
    #[arg(long, default_value = "0.0.0.0:8080", env = "RPGLOT_LISTEN")]
    listen: String,

    /// Path to history data directory (history mode).
    /// If not specified, runs in live mode collecting from local system + PostgreSQL.
    #[arg(long, env = "RPGLOT_HISTORY")]
    history: Option<PathBuf>,

    /// Snapshot interval in seconds (live mode).
    #[arg(long, default_value = "1", env = "RPGLOT_INTERVAL")]
    interval: u64,

    /// Path to /proc filesystem (live mode).
    #[arg(long, default_value = "/proc")]
    proc_path: String,

    /// Path to cgroup filesystem (live mode, container).
    #[arg(long, value_name = "PATH")]
    cgroup_path: Option<String>,

    /// Force cgroup collection (live mode).
    #[arg(long)]
    force_cgroup: bool,

    /// Basic Auth username. If set, --auth-password is also required.
    #[arg(long, env = "RPGLOT_AUTH_USER")]
    auth_user: Option<String>,

    /// Basic Auth password.
    #[arg(long, env = "RPGLOT_AUTH_PASSWORD")]
    auth_password: Option<String>,
}

// ============================================================
// Shared application state
// ============================================================

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Live,
    History,
}

struct WebAppInner {
    provider: Box<dyn SnapshotProvider + Send>,
    mode: Mode,
    // Current API snapshot
    current_snapshot: Option<Arc<ApiSnapshot>>,
    // Raw snapshots for delta/rates computation
    raw_snapshot: Option<Snapshot>,
    prev_snapshot: Option<Snapshot>,
    // Rates
    pgs_rates: HashMap<i64, PgStatementsRates>,
    pgt_rates: HashMap<u32, PgTablesRates>,
    pgi_rates: HashMap<u32, PgIndexesRates>,
    // PGS rate tracking
    pgs_prev_sample: HashMap<i64, rpglot_core::storage::model::PgStatStatementsInfo>,
    pgs_prev_ts: Option<i64>,
    // PGT rate tracking
    pgt_prev_sample: HashMap<u32, rpglot_core::storage::model::PgStatUserTablesInfo>,
    pgt_prev_ts: Option<i64>,
    // PGI rate tracking
    pgi_prev_sample: HashMap<u32, rpglot_core::storage::model::PgStatUserIndexesInfo>,
    pgi_prev_ts: Option<i64>,
    // History metadata (cached at startup)
    total_snapshots: Option<usize>,
    history_start: Option<i64>,
    history_end: Option<i64>,
}

type SharedState = Arc<Mutex<WebAppInner>>;

// ============================================================
// Main
// ============================================================

fn main() {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rpglot_web=info,tower_http=info".parse().unwrap()),
        )
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async_main(args));
}

async fn async_main(args: Args) {
    let (provider, mode, total_snapshots, history_start, history_end): (
        Box<dyn SnapshotProvider + Send>,
        Mode,
        Option<usize>,
        Option<i64>,
        Option<i64>,
    ) = if let Some(ref history_path) = args.history {
        info!(path = %history_path.display(), "starting in history mode");
        let hp = HistoryProvider::from_path(history_path).expect("failed to open history data");
        let total = hp.len();
        let (start, end) = if total > 0 {
            let first_ts = hp.snapshot_at(0).map(|s| s.timestamp).unwrap_or(0);
            let last_ts = hp.snapshot_at(total - 1).map(|s| s.timestamp).unwrap_or(0);
            (first_ts, last_ts)
        } else {
            (0, 0)
        };
        info!(snapshots = total, "loaded history data");
        (
            Box::new(hp),
            Mode::History,
            Some(total),
            Some(start),
            Some(end),
        )
    } else {
        info!("starting in live mode");
        let provider = create_live_provider(&args);
        (provider, Mode::Live, None, None, None)
    };

    let (tx, _rx) = broadcast::channel::<Arc<ApiSnapshot>>(16);

    let inner = WebAppInner {
        provider,
        mode,
        current_snapshot: None,
        raw_snapshot: None,
        prev_snapshot: None,
        pgs_rates: HashMap::new(),
        pgt_rates: HashMap::new(),
        pgi_rates: HashMap::new(),
        pgs_prev_sample: HashMap::new(),
        pgs_prev_ts: None,
        pgt_prev_sample: HashMap::new(),
        pgt_prev_ts: None,
        pgi_prev_sample: HashMap::new(),
        pgi_prev_ts: None,
        total_snapshots,
        history_start,
        history_end,
    };

    let state: SharedState = Arc::new(Mutex::new(inner));

    // Start background tick loop for live mode
    if mode == Mode::Live {
        let state_clone = state.clone();
        let tx_clone = tx.clone();
        let interval = Duration::from_secs(args.interval);
        tokio::spawn(async move {
            tick_loop(state_clone, tx_clone, interval).await;
        });
    } else {
        // History: load first snapshot
        let mut inner = state.lock().unwrap();
        advance_and_convert(&mut inner);
    }

    // Basic Auth
    let auth_creds: Option<Arc<(String, String)>> = match (&args.auth_user, &args.auth_password) {
        (Some(user), Some(pass)) => {
            info!("basic auth enabled");
            Some(Arc::new((user.clone(), pass.clone())))
        }
        (Some(_), None) | (None, Some(_)) => {
            panic!("--auth-user and --auth-password must both be set");
        }
        _ => None,
    };

    // Router
    let mut app = Router::new()
        .route("/api/v1/health", get(handle_health))
        .route("/api/v1/schema", get(handle_schema))
        .route("/api/v1/snapshot", get(handle_snapshot))
        .route("/api/v1/stream", get(handle_stream))
        .route("/api/v1/timeline", get(handle_timeline))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .fallback(get(serve_frontend))
        .with_state((state, tx));

    if let Some(creds) = auth_creds {
        app = app.layer(axum::middleware::from_fn_with_state(
            creds,
            basic_auth_middleware,
        ));
    }

    let app = app
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new());

    let addr: SocketAddr = args.listen.parse().expect("invalid listen address");
    info!(%addr, "listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");

    axum::serve(listener, app).await.expect("server error");
}

fn create_live_provider(args: &Args) -> Box<dyn SnapshotProvider + Send> {
    #[cfg(target_os = "linux")]
    {
        let fs = RealFs::new();
        let mut collector = Collector::new(fs, &args.proc_path);
        if let Ok(pg) = PostgresCollector::from_env() {
            collector = collector.with_postgres(pg.with_statements_interval(Duration::ZERO));
        }
        if let Some(ref cgroup_path) = args.cgroup_path {
            collector = collector.with_cgroup(cgroup_path);
        } else if args.force_cgroup {
            collector = collector.force_cgroup(None);
        }
        Box::new(LiveProvider::new(collector, None))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let fs = MockFs::typical_system();
        let mut collector = Collector::new(fs, &args.proc_path);
        if let Ok(pg) = PostgresCollector::from_env() {
            collector = collector.with_postgres(pg.with_statements_interval(Duration::ZERO));
        }
        if let Some(ref cgroup_path) = args.cgroup_path {
            collector = collector.with_cgroup(cgroup_path);
        } else if args.force_cgroup {
            collector = collector.force_cgroup(None);
        }
        Box::new(LiveProvider::new(collector, None))
    }
}

// ============================================================
// Tick loop (live mode)
// ============================================================

async fn tick_loop(
    state: SharedState,
    tx: broadcast::Sender<Arc<ApiSnapshot>>,
    interval: Duration,
) {
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tick.tick().await;

        // Run blocking provider.advance() off the async runtime
        let state_clone = state.clone();
        let snapshot = tokio::task::spawn_blocking(move || {
            let mut inner = state_clone.lock().unwrap();
            advance_and_convert(&mut inner);
            inner.current_snapshot.clone()
        })
        .await
        .ok()
        .flatten();

        if let Some(snap) = snapshot {
            let _ = tx.send(snap);
        }
    }
}

/// Advance provider, compute rates, convert to ApiSnapshot.
fn advance_and_convert(inner: &mut WebAppInner) {
    // Advance provider to get next snapshot
    let snapshot = match inner.provider.advance() {
        Some(s) => s.clone(),
        None => {
            if let Some(e) = inner.provider.last_error() {
                warn!(error = %e, "failed to collect snapshot");
            }
            return;
        }
    };

    // Update rates (must happen before borrowing interner)
    update_pgs_rates(inner, &snapshot);
    update_pgt_rates(inner, &snapshot);
    update_pgi_rates(inner, &snapshot);

    // Convert to API snapshot (interner borrowed here, after rates are done)
    let ctx = ConvertContext {
        snapshot: &snapshot,
        prev_snapshot: inner.prev_snapshot.as_ref(),
        interner: inner.provider.interner(),
        pgs_rates: &inner.pgs_rates,
        pgt_rates: &inner.pgt_rates,
        pgi_rates: &inner.pgi_rates,
    };
    let api_snapshot = convert(&ctx);

    // Rotate snapshots
    inner.prev_snapshot = inner.raw_snapshot.take();
    inner.raw_snapshot = Some(snapshot);
    inner.current_snapshot = Some(Arc::new(api_snapshot));
}

/// Navigate history provider to position and reconvert.
fn history_jump_to(inner: &mut WebAppInner, position: usize) -> bool {
    let provider = inner
        .provider
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<HistoryProvider>());
    if let Some(hp) = provider {
        if hp.jump_to(position).is_some() {
            reconvert_current(inner);
            return true;
        }
    }
    false
}

/// Navigate history provider to timestamp and reconvert.
fn history_jump_to_timestamp(inner: &mut WebAppInner, timestamp: i64) -> bool {
    let provider = inner
        .provider
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<HistoryProvider>());
    if let Some(hp) = provider {
        if hp.jump_to_timestamp_floor(timestamp).is_some() {
            reconvert_current(inner);
            return true;
        }
    }
    false
}

/// Reconvert current provider snapshot to ApiSnapshot (after history jump).
fn reconvert_current(inner: &mut WebAppInner) {
    let Some(snapshot) = inner.provider.current().cloned() else {
        return;
    };
    let interner = inner.provider.interner();

    // Reset rates after jump (previous samples no longer adjacent)
    inner.pgs_rates.clear();
    inner.pgt_rates.clear();
    inner.pgi_rates.clear();
    inner.pgs_prev_sample.clear();
    inner.pgs_prev_ts = None;
    inner.pgt_prev_sample.clear();
    inner.pgt_prev_ts = None;
    inner.pgi_prev_sample.clear();
    inner.pgi_prev_ts = None;

    let ctx = ConvertContext {
        snapshot: &snapshot,
        prev_snapshot: inner.prev_snapshot.as_ref(),
        interner,
        pgs_rates: &inner.pgs_rates,
        pgt_rates: &inner.pgt_rates,
        pgi_rates: &inner.pgi_rates,
    };
    let api_snapshot = convert(&ctx);

    inner.prev_snapshot = inner.raw_snapshot.take();
    inner.raw_snapshot = Some(snapshot);
    inner.current_snapshot = Some(Arc::new(api_snapshot));
}

// ============================================================
// Rate computation (mirrors TUI tab_states logic)
// ============================================================

fn update_pgs_rates(inner: &mut WebAppInner, snapshot: &Snapshot) {
    let Some(stmts) = snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatStatements(v) = b {
            Some(v)
        } else {
            None
        }
    }) else {
        inner.pgs_rates.clear();
        return;
    };

    if stmts.is_empty() {
        inner.pgs_rates.clear();
        return;
    }

    let now_ts = stmts
        .first()
        .map(|s| s.collected_at)
        .filter(|&t| t > 0)
        .unwrap_or(snapshot.timestamp);

    let Some(prev_ts) = inner.pgs_prev_ts else {
        inner.pgs_prev_ts = Some(now_ts);
        inner.pgs_prev_sample = stmts.iter().map(|s| (s.queryid, s.clone())).collect();
        inner.pgs_rates.clear();
        return;
    };

    if now_ts == prev_ts {
        return;
    }

    if now_ts < prev_ts {
        inner.pgs_prev_ts = Some(now_ts);
        inner.pgs_prev_sample = stmts.iter().map(|s| (s.queryid, s.clone())).collect();
        inner.pgs_rates.clear();
        return;
    }

    let dt = (now_ts - prev_ts) as f64;

    let mut rates = HashMap::with_capacity(stmts.len());
    for s in stmts {
        let mut r = PgStatementsRates {
            dt_secs: dt,
            ..Default::default()
        };
        if let Some(prev) = inner.pgs_prev_sample.get(&s.queryid) {
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

    inner.pgs_rates = rates;
    inner.pgs_prev_ts = Some(now_ts);
    inner.pgs_prev_sample = stmts.iter().map(|s| (s.queryid, s.clone())).collect();
}

fn update_pgt_rates(inner: &mut WebAppInner, snapshot: &Snapshot) {
    let Some(tables) = snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserTables(v) = b {
            Some(v)
        } else {
            None
        }
    }) else {
        inner.pgt_rates.clear();
        return;
    };

    let now_ts = snapshot.timestamp;

    let Some(prev_ts) = inner.pgt_prev_ts else {
        inner.pgt_prev_ts = Some(now_ts);
        inner.pgt_prev_sample = tables.iter().map(|t| (t.relid, t.clone())).collect();
        inner.pgt_rates.clear();
        return;
    };

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
        if let Some(prev) = inner.pgt_prev_sample.get(&t.relid) {
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

    inner.pgt_rates = rates;
    inner.pgt_prev_ts = Some(now_ts);
    inner.pgt_prev_sample = tables.iter().map(|t| (t.relid, t.clone())).collect();
}

fn update_pgi_rates(inner: &mut WebAppInner, snapshot: &Snapshot) {
    let Some(indexes) = snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserIndexes(v) = b {
            Some(v)
        } else {
            None
        }
    }) else {
        inner.pgi_rates.clear();
        return;
    };

    let now_ts = snapshot.timestamp;

    let Some(prev_ts) = inner.pgi_prev_ts else {
        inner.pgi_prev_ts = Some(now_ts);
        inner.pgi_prev_sample = indexes.iter().map(|i| (i.indexrelid, i.clone())).collect();
        inner.pgi_rates.clear();
        return;
    };

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
        if let Some(prev) = inner.pgi_prev_sample.get(&i.indexrelid) {
            r.idx_scan_s = di64(i.idx_scan, prev.idx_scan).map(|d| d as f64 / dt);
            r.idx_tup_read_s = di64(i.idx_tup_read, prev.idx_tup_read).map(|d| d as f64 / dt);
            r.idx_tup_fetch_s = di64(i.idx_tup_fetch, prev.idx_tup_fetch).map(|d| d as f64 / dt);
            r.idx_blks_read_s = di64(i.idx_blks_read, prev.idx_blks_read).map(|d| d as f64 / dt);
            r.idx_blks_hit_s = di64(i.idx_blks_hit, prev.idx_blks_hit).map(|d| d as f64 / dt);
        }
        rates.insert(i.indexrelid, r);
    }

    inner.pgi_rates = rates;
    inner.pgi_prev_ts = Some(now_ts);
    inner.pgi_prev_sample = indexes.iter().map(|i| (i.indexrelid, i.clone())).collect();
}

fn di64(curr: i64, prev: i64) -> Option<i64> {
    (curr >= prev).then_some(curr - prev)
}

fn df64(curr: f64, prev: f64) -> Option<f64> {
    (curr >= prev).then_some(curr - prev)
}

// ============================================================
// Handlers
// ============================================================

type AppState = State<(SharedState, broadcast::Sender<Arc<ApiSnapshot>>)>;

#[utoipa::path(
    get,
    path = "/api/v1/health",
    responses(
        (status = 200, description = "Service is healthy", body = String)
    )
)]
async fn handle_health() -> &'static str {
    "ok"
}

#[utoipa::path(
    get,
    path = "/api/v1/schema",
    responses(
        (status = 200, description = "API schema describing snapshot structure", body = ApiSchema)
    )
)]
async fn handle_schema(State(state_tuple): AppState) -> Json<ApiSchema> {
    let inner = state_tuple.0.lock().unwrap();
    let mode = match inner.mode {
        Mode::Live => ApiMode::Live,
        Mode::History => ApiMode::History,
    };
    let timeline = if inner.mode == Mode::History {
        Some(TimelineInfo {
            start: inner.history_start.unwrap_or(0),
            end: inner.history_end.unwrap_or(0),
            total_snapshots: inner.total_snapshots.unwrap_or(0),
        })
    } else {
        None
    };
    Json(ApiSchema::generate(mode, timeline))
}

#[derive(Deserialize, utoipa::IntoParams)]
struct SnapshotQuery {
    /// Snapshot position index (history mode).
    position: Option<usize>,
    /// Unix timestamp to navigate to (history mode, nearest floor).
    timestamp: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/snapshot",
    params(SnapshotQuery),
    responses(
        (status = 200, description = "Current or historical snapshot", body = ApiSnapshot),
        (status = 400, description = "Invalid position or timestamp"),
        (status = 503, description = "No snapshot available yet")
    )
)]
async fn handle_snapshot(
    State(state_tuple): AppState,
    axum::extract::Query(query): axum::extract::Query<SnapshotQuery>,
) -> Result<axum::response::Response, StatusCode> {
    let state = state_tuple.0;
    // History navigation may call blocking provider methods — run in spawn_blocking
    let snap = tokio::task::spawn_blocking(move || {
        let mut inner = state.lock().unwrap();

        // History navigation via query params
        if inner.mode == Mode::History {
            if let Some(pos) = query.position {
                if !history_jump_to(&mut inner, pos) {
                    return Err(StatusCode::BAD_REQUEST);
                }
            } else if let Some(ts) = query.timestamp {
                if !history_jump_to_timestamp(&mut inner, ts) {
                    return Err(StatusCode::BAD_REQUEST);
                }
            }
        }

        inner
            .current_snapshot
            .clone()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    let json =
        serde_json::to_string(snap.as_ref()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(axum::response::Response::builder()
        .header("content-type", "application/json")
        .body(axum::body::Body::from(json))
        .unwrap())
}

#[utoipa::path(
    get,
    path = "/api/v1/timeline",
    responses(
        (status = 200, description = "History timeline metadata", body = TimelineInfo),
        (status = 404, description = "Not available in live mode")
    )
)]
async fn handle_timeline(State(state_tuple): AppState) -> Result<Json<TimelineInfo>, StatusCode> {
    let inner = state_tuple.0.lock().unwrap();
    if inner.mode != Mode::History {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(TimelineInfo {
        start: inner.history_start.unwrap_or(0),
        end: inner.history_end.unwrap_or(0),
        total_snapshots: inner.total_snapshots.unwrap_or(0),
    }))
}

async fn handle_stream(
    State(state_tuple): AppState,
) -> Result<
    Sse<impl futures_core::Stream<Item = Result<Event, std::convert::Infallible>>>,
    StatusCode,
> {
    let (state, tx) = state_tuple;
    {
        let inner = state.lock().unwrap();
        if inner.mode != Mode::Live {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    let mut rx = tx.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(snapshot) => {
                    match serde_json::to_string(snapshot.as_ref()) {
                        Ok(json) => {
                            yield Ok(Event::default().event("snapshot").data(json));
                        }
                        Err(e) => {
                            error!(error = %e, "failed to serialize snapshot");
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SSE client lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ============================================================
// Frontend static files
// ============================================================

async fn serve_frontend(uri: Uri) -> axum::response::Response<Body> {
    let path = uri.path().trim_start_matches('/');

    // Try exact file match first
    if let Some(file) = FrontendAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return axum::response::Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(file.data.to_vec()))
            .unwrap();
    }

    // SPA fallback: serve index.html for non-file paths
    if let Some(index) = FrontendAssets::get("index.html") {
        return axum::response::Response::builder()
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(index.data.to_vec()))
            .unwrap();
    }

    axum::response::Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("not found"))
        .unwrap()
}

// ============================================================
// Basic Auth middleware
// ============================================================

async fn basic_auth_middleware(
    State(creds): State<Arc<(String, String)>>,
    req: Request,
    next: Next,
) -> axum::response::Response {
    let unauthorized = || {
        axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(header::WWW_AUTHENTICATE, "Basic realm=\"rpglot\"")
            .body(Body::from("Unauthorized"))
            .unwrap()
    };

    let auth_header = match req.headers().get(header::AUTHORIZATION) {
        Some(v) => v,
        None => return unauthorized(),
    };

    let auth_str = match auth_header.to_str() {
        Ok(s) => s,
        Err(_) => return unauthorized(),
    };

    if !auth_str.starts_with("Basic ") {
        return unauthorized();
    }

    use base64::Engine;
    let decoded = match base64::engine::general_purpose::STANDARD.decode(&auth_str[6..]) {
        Ok(d) => d,
        Err(_) => return unauthorized(),
    };

    let decoded_str = match String::from_utf8(decoded) {
        Ok(s) => s,
        Err(_) => return unauthorized(),
    };

    let (user, pass) = match decoded_str.split_once(':') {
        Some(pair) => pair,
        None => return unauthorized(),
    };

    if user != creds.0 || pass != creds.1 {
        return unauthorized();
    }

    next.run(req).await
}

// ============================================================
// OpenAPI documentation
// ============================================================

#[derive(OpenApi)]
#[openapi(
    paths(handle_health, handle_schema, handle_snapshot, handle_timeline),
    components(schemas(
        ApiSnapshot,
        ApiSchema,
        TimelineInfo,
        rpglot_core::api::schema::ApiMode,
        rpglot_core::api::schema::SummarySchema,
        rpglot_core::api::schema::SummarySection,
        rpglot_core::api::schema::FieldSchema,
        rpglot_core::api::schema::TabsSchema,
        rpglot_core::api::schema::TabSchema,
        rpglot_core::api::schema::ColumnSchema,
        rpglot_core::api::schema::ViewSchema,
        rpglot_core::api::schema::DrillDown,
        rpglot_core::api::schema::DataType,
        rpglot_core::api::schema::Unit,
        rpglot_core::api::schema::Format,
        rpglot_core::api::snapshot::SystemSummary,
        rpglot_core::api::snapshot::CpuSummary,
        rpglot_core::api::snapshot::LoadSummary,
        rpglot_core::api::snapshot::MemorySummary,
        rpglot_core::api::snapshot::SwapSummary,
        rpglot_core::api::snapshot::DiskSummary,
        rpglot_core::api::snapshot::NetworkSummary,
        rpglot_core::api::snapshot::PsiSummary,
        rpglot_core::api::snapshot::VmstatSummary,
        rpglot_core::api::snapshot::PgSummary,
        rpglot_core::api::snapshot::BgwriterSummary,
        rpglot_core::api::snapshot::PgActivityRow,
        rpglot_core::api::snapshot::PgStatementsRow,
        rpglot_core::api::snapshot::PgTablesRow,
        rpglot_core::api::snapshot::PgIndexesRow,
        rpglot_core::api::snapshot::PgLocksRow,
    )),
    info(
        title = "rpglot API",
        version = "1.0",
        description = "PostgreSQL monitoring API — real-time and historical system/database snapshots"
    )
)]
struct ApiDoc;
