use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use tower::ServiceExt;
use uuid::Uuid;

use api_gateway::routes;
use api_gateway::state::AppState;

fn app(state: AppState) -> Router {
    routes::router().with_state(state)
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn register(app: &Router) -> (Uuid, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/register")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    (
        Uuid::parse_str(json["user_id"].as_str().unwrap()).unwrap(),
        json["token"].as_str().unwrap().to_string(),
    )
}

fn authed(method: &str, uri: &str, token: &str, body: Option<&str>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));

    match body {
        Some(b) => builder
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

#[tokio::test]
async fn health_returns_ok() {
    let app = app(AppState::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["status"], "ok");
    assert_eq!(json["database"], true);
}

#[tokio::test]
async fn register_and_login_issue_tokens() {
    let app = app(AppState::new());
    let (user_id, token) = register(&app).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"user_id":"{user_id}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["user_id"], user_id.to_string());
    assert!(!json["token"].as_str().unwrap().is_empty());
    assert!(!token.is_empty());
}

#[tokio::test]
async fn wallet_requires_auth() {
    let app = app(AppState::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/users/me/wallet")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn payout_debits_wallet_and_blocks_double_spend() {
    let state = AppState::new();
    let app = app(state.clone());
    let (user_id, token) = register(&app).await;

    state
        .credit(user_id, Decimal::from_str_exact("0.01").unwrap())
        .await
        .unwrap();
    state
        .add_revenue(Decimal::from_str_exact("1.0").unwrap())
        .await
        .unwrap();

    let first = app
        .clone()
        .oneshot(authed(
            "POST",
            "/users/me/payout/request",
            &token,
            Some(r#"{"amount_usdt":"0.01"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
        .oneshot(authed(
            "POST",
            "/users/me/payout/request",
            &token,
            Some(r#"{"amount_usdt":"0.01"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::BAD_REQUEST);

    let balance = state.balance(user_id).await.unwrap();
    assert_eq!(balance, Decimal::ZERO);
}

#[tokio::test]
async fn watch_complete_credits_wallet() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    let response = app
        .clone()
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token,
            Some(r#"{"watch_duration_secs": 60}"#),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["reward_usdt"], "0.002");

    let wallet = app
        .oneshot(authed("GET", "/users/me/wallet", &token, None))
        .await
        .unwrap();
    assert_eq!(wallet.status(), StatusCode::OK);
    let wallet_json = body_json(wallet).await;
    assert_eq!(wallet_json["balance_usdt"], "0.002");
    assert!(wallet_json["trust_score"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn watch_too_short_is_rejected() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    let response = app
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token,
            Some(r#"{"watch_duration_secs": 5}"#),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
