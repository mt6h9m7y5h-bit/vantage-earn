use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use chrono::Utc;
use http_body_util::BodyExt;
use reward_engine::{BonusEngine, RewardEngine};
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

    let min = state.min_payout_usdt().await;
    state.credit(user_id, min).await.unwrap();
    state
        .add_revenue(Decimal::from_str_exact("1000.0").unwrap())
        .await
        .unwrap();

    let body = format!(
        r#"{{"amount_usdt":"{min}","payout_method":"crypto"}}"#
    );

    let first = app
        .clone()
        .oneshot(authed(
            "POST",
            "/users/me/payout/request",
            &token,
            Some(&body),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_json = body_json(first).await;
    assert_eq!(first_json["payout_method"], "crypto");

    let second = app
        .oneshot(authed(
            "POST",
            "/users/me/payout/request",
            &token,
            Some(&body),
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::BAD_REQUEST);

    let balance = state.balance(user_id).await.unwrap();
    assert_eq!(balance, Decimal::ZERO);
}

#[tokio::test]
async fn payout_rejects_invalid_method() {
    let state = AppState::new();
    let app = app(state.clone());
    let (user_id, token) = register(&app).await;

    let min = state.min_payout_usdt().await;
    state.credit(user_id, min).await.unwrap();
    state
        .add_revenue(Decimal::from_str_exact("1000.0").unwrap())
        .await
        .unwrap();

    let response = app
        .oneshot(authed(
            "POST",
            "/users/me/payout/request",
            &token,
            Some(r#"{"amount_usdt":"1","payout_method":"bitcoin"}"#),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn watch_complete_credits_wallet() {
    let app = app(AppState::new());
    let (user_id, token) = register(&app).await;

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

    let today = Utc::now().date_naive();
    let base = RewardEngine::calculate_watch_reward(60, 1);
    let (after_surprise, _) = BonusEngine::apply_surprise(base, user_id, today, 1);
    let expected_total = after_surprise + BonusEngine::daily_bonus_amount();
    assert_eq!(json["reward_usdt"], expected_total.to_string());
    assert!(json["bonuses"].as_array().unwrap().iter().any(|b| {
        b["id"] == "daily_login"
    }));

    let wallet = app
        .oneshot(authed("GET", "/users/me/wallet", &token, None))
        .await
        .unwrap();
    assert_eq!(wallet.status(), StatusCode::OK);
    let wallet_json = body_json(wallet).await;
    assert_eq!(wallet_json["balance_usdt"], json["reward_usdt"]);
    assert!(wallet_json["trust_score"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn stats_returns_streak_and_estimates() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    let response = app
        .oneshot(authed("GET", "/users/me/stats", &token, None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["streak_days"], 0);
    assert_eq!(json["streak_bonus_percent"], 0);
    assert_eq!(json["total_watches"], 0);
    assert_eq!(json["daily_bonus_claimed_today"], false);
    assert_eq!(json["next_milestone"], 10);
    assert!(json["bonus_catalog"].as_array().unwrap().len() >= 5);
    assert_eq!(json["watches_remaining_today"], 30);
    assert_eq!(json["reward_estimate_30s"], "0.001");
    assert_eq!(json["min_payout_eur"], "170");
    assert_eq!(json["payout_methods"].as_array().unwrap().len(), 3);
    let demo = json["payout_demo_mode"].as_bool().unwrap();
    if demo {
        assert_eq!(json["min_payout_usdt"], "0.01");
    } else {
        assert_eq!(json["min_payout_usdt"], "184.782609");
    }
}

#[tokio::test]
async fn watch_complete_updates_streak() {
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

    let stats = app
        .oneshot(authed("GET", "/users/me/stats", &token, None))
        .await
        .unwrap();
    let json = body_json(stats).await;
    assert_eq!(json["streak_days"], 1);
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

#[tokio::test]
async fn stats_reflect_bonus_progress_after_watch() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    app.clone()
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token,
            Some(r#"{"watch_duration_secs": 60}"#),
        ))
        .await
        .unwrap();

    let stats = app
        .oneshot(authed("GET", "/users/me/stats", &token, None))
        .await
        .unwrap();
    let json = body_json(stats).await;
    assert_eq!(json["total_watches"], 1);
    assert_eq!(json["daily_bonus_claimed_today"], true);
    assert_eq!(json["streak_bonus_percent"], 5);
    assert_eq!(json["next_milestone"], 10);
}
