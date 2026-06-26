use axum::{
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Redirect, Response},
};
use std::path::PathBuf;

const NO_CACHE: [(&str, &str); 1] = [("cache-control", "no-cache, no-store, must-revalidate")];

const DEMO_HTML: &str = include_str!("../../../frontend/index.html");
const ADMIN_HTML: &str = include_str!("../../../frontend/admin.html");
const DATENSCHUTZ_HTML: &str = include_str!("../../../frontend/legal/datenschutz.html");
const IMPRESSUM_HTML: &str = include_str!("../../../frontend/legal/impressum.html");
const AGB_HTML: &str = include_str!("../../../frontend/legal/agb.html");
const MANIFEST_JSON: &str = include_str!("../../../frontend/manifest.webmanifest");
const SERVICE_WORKER_JS: &str = include_str!("../../../frontend/sw.js");
const ERROR_404_HTML: &str = include_str!("../../../frontend/404.html");
const ERROR_500_HTML: &str = include_str!("../../../frontend/500.html");

fn frontend_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend")
}

fn load_text(rel: &str, embedded: &str) -> String {
    let production = std::env::var("RUST_ENV")
        .map(|v| v.eq_ignore_ascii_case("production"))
        .unwrap_or(false);
    if !production {
        let path = frontend_dir().join(rel);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            return contents;
        }
    }
    embedded.to_string()
}

pub async fn root() -> Redirect {
    Redirect::permanent("/demo")
}

pub async fn demo_page() -> impl IntoResponse {
    (NO_CACHE, Html(load_text("index.html", DEMO_HTML)))
}

pub async fn admin_page() -> impl IntoResponse {
    (NO_CACHE, Html(load_text("admin.html", ADMIN_HTML)))
}

pub async fn datenschutz_page() -> impl IntoResponse {
    (
        NO_CACHE,
        Html(load_text("legal/datenschutz.html", DATENSCHUTZ_HTML)),
    )
}

pub async fn impressum_page() -> impl IntoResponse {
    (
        NO_CACHE,
        Html(load_text("legal/impressum.html", IMPRESSUM_HTML)),
    )
}

pub async fn agb_page() -> impl IntoResponse {
    (
        NO_CACHE,
        Html(load_text("legal/agb.html", AGB_HTML)),
    )
}

pub async fn manifest() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        load_text("manifest.webmanifest", MANIFEST_JSON),
    )
}

pub async fn service_worker() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache, no-store, must-revalidate"),
        ],
        load_text("sw.js", SERVICE_WORKER_JS),
    )
}

pub async fn icon_180() -> impl IntoResponse {
    icon_png(include_bytes!("../../../frontend/icons/icon-180.png"))
}

pub async fn icon_192() -> impl IntoResponse {
    icon_png(include_bytes!("../../../frontend/icons/icon-192.png"))
}

pub async fn icon_512() -> impl IntoResponse {
    icon_png(include_bytes!("../../../frontend/icons/icon-512.png"))
}

fn icon_png(bytes: &'static [u8]) -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400")],
        bytes,
    )
}

pub async fn fallback_handler(uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/users/")
        || path.starts_with("/auth/")
        || path.starts_with("/admin/")
        || path.starts_with("/api/")
        || path.starts_with("/dev/")
        || path.starts_with("/announcements")
    {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::json!({ "error": "not found" }).to_string(),
        )
            .into_response();
    }
    (
        StatusCode::NOT_FOUND,
        NO_CACHE,
        Html(load_text("404.html", ERROR_404_HTML)),
    )
        .into_response()
}

pub fn internal_error_page() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        NO_CACHE,
        Html(load_text("500.html", ERROR_500_HTML)),
    )
        .into_response()
}
