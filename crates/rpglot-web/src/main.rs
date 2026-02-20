#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

/// Releases unused memory back to the operating system.
/// Uses jemalloc's arena purge to reduce RSS after memory-intensive operations.
#[cfg(not(target_env = "msvc"))]
fn release_memory_to_os() {
    unsafe {
        // MALLCTL_ARENAS_ALL = 4096: purge dirty pages from ALL jemalloc arenas.
        // Using arena.0 only purges one arena, missing allocations from tokio worker threads.
        tikv_jemalloc_sys::mallctl(
            c"arena.4096.purge".as_ptr().cast(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
    }
}

#[cfg(target_env = "msvc")]
fn release_memory_to_os() {}

use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::Infallible;
use std::fs;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::process;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
use tracing::{debug, error, info, warn};

use axum::body::Body;
use axum::extract::Request;
use axum::http::{Uri, header};
use axum::middleware::Next;
use rpglot_core::api::convert::{ConvertContext, convert, resolve};
use rpglot_core::api::schema::{ApiMode, ApiSchema, DateInfo, InstanceInfo, TimelineInfo};
use rpglot_core::api::snapshot::{ApiSnapshot, PgStatementsRow, PgStorePlansRow};
#[cfg(target_os = "linux")]
use rpglot_core::collector::RealFs;
#[cfg(not(target_os = "linux"))]
use rpglot_core::collector::mock::MockFs;
use rpglot_core::collector::{Collector, PostgresCollector};
use rpglot_core::provider::{HistoryProvider, LiveProvider, SnapshotProvider};
use rpglot_core::storage::StringInterner;
use rpglot_core::storage::model::{DataBlock, PgStatStatementsInfo, PgStorePlansInfo, Snapshot};
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
#[command(name = "rpglot-web", about = "rpglot web API server", version = rpglot_core::VERSION)]
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

    /// SSO proxy URL for token acquisition (enables SSO when set).
    #[arg(long, env = "RPGLOT_SSO_PROXY_URL")]
    sso_proxy_url: Option<String>,

    /// Path to PEM file with public key for JWT signature verification.
    #[arg(long, env = "RPGLOT_SSO_PROXY_KEY_FILE")]
    sso_proxy_key_file: Option<PathBuf>,

    /// Comma-separated list of accepted JWT audience values.
    #[arg(long, env = "RPGLOT_SSO_PROXY_AUDIENCE", value_delimiter = ',')]
    sso_proxy_audience: Vec<String>,

    /// Comma-separated list of allowed usernames, or "*" for any authenticated user.
    #[arg(
        long,
        env = "RPGLOT_SSO_PROXY_ALLOWED_USERS",
        default_value = "*",
        value_delimiter = ','
    )]
    sso_proxy_allowed_users: Vec<String>,
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
    pgs_rate: rpglot_core::rates::PgsRateState,
    pgp_rate: rpglot_core::rates::PgpRateState,
    pgt_rate: rpglot_core::rates::PgtRateState,
    pgi_rate: rpglot_core::rates::PgiRateState,
    // History metadata (updated by refresh task)
    total_snapshots: Option<usize>,
    history_start: Option<i64>,
    history_end: Option<i64>,
    // Heatmap cache: per-date ("YYYY-MM-DD" → bucketed data).
    // Past dates are immutable — cached forever. Today invalidated on refresh.
    heatmap_cache: HashMap<String, Vec<rpglot_core::storage::heatmap::HeatmapBucket>>,
    // Instance metadata (database name + PG version + is_standby), cached from provider.
    instance_info: Option<(String, String, Option<bool>)>,
    // Machine hostname, obtained at startup.
    hostname: String,
}

type SharedState = Arc<Mutex<WebAppInner>>;

// ============================================================
// SSO configuration
// ============================================================

enum AllowedUsers {
    Any,
    List(HashSet<String>),
}

struct SsoConfig {
    proxy_url: String,
    decoding_key: jsonwebtoken::DecodingKey,
    validation: jsonwebtoken::Validation,
    allowed_users: AllowedUsers,
}

// ============================================================
// Main
// ============================================================

fn main() {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rpglot_web=info".parse().unwrap()),
        )
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async_main(args));
}

