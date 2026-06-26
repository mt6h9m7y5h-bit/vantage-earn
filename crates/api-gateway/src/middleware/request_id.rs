use axum::{
    body::Body,
    http::{HeaderValue, Request, Response},
    middleware::Next,
};
use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone, Debug)]
pub struct RequestId(pub String);

pub async fn request_id_middleware(mut req: Request<Body>, next: Next) -> Response<Body> {
    let request_id = Uuid::new_v4().to_string();
    req.extensions_mut().insert(RequestId(request_id.clone()));
    let span = tracing::info_span!("request", request_id = %request_id);
    let _guard = span.enter();
    let mut res = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        res.headers_mut().insert(REQUEST_ID_HEADER, val);
    }
    res
}
