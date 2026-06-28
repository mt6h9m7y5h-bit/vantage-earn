use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    Json,
};

use serde::Serialize;
use std::net::SocketAddr;

use crate::ad_config::AdConfig;
use crate::middleware::client_ip;
use crate::release_info;
use crate::state::AppState;

#[derive(Serialize)]
pub struct ComponentStatus {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_count: Option<i64>,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub database: bool,
    pub components: HealthComponents,
}

#[derive(Serialize)]
pub struct HealthComponents {
    pub api: ComponentStatus,
    pub database: ComponentStatus,
    pub jobs_queue: ComponentStatus,
    pub ads_provider: ComponentStatus,
    pub payouts: ComponentStatus,
    pub email: ComponentStatus,
}

pub async fn build_health(state: &AppState) -> HealthResponse {
    let (db_ok, db_latency) = state.store_ping_ms().await;
    let ad = AdConfig::default();
    let effective = ad.effective_provider();
    let ads_ok = effective != "mock" || ad.provider == "mock";
    let pending_count = state
        .store
        .pending_payout_request_count()
        .await
        .unwrap_or(0);
    let (email_provider, email_ok, email_note) = state.email.health_status();
    let email_affects_overall = release_info::is_production();

    let overall_ok = db_ok && ads_ok && (!email_affects_overall || email_ok);
    HealthResponse {
        status: if overall_ok { "ok" } else { "degraded" },
        service: "vantage-earn",
        version: release_info::app_version(),
        commit: std::env::var("GIT_COMMIT")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("RENDER_GIT_COMMIT")
                    .ok()
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                let c = release_info::git_commit();
                if c == "dev" {
                    None
                } else {
                    Some(c.to_string())
                }
            }),
        database: db_ok,
        components: HealthComponents {
            api: ComponentStatus {
                status: "ok",
                latency_ms: None,
                note: None,
                provider: None,
                configured: None,
                pending_count: None,
            },
            database: ComponentStatus {
                status: if db_ok { "ok" } else { "error" },
                latency_ms: db_latency,
                note: None,
                provider: None,
                configured: None,
                pending_count: None,
            },
            jobs_queue: ComponentStatus {
                status: "ok",
                latency_ms: None,
                note: Some("stub — Hintergrundjobs nicht angebunden".into()),
                provider: None,
                configured: None,
                pending_count: None,
            },
            ads_provider: ComponentStatus {
                status: if ads_ok { "ok" } else { "degraded" },
                latency_ms: None,
                note: if effective == "mock" && ad.provider != "mock" {
                    Some("Konfigurierter Provider ohne Credentials — Fallback mock".into())
                } else {
                    None
                },
                provider: Some(effective.into()),
                configured: Some(ad.provider == effective),
                pending_count: None,
            },
            payouts: ComponentStatus {
                status: if pending_count > 50 {
                    "degraded"
                } else {
                    "ok"
                },
                latency_ms: None,
                note: None,
                provider: None,
                configured: None,
                pending_count: Some(pending_count),
            },
            email: ComponentStatus {
                status: if email_ok { "ok" } else { "error" },
                latency_ms: None,
                note: email_note,
                provider: Some(email_provider.into()),
                configured: Some(email_ok),
                pending_count: None,
            },
        },
    }
}

#[derive(Serialize)]
pub struct AdminHealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub commit: &'static str,
    pub environment: String,
    pub uptime_secs: i64,
    pub database_latency_ms: Option<u64>,
    pub open_db_connections: Option<i64>,
    pub memory_mb: Option<f64>,
    pub cpu_percent: Option<f64>,
    pub components: HealthComponents,
    pub client_ip: Option<String>,
}

pub async fn build_admin_health(
    state: &AppState,
    headers: &HeaderMap,
    connect_info: Option<SocketAddr>,
) -> AdminHealthResponse {
    let health = build_health(state).await;
    let (mem, cpu) = system_metrics();
    AdminHealthResponse {
        status: health.status,
        version: release_info::app_version(),
        commit: release_info::git_commit(),
        environment: release_info::rust_env(),
        uptime_secs: release_info::uptime_secs(state.started_at),
        database_latency_ms: health.components.database.latency_ms,
        open_db_connections: state.store_open_connections().await.ok().flatten(),
        memory_mb: mem,
        cpu_percent: cpu,
        components: health.components,
        client_ip: client_ip::client_ip(headers, connect_info),
    }
}

fn system_metrics() -> (Option<f64>, Option<f64>) {
    #[cfg(target_os = "linux")]
    {
        let mem = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| parse_mem_available_mb(&s));
        let cpu = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|s| parse_cpu_idle_percent(&s));
        (mem, cpu)
    }
    #[cfg(not(target_os = "linux"))]
    {
        (None, None)
    }
}

#[cfg(target_os = "linux")]
fn parse_mem_available_mb(content: &str) -> Option<f64> {
    for line in content.lines() {
        if line.starts_with("MemAvailable:") {
            let kb: f64 = line
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()?;
            return Some(kb / 1024.0);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn parse_cpu_idle_percent(content: &str) -> Option<f64> {
    let line = content.lines().next()?;
    let parts: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|p| p.parse().ok())
        .collect();
    if parts.len() < 4 {
        return None;
    }
    let idle = parts[3];
    let total: u64 = parts.iter().sum();
    if total == 0 {
        return None;
    }
    Some(((total - idle) as f64 / total as f64) * 100.0)
}

pub async fn public_health(State(state): State<AppState>) -> impl IntoResponse {
    Json(build_health(&state).await)
}

pub async fn admin_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminHealthResponse>, crate::error::ApiError> {
    crate::admin::verify_admin_headers(&headers)?;
    Ok(Json(build_admin_health(&state, &headers, None).await))
}