async fn async_main(args: Args) {
    #[allow(clippy::type_complexity)]
    let (provider, mode, total_snapshots, history_start, history_end): (
        Box<dyn SnapshotProvider + Send>,
        Mode,
        Option<usize>,
        Option<i64>,
        Option<i64>,
    ) = if let Some(ref history_path) = args.history {
        info!(version = rpglot_core::VERSION, path = %history_path.display(), "starting in history mode");
        let hp = match HistoryProvider::from_path_lazy(history_path) {
            Ok(hp) => hp,
            Err(e) => {
                error!(path = %history_path.display(), error = %e,
                    "failed to open history data (no snapshots yet? wrong format?)");
                process::exit(1);
            }
        };
        // Fully lazy: no disk scanning at startup. Index builds on first client request.
        info!("history mode ready (lazy init on first request)");
        (Box::new(hp), Mode::History, None, None, None)
    } else {
        info!(version = rpglot_core::VERSION, "starting in live mode");
        let provider = create_live_provider(&args);
        (provider, Mode::Live, None, None, None)
    };

    let (tx, _rx) = broadcast::channel::<Arc<ApiSnapshot>>(16);

    let hostname = get_hostname();

    let inner = WebAppInner {
        provider,
        mode,
        current_snapshot: None,
        raw_snapshot: None,
        prev_snapshot: None,
        pgs_rate: rpglot_core::rates::PgsRateState::default(),
        pgp_rate: rpglot_core::rates::PgpRateState::default(),
        pgt_rate: rpglot_core::rates::PgtRateState::default(),
        pgi_rate: rpglot_core::rates::PgiRateState::default(),
        total_snapshots,
        history_start,
        history_end,
        heatmap_cache: HashMap::new(),
        instance_info: None,
        hostname,
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
        // History: skip initial snapshot loading — data loads lazily on first client request
        // Start background refresh for history mode
        if let Some(ref history_path) = args.history {
            let state_clone = state.clone();
            let path = history_path.clone();
            tokio::spawn(async move {
                history_refresh_loop(state_clone, path).await;
            });
        }
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

    // SSO
    let sso_config: Option<Arc<SsoConfig>> = if let Some(ref proxy_url) = args.sso_proxy_url {
        let key_path = args
            .sso_proxy_key_file
            .as_ref()
            .expect("--sso-proxy-key-file required when --sso-proxy-url is set");
        let pem = fs::read(key_path).expect("failed to read SSO public key file");
        let decoding_key =
            jsonwebtoken::DecodingKey::from_rsa_pem(&pem).expect("invalid PEM public key");

        assert!(
            !args.sso_proxy_audience.is_empty(),
            "--sso-proxy-audience required when --sso-proxy-url is set"
        );

        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_audience(&args.sso_proxy_audience);
        validation.validate_exp = true;

        let allowed_users = if args.sso_proxy_allowed_users == ["*"] {
            AllowedUsers::Any
        } else {
            AllowedUsers::List(args.sso_proxy_allowed_users.iter().cloned().collect())
        };

        info!(
            proxy_url,
            audiences = ?args.sso_proxy_audience,
            "SSO enabled"
        );
        Some(Arc::new(SsoConfig {
            proxy_url: proxy_url.clone(),
            decoding_key,
            validation,
            allowed_users,
        }))
    } else {
        None
    };

    // SSO and Basic Auth are mutually exclusive
    if auth_creds.is_some() && sso_config.is_some() {
        panic!("--auth-user and --sso-proxy-url are mutually exclusive");
    }

    // SSO proxy URL and auth user for /api/v1/auth/config (accessible without auth)
    let sso_proxy_url_for_config: Arc<Option<String>> =
        Arc::new(sso_config.as_ref().map(|c| c.proxy_url.clone()));
    let auth_user_for_config: Arc<Option<String>> =
        Arc::new(auth_creds.as_ref().map(|c| c.0.clone()));

    // Router
    let mut app = Router::new()
        .route("/api/v1/health", get(handle_health))
        .route("/api/v1/schema", get(handle_schema))
        .route("/api/v1/snapshot", get(handle_snapshot))
        .route("/api/v1/stream", get(handle_stream))
        .route("/api/v1/timeline", get(handle_timeline))
        .route("/api/v1/timeline/latest", get(handle_timeline_latest))
        .route("/api/v1/timeline/heatmap", get(handle_heatmap))
        .route("/api/v1/analysis", get(handle_analysis))
        .route(
            "/api/v1/auth/config",
            get({
                let url = sso_proxy_url_for_config.clone();
                let user = auth_user_for_config.clone();
                move || handle_auth_config(url, user)
            }),
        )
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .fallback(get(serve_frontend))
        .with_state((state, tx));

    // AccessLogLayer goes BEFORE auth layers so it wraps them and can read AuthUser extension
    // (axum layers: last .layer() = outermost; request flows outside-in)
    app = app.layer(AccessLogLayer);

    if let Some(creds) = auth_creds {
        app = app.layer(axum::middleware::from_fn_with_state(
            creds,
            basic_auth_middleware,
        ));
    }

    if let Some(sso) = sso_config {
        app = app.layer(SsoLayer {
            config: sso.clone(),
        });
    }

    let app = app
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new());

    let app = app.into_make_service_with_connect_info::<SocketAddr>();

    let addr: SocketAddr = args.listen.parse().expect("invalid listen address");
    info!(%addr, "listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");

    axum::serve(listener, app).await.expect("server error");
}

