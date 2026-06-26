use chrono::{DateTime, Utc};

pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn git_commit() -> &'static str {
    option_env!("GIT_COMMIT").unwrap_or("dev")
}

pub fn rust_env() -> String {
    std::env::var("RUST_ENV").unwrap_or_else(|_| "development".into())
}

pub fn is_production() -> bool {
    rust_env().eq_ignore_ascii_case("production")
}

pub fn uptime_secs(started_at: DateTime<Utc>) -> i64 {
    Utc::now()
        .signed_duration_since(started_at)
        .num_seconds()
        .max(0)
}

#[derive(serde::Serialize)]
pub struct ReleaseInfo {
    pub version: &'static str,
    pub commit: &'static str,
    pub environment: String,
    pub uptime_secs: i64,
}

pub fn release_info(started_at: DateTime<Utc>) -> ReleaseInfo {
    ReleaseInfo {
        version: app_version(),
        commit: git_commit(),
        environment: rust_env(),
        uptime_secs: uptime_secs(started_at),
    }
}
