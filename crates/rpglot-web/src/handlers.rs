//! HTTP request handlers: API endpoints, SSE streaming, and frontend serving.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::Json;
use axum::response::sse::{Event, KeepAlive, Sse};
use rust_embed::Embed;
use serde::Deserialize;
use tracing::{error, info, warn};

use rpglot_core::api::schema::{ApiMode, ApiSchema, DateInfo, InstanceInfo, TimelineInfo};
use rpglot_core::api::snapshot::ApiSnapshot;
use rpglot_core::provider::HistoryProvider;
use rpglot_core::storage::heatmap::HeatmapBucket;

use crate::background::{
    chrono_free_date, ensure_history_ready, history_jump_to_timestamp, reconvert_current,
};
use crate::state::{AppState, LAST_CLIENT_ACTIVITY, Mode, SSE_CONNECTIONS, now_epoch};

// ============================================================
// Embedded frontend assets
// ============================================================

#[derive(Embed)]
#[folder = "frontend/dist"]
struct FrontendAssets;

// ============================================================
// Health
// ============================================================

#[utoipa::path(
    get,
    path = "/api/v1/health",
    responses(
        (status = 200, description = "Service is healthy", body = String)
    )
)]
pub(crate) async fn handle_health() -> &'static str {
    "ok"
}

// ============================================================
// Schema
// ============================================================

#[utoipa::path(
    get,
    path = "/api/v1/schema",
    responses(
        (status = 200, description = "API schema describing snapshot structure", body = ApiSchema)
    )
)]
pub(crate) async fn handle_schema(State(state_tuple): AppState) -> Json<ApiSchema> {
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

// ============================================================
// Snapshot
// ============================================================

#[derive(Deserialize, utoipa::IntoParams)]
pub(crate) struct SnapshotQuery {
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
pub(crate) async fn handle_snapshot(
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
        .body(Body::from(json))
        .unwrap())
}

// ============================================================
// Timeline
// ============================================================

#[utoipa::path(
    get,
    path = "/api/v1/timeline",
    responses(
        (status = 200, description = "History timeline metadata", body = TimelineInfo),
        (status = 404, description = "Not available in live mode")
    )
)]
pub(crate) async fn handle_timeline(
    State(state_tuple): AppState,
) -> Result<Json<TimelineInfo>, StatusCode> {
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
pub(crate) struct TimelineLatest {
    end: i64,
    total_snapshots: usize,
}

pub(crate) async fn handle_timeline_latest(
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

// ============================================================
// Analysis
// ============================================================

#[derive(Deserialize)]
pub(crate) struct AnalysisQuery {
    start: i64,
    end: i64,
}

pub(crate) async fn handle_analysis(
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

// ============================================================
// Heatmap
// ============================================================

#[derive(Deserialize, utoipa::IntoParams)]
pub(crate) struct HeatmapQuery {
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
pub(crate) async fn handle_heatmap(
    State(state_tuple): AppState,
    axum::extract::Query(query): axum::extract::Query<HeatmapQuery>,
) -> Result<Json<Vec<HeatmapBucket>>, StatusCode> {
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

// ============================================================
// SSE streaming (live mode)
// ============================================================

struct SseGuard;

impl Drop for SseGuard {
    fn drop(&mut self) {
        let active = SSE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed) - 1;
        info!(active_connections = active, "SSE client disconnected");
    }
}

pub(crate) async fn handle_stream(
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
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SSE client lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
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

pub(crate) async fn serve_frontend(uri: Uri) -> axum::response::Response<Body> {
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
pub(crate) struct AuthConfig {
    sso_proxy_url: Option<String>,
    auth_user: Option<String>,
}

pub(crate) async fn handle_auth_config(
    sso_proxy_url: Arc<Option<String>>,
    auth_user: Arc<Option<String>>,
) -> Json<AuthConfig> {
    Json(AuthConfig {
        sso_proxy_url: (*sso_proxy_url).clone(),
        auth_user: (*auth_user).clone(),
    })
}