/// Get machine hostname via the `hostname` command.
fn get_hostname() -> String {
    process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
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

/// Background loop: idle eviction + refresh history snapshots from disk.
///
/// Does NO work until a client has connected at least once. After client leaves,
/// evicts all data after IDLE_EVICT_SECS. Only refreshes during active use.
async fn history_refresh_loop(state: SharedState, path: PathBuf) {
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
fn ensure_history_ready(inner: &mut WebAppInner) -> bool {
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
    rpglot_core::rates::update_pgs_rates(&mut inner.pgs_rate, &snapshot);
    rpglot_core::rates::update_pgp_rates(&mut inner.pgp_rate, &snapshot);
    rpglot_core::rates::update_pgt_rates(&mut inner.pgt_rate, &snapshot);
    rpglot_core::rates::update_pgi_rates(&mut inner.pgi_rate, &snapshot);

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

/// Navigate history provider to timestamp and reconvert.
fn history_jump_to_timestamp(inner: &mut WebAppInner, timestamp: i64, ceil: bool) -> bool {
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
    let max_lookback = 30; // PGT cached ~30s, need ~300s / 5min lookback
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
    let max_lookback = 30; // PGI cached ~30s, need ~300s / 5min lookback
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

/// Reconvert current provider snapshot to ApiSnapshot (after history jump).
/// Uses the adjacent previous snapshot (position-1) to compute rates and system deltas.
fn reconvert_current(inner: &mut WebAppInner) {
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
        rpglot_core::rates::update_pgt_rates(&mut inner.pgt_rate, &snapshot);

        // PGI
        let pgi_seed = extract_pgi_collected_at(&snapshot).and_then(|curr_ts| {
            hp_mut!(inner).and_then(|hp| find_pgi_prev_snapshot(hp, position, curr_ts))
        });
        let pgi_prev = pgi_seed.as_ref().unwrap_or(prev);
        seed_pgi_prev(inner, pgi_prev);
        rpglot_core::rates::update_pgi_rates(&mut inner.pgi_rate, &snapshot);

        // PGS
        let pgs_seed = extract_pgs_collected_at(&snapshot).and_then(|curr_ts| {
            hp_mut!(inner).and_then(|hp| find_pgs_prev_snapshot(hp, position, curr_ts))
        });
        let pgs_prev = pgs_seed.as_ref().unwrap_or(prev);
        seed_pgs_prev(inner, pgs_prev);
        rpglot_core::rates::update_pgs_rates(&mut inner.pgs_rate, &snapshot);

        // PGP
        let pgp_seed = extract_pgp_collected_at(&snapshot).and_then(|curr_ts| {
            hp_mut!(inner).and_then(|hp| find_pgp_prev_snapshot(hp, position, curr_ts))
        });
        let pgp_prev = pgp_seed.as_ref().unwrap_or(prev);
        seed_pgp_prev(inner, pgp_prev);
        rpglot_core::rates::update_pgp_rates(&mut inner.pgp_rate, &snapshot);
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
    LAST_CLIENT_ACTIVITY.store(now_epoch(), Ordering::Relaxed);
    let mut inner = state_tuple.0.lock().unwrap();
    // Lazy init: build chunk index on first schema request (frontend's first call)
    if inner.mode == Mode::History {
        ensure_history_ready(&mut inner);
    }
    let mode = match inner.mode {
        Mode::Live => ApiMode::Live,
        Mode::History => ApiMode::History,
    };
    let timeline = if inner.mode == Mode::History {
        Some(TimelineInfo {
            start: inner.history_start.unwrap_or(0),
            end: inner.history_end.unwrap_or(0),
            total_snapshots: inner.total_snapshots.unwrap_or(0),
            dates: None, // lightweight — dates available via /api/v1/timeline
        })
    } else {
        None
    };
    let hostname = inner.hostname.clone();
    let instance = inner
        .instance_info
        .as_ref()
        .map(|(db, ver, is_standby)| InstanceInfo {
            database: db.clone(),
            pg_version: ver.clone(),
            is_standby: *is_standby,
            hostname: if hostname.is_empty() {
                None
            } else {
                Some(hostname.clone())
            },
        });
    Json(ApiSchema::generate(mode, timeline, instance))
}

#[derive(Deserialize, utoipa::IntoParams)]
struct SnapshotQuery {
    /// Unix timestamp to navigate to (history mode).
    timestamp: Option<i64>,
    /// Direction for timestamp lookup: "floor" (default, latest snapshot <= ts)
    /// or "ceil" (earliest snapshot >= ts).
    direction: Option<String>,
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
    LAST_CLIENT_ACTIVITY.store(now_epoch(), Ordering::Relaxed);
    let state = state_tuple.0;
    // History navigation may call blocking provider methods — run in spawn_blocking
    let snap = tokio::task::spawn_blocking(move || {
        let mut inner = state.lock().unwrap();

        // Lazy init: build chunk index if needed (after idle eviction or first request)
        if inner.mode == Mode::History {
            ensure_history_ready(&mut inner);
        }

        // History navigation via query params
        if inner.mode == Mode::History
            && let Some(ts) = query.timestamp
        {
            let use_ceil = query.direction.as_deref() == Some("ceil");
            if !history_jump_to_timestamp(&mut inner, ts, use_ceil) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }

        // Lazy loading after idle eviction: reload current snapshot if needed
        if inner.mode == Mode::History && inner.current_snapshot.is_none() {
            reconvert_current(&mut inner);
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
    LAST_CLIENT_ACTIVITY.store(now_epoch(), Ordering::Relaxed);
    let mut inner = state_tuple.0.lock().unwrap();
    if inner.mode != Mode::History {
        return Err(StatusCode::NOT_FOUND);
    }
    ensure_history_ready(&mut inner);
    let dates = {
        let provider = inner
            .provider
            .as_any()
            .and_then(|a| a.downcast_ref::<HistoryProvider>());
        provider.map(compute_dates_index)
    };
    Ok(Json(TimelineInfo {
        start: inner.history_start.unwrap_or(0),
        end: inner.history_end.unwrap_or(0),
        total_snapshots: inner.total_snapshots.unwrap_or(0),
        dates,
    }))
}

// Lightweight struct for /api/v1/timeline/latest (O(1), no date index computation).
#[derive(serde::Serialize)]
struct TimelineLatest {
    end: i64,
    total_snapshots: usize,
}

async fn handle_timeline_latest(
    State(state_tuple): AppState,
) -> Result<Json<TimelineLatest>, StatusCode> {
    LAST_CLIENT_ACTIVITY.store(now_epoch(), Ordering::Relaxed);
    let inner = state_tuple.0.lock().unwrap();
    if inner.mode != Mode::History {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(TimelineLatest {
        end: inner.history_end.unwrap_or(0),
        total_snapshots: inner.total_snapshots.unwrap_or(0),
    }))
}

// ============================================================
// Analysis endpoint
// ============================================================

#[derive(serde::Deserialize)]
struct AnalysisQuery {
    start: i64,
    end: i64,
}

async fn handle_analysis(
    State(state_tuple): AppState,
    axum::extract::Query(query): axum::extract::Query<AnalysisQuery>,
) -> Result<Json<rpglot_core::analysis::AnalysisReport>, StatusCode> {
    LAST_CLIENT_ACTIVITY.store(now_epoch(), Ordering::Relaxed);
    if query.end <= query.start {
        return Err(StatusCode::BAD_REQUEST);
    }

    let state = state_tuple.0.clone();

    tokio::task::spawn_blocking(move || {
        let mut inner = state.lock().unwrap();
        if inner.mode != Mode::History {
            return Err(StatusCode::NOT_FOUND);
        }
        ensure_history_ready(&mut inner);

        let provider = inner
            .provider
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<HistoryProvider>())
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        let analyzer = rpglot_core::analysis::Analyzer::new();
        let report = analyzer.analyze(provider, query.start, query.end);
        Ok(Json(report))
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

/// Build a per-date index from HistoryProvider timestamps (no snapshot loading).
fn compute_dates_index(hp: &HistoryProvider) -> Vec<DateInfo> {
    struct DateAcc {
        count: usize,
        first_timestamp: i64,
        last_timestamp: i64,
    }

    let mut map: BTreeMap<String, DateAcc> = BTreeMap::new();
    for &ts in hp.timestamps() {
        let days = ts / 86400;
        let date_str = {
            let d = chrono_free_date(days);
            format!("{:04}-{:02}-{:02}", d.0, d.1, d.2)
        };
        map.entry(date_str)
            .and_modify(|acc| {
                acc.count += 1;
                acc.last_timestamp = ts;
            })
            .or_insert(DateAcc {
                count: 1,
                first_timestamp: ts,
                last_timestamp: ts,
            });
    }
    map.into_iter()
        .map(|(date, acc)| DateInfo {
            date,
            count: acc.count,
            first_timestamp: acc.first_timestamp,
            last_timestamp: acc.last_timestamp,
        })
        .collect()
}

/// Convert days-since-epoch to (year, month, day) without chrono crate.
fn chrono_free_date(days_since_epoch: i64) -> (i32, u32, u32) {
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

// ============================================================
// Heatmap
// ============================================================

#[derive(Deserialize, utoipa::IntoParams)]
struct HeatmapQuery {
    /// Start timestamp (epoch seconds).
    start: i64,
    /// End timestamp (epoch seconds).
    end: i64,
    /// Number of buckets (default: 400, max: 1000).
    buckets: Option<usize>,
}

/// Get activity heatmap data for a time range (history mode only).
#[utoipa::path(
    get,
    path = "/api/v1/timeline/heatmap",
    params(HeatmapQuery),
    responses(
        (status = 200, description = "Heatmap bucket data"),
        (status = 404, description = "Not available in live mode")
    )
)]
async fn handle_heatmap(
    State(state_tuple): AppState,
    axum::extract::Query(query): axum::extract::Query<HeatmapQuery>,
) -> Result<Json<Vec<rpglot_core::storage::heatmap::HeatmapBucket>>, StatusCode> {
    LAST_CLIENT_ACTIVITY.store(now_epoch(), Ordering::Relaxed);
    let num_buckets = query.buckets.unwrap_or(400).min(1000);

    if query.end <= query.start {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Derive date key for caching
    let days = query.start / 86400;
    let d = chrono_free_date(days);
    let date_key = format!("{:04}-{:02}-{:02}", d.0, d.1, d.2);

    // Check cache first
    {
        let mut inner = state_tuple.0.lock().unwrap();
        if inner.mode != Mode::History {
            return Err(StatusCode::NOT_FOUND);
        }
        ensure_history_ready(&mut inner);
        // Use cached data for past dates (they are immutable)
        let today_days = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            / 86400;
        let is_past_date = days < today_days;
        if is_past_date && let Some(cached) = inner.heatmap_cache.get(&date_key) {
            return Ok(Json(cached.clone()));
        }
    }

    // Compute heatmap (potentially expensive — spawn_blocking)
    let state = state_tuple.0.clone();
    let buckets = tokio::task::spawn_blocking(move || {
        let mut inner = state.lock().unwrap();
        let hp = inner
            .provider
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<HistoryProvider>())
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        let raw = hp.load_heatmap_range(query.start, query.end);
        let buckets = rpglot_core::storage::heatmap::bucket_heatmap(
            &raw,
            query.start,
            query.end,
            num_buckets,
        );

        // Cache the result
        inner.heatmap_cache.insert(date_key, buckets.clone());

        Ok::<_, StatusCode>(buckets)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    Ok(Json(buckets))
}

static LAST_CLIENT_ACTIVITY: AtomicI64 = AtomicI64::new(0);

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

static SSE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

struct SseGuard;

impl Drop for SseGuard {
    fn drop(&mut self) {
        let active = SSE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed) - 1;
        info!(active_connections = active, "SSE client disconnected");
    }
}

async fn handle_stream(
    State(state_tuple): AppState,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let (state, tx) = state_tuple;
    {
        let inner = state.lock().unwrap();
        if inner.mode != Mode::Live {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    let active = SSE_CONNECTIONS.fetch_add(1, Ordering::Relaxed) + 1;
    info!(active_connections = active, "SSE client connected");

    let mut rx = tx.subscribe();

    let stream = async_stream::stream! {
        let _guard = SseGuard;
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
// Auth config endpoint (public, no auth required)
// ============================================================

#[derive(serde::Serialize)]
struct AuthConfig {
    sso_proxy_url: Option<String>,
    auth_user: Option<String>,
}

async fn handle_auth_config(
    sso_proxy_url: Arc<Option<String>>,
    auth_user: Arc<Option<String>>,
) -> Json<AuthConfig> {
    Json(AuthConfig {
        sso_proxy_url: (*sso_proxy_url).clone(),
        auth_user: (*auth_user).clone(),
    })
}

// ============================================================
// SSO middleware (JWT validation)
// ============================================================

#[derive(serde::Deserialize)]
struct SsoClaims {
    preferred_username: Option<String>,
    sub: Option<String>,
}

fn extract_token(req: &Request) -> Option<String> {
    // 1. Authorization: Bearer <token>
    if let Some(auth) = req.headers().get(header::AUTHORIZATION)
        && let Ok(s) = auth.to_str()
        && let Some(token) = s.strip_prefix("Bearer ")
    {
        return Some(token.to_owned());
    }
    // 2. Query param ?token=<token>
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("token=") {
                return Some(val.to_owned());
            }
        }
    }
    // 3. Cookie sso_access_token=<token>
    if let Some(cookie_header) = req.headers().get(header::COOKIE)
        && let Ok(s) = cookie_header.to_str()
    {
        for part in s.split(';') {
            let part = part.trim();
            if let Some(val) = part.strip_prefix("sso_access_token=") {
                return Some(val.to_owned());
            }
        }
    }
    None
}

fn unauthorized_json() -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"error":"unauthorized"}"#))
        .unwrap()
}

fn forbidden_json(username: &str) -> axum::response::Response {
    let body = serde_json::json!({"error": "forbidden", "username": username}).to_string();
    axum::response::Response::builder()
        .status(StatusCode::FORBIDDEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[derive(Clone)]
struct SsoLayer {
    config: Arc<SsoConfig>,
}

impl<S> tower::Layer<S> for SsoLayer {
    type Service = SsoService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        SsoService {
            inner,
            config: self.config.clone(),
        }
    }
}

#[derive(Clone)]
struct SsoService<S> {
    inner: S,
    config: Arc<SsoConfig>,
}

impl<S> tower::Service<Request> for SsoService<S>
where
    S: tower::Service<Request, Response = axum::response::Response> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        // Skip auth for public endpoints
        let path = req.uri().path();
        if path == "/api/v1/auth/config" || path == "/api/v1/health" {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(req).await });
        }
        // Skip for non-API paths: static assets, index.html, favicon, etc.
        // Frontend handles auth in JS (fetches /api/v1/auth/config, then redirects to SSO).
        if !path.starts_with("/api/") {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(req).await });
        }

        let config = self.config.clone();
        let mut inner = self.inner.clone();
        let req_path = path.to_owned();

        Box::pin(async move {
            let token = match extract_token(&req) {
                Some(t) => t,
                None => {
                    warn!(path = %req_path, "SSO: no token");
                    return Ok(unauthorized_json());
                }
            };

            let claims = match jsonwebtoken::decode::<SsoClaims>(
                &token,
                &config.decoding_key,
                &config.validation,
            ) {
                Ok(data) => data.claims,
                Err(e) => {
                    warn!(error = %e, path = %req_path, "SSO: invalid token");
                    return Ok(unauthorized_json());
                }
            };

            let username = claims.preferred_username.or(claims.sub).unwrap_or_default();

            match &config.allowed_users {
                AllowedUsers::Any => {}
                AllowedUsers::List(set) => {
                    if !set.contains(&username) {
                        warn!(user = %username, path = %req_path, "SSO: user not allowed");
                        return Ok(forbidden_json(&username));
                    }
                }
            }

            debug!(user = %username, path = %req_path, "SSO: authenticated");
            req.extensions_mut().insert(AuthUser(username));
            inner.call(req).await
        })
    }
}

// ============================================================
// Access log layer (tower Layer + Service)
// ============================================================

#[derive(Clone)]
struct AccessLogLayer;

impl<S> tower::Layer<S> for AccessLogLayer {
    type Service = AccessLogService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        AccessLogService { inner }
    }
}

/// Authenticated username, inserted into request extensions by auth middleware.
#[derive(Clone)]
struct AuthUser(String);

#[derive(Clone)]
struct AccessLogService<S> {
    inner: S,
}

impl<S> tower::Service<Request> for AccessLogService<S>
where
    S: tower::Service<Request, Response = axum::response::Response> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let method = req.method().clone();
        let path = req.uri().path().to_owned();
        let client = req
            .extensions()
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip().to_string())
            .unwrap_or_else(|| "-".to_owned());
        let user = req
            .extensions()
            .get::<AuthUser>()
            .map(|u| u.0.clone())
            .unwrap_or_else(|| "-".to_owned());
        let t0 = Instant::now();

        let mut inner = self.inner.clone();
        Box::pin(async move {
            let response = inner.call(req).await?;
            let latency_ms = t0.elapsed().as_millis() as u64;
            let status = response.status().as_u16();
            if !path.starts_with("/assets/") && path != "/favicon.ico" {
                info!(client, user, status, latency_ms, "{method} {path}");
            }
            Ok(response)
        })
    }
}

