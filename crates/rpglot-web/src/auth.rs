//! Authentication and access logging middleware: SSO (JWT), Basic Auth, Access Log.

use std::collections::HashSet;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use tracing::{debug, info, warn};

// ============================================================
// SSO configuration
// ============================================================

pub(crate) enum AllowedUsers {
    Any,
    List(HashSet<String>),
}

pub(crate) struct SsoConfig {
    pub(crate) proxy_url: String,
    pub(crate) decoding_key: jsonwebtoken::DecodingKey,
    pub(crate) validation: jsonwebtoken::Validation,
    pub(crate) allowed_users: AllowedUsers,
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
pub(crate) struct SsoLayer {
    pub(crate) config: Arc<SsoConfig>,
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
pub(crate) struct SsoService<S> {
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
pub(crate) struct AccessLogLayer;

impl<S> tower::Layer<S> for AccessLogLayer {
    type Service = AccessLogService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        AccessLogService { inner }
    }
}

/// Authenticated username, inserted into request extensions by auth middleware.
#[derive(Clone)]
pub(crate) struct AuthUser(pub(crate) String);

#[derive(Clone)]
pub(crate) struct AccessLogService<S> {
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

pub(crate) async fn basic_auth_middleware(
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
