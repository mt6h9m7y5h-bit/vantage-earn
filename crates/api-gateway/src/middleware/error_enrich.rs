use axum::{
    body::Body,
    http::Request,
    middleware::Next,
    response::Response,
};
use http_body_util::BodyExt;

use crate::middleware::RequestId;

pub async fn enrich_json_errors(req: Request<Body>, next: Next) -> Response<Body> {
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| "unknown".into());
    let res = next.run(req).await;
    let status = res.status();
    if !(status.is_client_error() || status.is_server_error()) {
        return res;
    }
    let is_json = res
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/json"));
    if !is_json {
        return res;
    }

    let (parts, body) = res.into_parts();
    let Ok(bytes) = body.collect().await else {
        return Response::from_parts(parts, Body::empty());
    };
    let bytes = bytes.to_bytes();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({
        "error": "unknown error"
    }));
    if let Some(obj) = value.as_object_mut() {
        obj.entry("request_id")
            .or_insert(serde_json::json!(request_id));
    }
    let body = Body::from(serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec()));
    Response::from_parts(parts, body)
}
