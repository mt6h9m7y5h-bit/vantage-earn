use axum::{
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect},
};

pub async fn root() -> Redirect {
    Redirect::permanent("/demo")
}

pub async fn demo_page() -> Html<&'static str> {
    Html(include_str!("../../../frontend/index.html"))
}

pub async fn manifest() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        include_str!("../../../frontend/manifest.webmanifest"),
    )
}

pub async fn service_worker() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        include_str!("../../../frontend/sw.js"),
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