// ============================================================
// Basic Auth middleware
// ============================================================

async fn basic_auth_middleware(
    State(creds): State<Arc<(String, String)>>,
    mut req: Request,
    next: Next,
) -> axum::response::Response {
    let path = req.uri().path().to_owned();

    let unauthorized = || {
        axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(header::WWW_AUTHENTICATE, "Basic realm=\"rpglot\"")
            .body(Body::from("Unauthorized"))
            .unwrap()
    };

    let auth_header = match req.headers().get(header::AUTHORIZATION) {
        Some(v) => v,
        None => {
            warn!(path = %path, "auth failed: no authorization header");
            return unauthorized();
        }
    };

    let auth_str = match auth_header.to_str() {
        Ok(s) => s,
        Err(_) => {
            warn!(path = %path, "auth failed: invalid header encoding");
            return unauthorized();
        }
    };

    if !auth_str.starts_with("Basic ") {
        warn!(path = %path, "auth failed: not basic auth");
        return unauthorized();
    }

    use base64::Engine;
    let decoded = match base64::engine::general_purpose::STANDARD.decode(&auth_str[6..]) {
        Ok(d) => d,
        Err(_) => {
            warn!(path = %path, "auth failed: invalid base64");
            return unauthorized();
        }
    };

    let decoded_str = match String::from_utf8(decoded) {
        Ok(s) => s,
        Err(_) => {
            warn!(path = %path, "auth failed: invalid utf8");
            return unauthorized();
        }
    };

    let (user, pass) = match decoded_str.split_once(':') {
        Some(pair) => pair,
        None => {
            warn!(path = %path, "auth failed: malformed credentials");
            return unauthorized();
        }
    };

    if user != creds.0 || pass != creds.1 {
        warn!(user = %user, path = %path, "auth failed: invalid credentials");
        return unauthorized();
    }

    debug!(user = %user, path = %path, "authenticated");
    req.extensions_mut().insert(AuthUser(user.to_owned()));
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
        DateInfo,
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
        rpglot_core::api::snapshot::PgStorePlansRow,
        rpglot_core::api::snapshot::PgLocksRow,
        rpglot_core::api::snapshot::ReplicationInfo,
        rpglot_core::api::snapshot::ReplicaDetail,
    )),
    info(
        title = "rpglot API",
        version = "1.0",
        description = "PostgreSQL monitoring API — real-time and historical system/database snapshots"
    )
)]
struct ApiDoc;
