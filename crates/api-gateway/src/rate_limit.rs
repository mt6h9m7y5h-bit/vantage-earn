use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{ConnectInfo, State},
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use tokio::sync::Mutex;

use crate::error::ApiError;
use crate::release_info;

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    pub fn from_env() -> Self {
        Self::with_limits(
            std::env::var("RATE_LIMIT_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            std::env::var("RATE_LIMIT_WINDOW_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        )
    }

    pub fn auth_from_env() -> Self {
        Self::with_limits(
            std::env::var("AUTH_RATE_LIMIT_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20),
            std::env::var("RATE_LIMIT_WINDOW_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        )
    }

    fn with_limits(max_requests: usize, window_secs: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window: Duration::from_secs(window_secs),
        }
    }

    async fn allow(&self, key: &str) -> bool {
        let mut map = self.inner.lock().await;
        let now = Instant::now();
        let entries = map.entry(key.to_string()).or_default();
        entries.retain(|t| now.duration_since(*t) < self.window);
        if entries.len() >= self.max_requests {
            return false;
        }
        entries.push(now);
        true
    }
}

fn client_key(request: &Request<axum::body::Body>) -> String {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "local".into())
}

pub async fn middleware(
    State(limiter): State<RateLimiter>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    // PWA shell + static assets: never rate-limit (dev reloads /demo often).
    // Admin API: dashboard polls /admin/live every 8s plus stats/health — must not 429.
    // Dev routes: seed-demo / reset in local development only.
    if matches!(
        path,
        "/"
            | "/demo"
            | "/admin"
            | "/health"
            | "/config"
            | "/sw.js"
            | "/manifest.webmanifest"
    ) || path.starts_with("/icons/")
        || path.starts_with("/admin/")
        || (!release_info::is_production() && path.starts_with("/dev/"))
    {
        return next.run(request).await;
    }

    if !limiter.allow(&client_key(&request)).await {
        return ApiError(shared::AppError::RateLimited).into_response();
    }
    next.run(request).await
}
