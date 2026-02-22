mod auth;
mod background;
mod handlers;
mod openapi;
mod state;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use clap::Parser;
use tokio::sync::broadcast;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

#[cfg(target_os = "linux")]
use rpglot_core::collector::RealFs;
#[cfg(not(target_os = "linux"))]
use rpglot_core::collector::mock::MockFs;
use rpglot_core::collector::{Collector, PostgresCollector};
use rpglot_core::provider::{HistoryProvider, LiveProvider, SnapshotProvider};
use rpglot_core::rates::{PgiRateState, PgpRateState, PgsRateState, PgtRateState};

use auth::{AccessLogLayer, AllowedUsers, SsoConfig, SsoLayer};
use openapi::ApiDoc;
use state::{Mode, SharedState, WebAppInner};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

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

    let (tx, _rx) = broadcast::channel(16);

    let hostname = get_hostname();

    let inner = WebAppInner {
        provider,
        mode,
        current_snapshot: None,
        raw_snapshot: None,
        prev_snapshot: None,
        pgs_rate: PgsRateState::default(),
        pgp_rate: PgpRateState::default(),
        pgt_rate: PgtRateState::default(),
        pgi_rate: PgiRateState::default(),
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
            background::tick_loop(state_clone, tx_clone, interval).await;
        });
    } else {
        // History: skip initial snapshot loading — data loads lazily on first client request
        // Start background refresh for history mode
        if let Some(ref history_path) = args.history {
            let state_clone = state.clone();
            let path = history_path.clone();
            tokio::spawn(async move {
                background::history_refresh_loop(state_clone, path).await;
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
        let pem = std::fs::read(key_path).expect("failed to read SSO public key file");
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
        .route("/api/v1/health", get(handlers::handle_health))
        .route("/api/v1/schema", get(handlers::handle_schema))
        .route("/api/v1/snapshot", get(handlers::handle_snapshot))
        .route("/api/v1/stream", get(handlers::handle_stream))
        .route("/api/v1/timeline", get(handlers::handle_timeline))
        .route(
            "/api/v1/timeline/latest",
            get(handlers::handle_timeline_latest),
        )
        .route("/api/v1/timeline/heatmap", get(handlers::handle_heatmap))
        .route("/api/v1/analysis", get(handlers::handle_analysis))
        .route(
            "/api/v1/auth/config",
            get({
                let url = sso_proxy_url_for_config.clone();
                let user = auth_user_for_config.clone();
                move || handlers::handle_auth_config(url, user)
            }),
        )
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .fallback(get(handlers::serve_frontend))
        .with_state((state, tx));

    // AccessLogLayer goes BEFORE auth layers so it wraps them and can read AuthUser extension
    // (axum layers: last .layer() = outermost; request flows outside-in)
    app = app.layer(AccessLogLayer);

    if let Some(creds) = auth_creds {
        app = app.layer(axum::middleware::from_fn_with_state(
            creds,
            auth::basic_auth_middleware,
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
