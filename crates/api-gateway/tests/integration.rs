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
    assert!(json["bonus_catalog"].as_array().unwrap().len() >= 6);
    assert_eq!(json["watches_remaining_today"], 30);
    assert_eq!(json["reward_estimate_30s"], "0.001");
    assert_eq!(json["min_payout_eur"], "170");
    assert_eq!(json["payout_methods"].as_array().unwrap().len(), 3);
    let method_info = json["payout_method_info"].as_array().unwrap();
    assert_eq!(method_info.len(), 3);
    let paypal = method_info
        .iter()
        .find(|m| m["method"] == "paypal")
        .expect("paypal info");
    assert_eq!(paypal["estimated_days_min"], 3);
    assert_eq!(paypal["estimated_days_max"], 5);
    assert!(paypal["estimated_time_de"]
        .as_str()
        .unwrap()
        .contains("Werktage"));
    assert!(json["payout_first_time_note_de"]
        .as_str()
        .unwrap()
        .contains("erste Auszahlung"));
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

#[tokio::test]
async fn weekly_leaderboard_is_public_and_anonymized() {
    let state = AppState::new();
    let app = app(state);
    let (user_a, token_a) = register(&app).await;
    let (_user_b, token_b) = register(&app).await;

    for _ in 0..2 {
        app.clone()
            .oneshot(authed(
                "POST",
                "/users/me/watch/complete",
                &token_a,
                Some(r#"{"watch_duration_secs": 60}"#),
            ))
            .await
            .unwrap();
    }
    app.clone()
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token_b,
            Some(r#"{"watch_duration_secs": 60}"#),
        ))
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/leaderboard/weekly")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    let top = &entries[0];
    assert_eq!(top["rank"], 1);
    let name = top["display_name"].as_str().unwrap();
    assert!(name.starts_with("User #"));
    assert!(!name.contains(&user_a.to_string()));
    assert!(top["weekly_earnings_usdt"].as_str().unwrap().parse::<f64>().unwrap() > 0.0);
}

#[tokio::test]
async fn admin_stats_requires_secret() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());

    let no_header = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_header.status(), StatusCode::UNAUTHORIZED);

    let wrong = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/stats")
                .header("X-Admin-Secret", "wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let ok = app
        .oneshot(
            Request::builder()
                .uri("/admin/stats")
                .header("X-Admin-Secret", "test-admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let json = body_json(ok).await;
    assert!(json["total_revenue"].is_string());
    assert!(json["user_count"].as_i64().unwrap() >= 0);
    assert!(json["recent_payout_count"].as_i64().unwrap() >= 0);

    std::env::remove_var("ADMIN_SECRET");
}

#[tokio::test]
async fn daily_challenge_bonus_on_fifth_watch() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    for _ in 0..4 {
        app.clone()
            .oneshot(authed(
                "POST",
                "/users/me/watch/complete",
                &token,
                Some(r#"{"watch_duration_secs": 60}"#),
            ))
            .await
            .unwrap();
    }

    let stats = app
        .clone()
        .oneshot(authed("GET", "/users/me/stats", &token, None))
        .await
        .unwrap();
    let stats_json = body_json(stats).await;
    assert_eq!(stats_json["challenge_watches_today"], 4);
    assert_eq!(stats_json["daily_challenge_completed_today"], false);

    let fifth = app
        .clone()
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token,
            Some(r#"{"watch_duration_secs": 60}"#),
        ))
        .await
        .unwrap();
    assert_eq!(fifth.status(), StatusCode::OK);
    let fifth_json = body_json(fifth).await;
    assert!(fifth_json["bonuses"].as_array().unwrap().iter().any(|b| {
        b["id"] == "daily_challenge"
    }));

    let stats2 = app
        .oneshot(authed("GET", "/users/me/stats", &token, None))
        .await
        .unwrap();
    let stats2_json = body_json(stats2).await;
    assert_eq!(stats2_json["daily_challenge_completed_today"], true);
    assert!(stats2_json["bonus_catalog"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "daily_challenge" && c["status"] == "claimed"));
}
