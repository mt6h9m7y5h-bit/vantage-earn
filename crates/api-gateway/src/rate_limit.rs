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
use crate::state::AppState;

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
        let default_max = if release_info::is_production() { 20 } else { 120 };
        Self::with_limits(
            std::env::var("AUTH_RATE_LIMIT_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default_max),
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

fn normalize_path(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn is_admin_path(path: &str) -> bool {
    path == "/admin" || path.starts_with("/admin/")
}

fn has_valid_admin_secret(request: &Request<axum::body::Body>) -> bool {
    request
        .headers()
        .get("X-Admin-Secret")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|secret| AppState::verify_admin_secret(Some(secret)).is_ok())
}

/// Paths that bypass rate limiting (PWA shells, health probes, admin polling, public banners).
pub(crate) fn is_exempt(path: &str) -> bool {
    let path = normalize_path(path);
    matches!(
        path,
        "/"
            | "/demo"
            | "/admin"
            | "/health"
            | "/config"
            | "/announcements/active"
            | "/sw.js"
            | "/manifest.webmanifest"
            | "/favicon.ico"
    ) || path.starts_with("/icons/")
        || is_admin_path(path)
        || path.starts_with("/announcements/")
        || (!release_info::is_production() && path.starts_with("/dev/"))
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
    let path = normalize_path(request.uri().path());
    if is_exempt(path) || has_valid_admin_secret(&request) {
        return next.run(request).await;
    }

    if !limiter.allow(&client_key(&request)).await {
        return ApiError(shared::AppError::RateLimited).into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exempt_pwa_and_health_paths() {
        for path in [
            "/",
            "/demo",
            "/admin",
            "/health",
            "/config",
            "/announcements/active",
            "/sw.js",
            "/manifest.webmanifest",
            "/icons/icon-192.png",
        ] {
            assert!(is_exempt(path), "{path} should be exempt");
        }
    }

    #[test]
    fn exempt_admin_api_paths() {
        for path in [
            "/admin/live",
            "/admin/stats",
            "/admin/health",
            "/admin/users?limit=10",
            "/admin/search?q=test",
            "/admin/announcements",
        ] {
            assert!(is_exempt(path), "{path} should be exempt");
        }
    }

    #[test]
    fn exempt_announcements_prefix() {
        assert!(is_exempt("/announcements/active"));
    }

    #[test]
    fn auth_paths_are_rate_limited() {
        assert!(!is_exempt("/auth/register"));
        assert!(!is_exempt("/auth/login"));
        assert!(!is_exempt("/leaderboard/weekly"));
    }

    #[test]
    fn admin_paths_with_query_string_are_exempt() {
        assert!(is_exempt("/admin/live?since=2026-01-01"));
        assert!(is_exempt("/admin/stats"));
        assert!(is_exempt("/admin/health"));
    }

    #[test]
    fn favicon_is_exempt() {
        assert!(is_exempt("/favicon.ico"));
    }
}
