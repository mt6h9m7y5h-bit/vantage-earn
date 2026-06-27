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

fn json_decimal(v: &serde_json::Value) -> Decimal {
    if let Some(s) = v.as_str() {
        s.parse().unwrap_or_else(|_| panic!("invalid decimal string: {s}"))
    } else if let Some(n) = v.as_f64() {
        Decimal::from_f64_retain(n)
            .unwrap_or_else(|| panic!("invalid JSON number: {n}"))
            .round_dp(6)
    } else {
        panic!("expected JSON number or decimal string, got {v}")
    }
}

async fn register(app: &Router) -> (Uuid, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"accept_terms":true,"accept_age_minimum":true}"#))
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

async fn register_with_email(app: &Router, email: &str, password: &str) -> (Uuid, String) {
    let body = format!(
        r#"{{"accept_terms":true,"accept_age_minimum":true,"email":"{email}","password":"{password}"}}"#
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK, "email register failed");
    let json = body_json(response).await;
    assert_eq!(json["email"], email);
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
async fn root_redirects_to_demo_not_health_json() {
    let app = app(AppState::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("accept", "text/html,application/xhtml+xml")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.headers().get("location").unwrap().to_str().unwrap(),
        "/demo"
    );
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
    assert!(json["components"]["api"]["status"].as_str().unwrap() == "ok");
    assert!(json["components"]["payouts"]["pending_count"].is_number());
}

#[tokio::test]
async fn health_includes_request_id_header() {
    let app = Router::new()
        .merge(routes::router())
        .layer(axum::middleware::from_fn(api_gateway::middleware::request_id_middleware))
        .with_state(AppState::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.headers().get("x-request-id").is_some());
}

#[tokio::test]
async fn api_v1_health_mirrors_public_health() {
    let app = app(AppState::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["service"], "vantage-earn");
}

#[tokio::test]
async fn admin_health_requires_secret() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let ok = app
        .oneshot(admin_req("GET", "/admin/health", None))
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let json = body_json(ok).await;
    assert!(json["uptime_secs"].as_i64().unwrap() >= 0);
    assert!(json["components"].is_object());
}

#[tokio::test]
async fn announcements_crud_and_public_active() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());

    let create = app
        .clone()
        .oneshot(admin_req(
            "POST",
            "/admin/announcements",
            Some(r#"{"type":"banner","title":"Test","body":"Hallo","priority":1,"active":true}"#),
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let created = body_json(create).await;
    let id = created["id"].as_str().unwrap();

    let active = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/announcements/active")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(active.status(), StatusCode::OK);
    let active_json = body_json(active).await;
    assert!(active_json.as_array().unwrap().iter().any(|a| a["title"] == "Test"));

    let patch = app
        .clone()
        .oneshot(admin_req(
            "PATCH",
            &format!("/admin/announcements/{id}"),
            Some(r#"{"active":false}"#),
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
}

#[tokio::test]
async fn fraud_admin_endpoints() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());
    let (_user_id, _token) = register(&app).await;

    let summary = app
        .clone()
        .oneshot(admin_req("GET", "/admin/fraud/summary", None))
        .await
        .unwrap();
    assert_eq!(summary.status(), StatusCode::OK);
    let summary_json = body_json(summary).await;
    assert_eq!(summary_json["repeated_ip_tracking"], "nicht verfügbar");

    let users = app
        .oneshot(admin_req("GET", "/admin/fraud/high-risk-users", None))
        .await
        .unwrap();
    assert_eq!(users.status(), StatusCode::OK);
}

#[tokio::test]
async fn dev_endpoints_respect_environment() {
    let prior = std::env::var("RUST_ENV").ok();
    std::env::set_var("JWT_SECRET", "test-jwt-secret-for-production-guard");

    std::env::set_var("RUST_ENV", "production");
    let prod_app = app(AppState::new());
    let prod = prod_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dev/seed-demo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(prod.status(), StatusCode::NOT_FOUND);

    std::env::set_var("RUST_ENV", "development");
    let dev_app = app(AppState::new());
    let dev = dev_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/dev/seed-demo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dev.status(), StatusCode::OK);

    match prior {
        Some(v) => std::env::set_var("RUST_ENV", v),
        None => std::env::remove_var("RUST_ENV"),
    }
}

#[tokio::test]
async fn public_config_returns_mock_by_default() {
    let app = app(AppState::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["ad_provider"], "mock");
    assert!(json["applixir_app_id"].is_null());
    assert!(json["adinplay_tag_url"].is_null());
    assert_eq!(json["watch_duration_secs"], 15);
}

#[tokio::test]
async fn register_requires_accept_terms() {
    async fn register_status(body: &str) -> StatusCode {
        let app = app(AppState::new());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/register")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        response.status()
    }

    assert_eq!(register_status("{}").await, StatusCode::BAD_REQUEST);
    assert_eq!(
        register_status(r#"{"accept_terms":false}"#).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        register_status(r#"{"accept_terms":true,"accept_age_minimum":false}"#).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        register_status(r#"{"accept_terms":true}"#).await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn register_and_login_issue_tokens() {
    let app = app(AppState::new());
    let email = format!("user-{}@example.com", Uuid::new_v4());
    let (user_id, _token) = register_with_email(&app, &email, "securepass1").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"email":"{email}","password":"securepass1"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["user_id"], user_id.to_string());
    assert_eq!(json["email"], email);
    assert!(!json["token"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn register_links_anonymous_account_when_jwt_present() {
    let app = app(AppState::new());
    let (anon_id, anon_token) = register(&app).await;
    let email = format!("link-{}@example.com", Uuid::new_v4());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/register")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {anon_token}"))
                .body(Body::from(format!(
                    r#"{{"accept_terms":true,"accept_age_minimum":true,"email":"{email}","password":"linkpass12"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["user_id"], anon_id.to_string());
    assert_eq!(json["email"], email);

    let wallet = app
        .oneshot(authed("GET", "/users/me/wallet", json["token"].as_str().unwrap(), None))
        .await
        .unwrap();
    assert_eq!(wallet.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_rejects_invalid_credentials() {
    let app = app(AppState::new());
    let email = format!("bad-{}@example.com", Uuid::new_v4());
    register_with_email(&app, &email, "correctpass").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"email":"{email}","password":"wrongpass"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
    assert_eq!(json["stats"]["watches_today"], 1);
    assert_eq!(json["stats"]["total_watches"], 1);
    assert_eq!(json["stats"]["streak_days"], 1);
    assert_eq!(
        json_decimal(&json["wallet"]["balance_usdt"]),
        expected_total
    );
    assert!(json["wallet"]["balance_usdt"].is_number());
    assert!(json["wallet"]["localized_balance"].is_number());
    assert!(json["bonuses"].as_array().unwrap().iter().any(|b| {
        b["id"] == "daily_login"
    }));

    let wallet = app
        .clone()
        .oneshot(authed("GET", "/users/me/wallet", &token, None))
        .await
        .unwrap();
    assert_eq!(wallet.status(), StatusCode::OK);
    let wallet_json = body_json(wallet).await;
    assert_eq!(json_decimal(&wallet_json["balance_usdt"]), expected_total);
    assert!(wallet_json["balance_usdt"].is_number());
    assert!(wallet_json["localized_balance"].is_number());
    assert!(wallet_json["trust_score"].as_i64().unwrap() > 0);

    // Simulate full page reload: fresh GET wallet + stats must still reflect credited balance.
    let stats = app
        .clone()
        .oneshot(authed("GET", "/users/me/stats", &token, None))
        .await
        .unwrap();
    assert_eq!(stats.status(), StatusCode::OK);
    let stats_json = body_json(stats).await;
    assert_eq!(stats_json["watches_today"], 1);
    assert_eq!(stats_json["total_watches"], 1);

    let wallet_reload = app
        .clone()
        .oneshot(authed("GET", "/users/me/wallet", &token, None))
        .await
        .unwrap();
    assert_eq!(wallet_reload.status(), StatusCode::OK);
    let reload_json = body_json(wallet_reload).await;
    assert_eq!(json_decimal(&reload_json["balance_usdt"]), expected_total);
    assert!(
        reload_json["balance_usdt"].as_f64().unwrap_or(0.0) > 0.0,
        "wallet balance must persist after reload-style GET"
    );
}

#[tokio::test]
async fn wallet_and_auth_responses_are_not_cacheable() {
    let app = Router::new()
        .merge(routes::router())
        .layer(axum::middleware::from_fn(
            api_gateway::middleware::security_headers_middleware,
        ))
        .with_state(AppState::new());
    let (_user_id, token) = register(&app).await;

    let wallet = app
        .oneshot(authed("GET", "/users/me/wallet", &token, None))
        .await
        .unwrap();
    assert_eq!(wallet.status(), StatusCode::OK);
    let cache_control = wallet
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        cache_control.contains("no-store"),
        "wallet must not be cached by intermediaries: {cache_control}"
    );
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
async fn video_offers_returns_tiered_catalog() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    let response = app
        .clone()
        .oneshot(authed("GET", "/users/me/video-offers", &token, None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let offers = json["offers"].as_array().unwrap();
    assert_eq!(offers.len(), 4);

    assert_eq!(offers[0]["tier"], "quick");
    assert_eq!(offers[0]["duration_secs"], 30);
    assert_eq!(offers[0]["label_de"], "Schnell");
    assert_eq!(offers[0]["reward_usdt"], "0.001");
    assert!(offers[0]["reward_eur_display"]
        .as_str()
        .unwrap()
        .starts_with('≈'));
    assert!(offers[0]["reward_eur_display"]
        .as_str()
        .unwrap()
        .contains(','));

    assert_eq!(offers[1]["duration_secs"], 60);
    assert_eq!(offers[2]["duration_secs"], 90);
    assert_eq!(offers[3]["tier"], "mega");
    assert_eq!(offers[3]["duration_secs"], 120);
    assert_eq!(offers[3]["bonus_multiplier"], 2);

    let expected_60 = RewardEngine::calculate_watch_reward(60, 0);
    assert_eq!(json_decimal(&offers[1]["reward_usdt"]), expected_60);

    let stats = app
        .oneshot(authed("GET", "/users/me/stats", &token, None))
        .await
        .unwrap();
    let stats_json = body_json(stats).await;
    assert_eq!(stats_json["video_offers"].as_array().unwrap().len(), 4);
    assert_eq!(stats_json["top_offers"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn top_offers_returns_mock_catalog() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    let response = app
        .clone()
        .oneshot(authed("GET", "/users/me/top-offers", &token, None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let offers = json["offers"].as_array().unwrap();
    assert_eq!(offers.len(), 5);

    assert_eq!(offers[0]["label_de"], "Umfrage");
    assert_eq!(offers[0]["reward_eur_cents"], 30);
    assert!(offers[0]["reward_eur_display"]
        .as_str()
        .unwrap()
        .starts_with('≈'));
    assert_eq!(offers[0]["status"], "mock");

    let cents: Vec<u32> = offers
        .iter()
        .map(|o| o["reward_eur_cents"].as_u64().unwrap() as u32)
        .collect();
    assert!(cents.iter().all(|c| (30..=80).contains(c)));
}

#[tokio::test]
async fn watch_complete_honors_chosen_duration() {
    let app = app(AppState::new());
    let (user_id, token) = register(&app).await;

    let response = app
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token,
            Some(r#"{"watch_duration_secs": 120}"#),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let today = Utc::now().date_naive();
    let base = RewardEngine::calculate_watch_reward(120, 1);
    let (after_surprise, _) = BonusEngine::apply_surprise(base, user_id, today, 1);
    assert_eq!(json_decimal(&json["base_reward_usdt"]), after_surprise);
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
        .clone()
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
}

#[tokio::test]
async fn admin_api_paths_are_not_rate_limited() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    std::env::set_var("AUTH_RATE_LIMIT_MAX", "3");
    let app = app(AppState::new());

    for _ in 0..25 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "admin/stats must bypass auth rate limit"
        );
    }

    for _ in 0..25 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/live")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "admin/live must bypass auth rate limit"
        );
    }
}

fn admin_req(method: &str, uri: &str, body: Option<&str>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("X-Admin-Secret", "test-admin-secret");
    match body {
        Some(b) => builder
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

#[tokio::test]
async fn admin_credit_and_audit_log() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());
    let (user_id, _token) = register(&app).await;

    let credit = app
        .clone()
        .oneshot(admin_req(
            "POST",
            &format!("/admin/users/{user_id}/credit"),
            Some(r#"{"amount_usdt":"1.5","reason":"Test-Bonus"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(credit.status(), StatusCode::OK);
    let credit_json = body_json(credit).await;
    assert_eq!(credit_json["balance_usdt"], "1.5");
    assert_eq!(credit_json["action"], "credit");

    let audit = app
        .oneshot(admin_req("GET", "/admin/audit-log?limit=10", None))
        .await
        .unwrap();
    assert_eq!(audit.status(), StatusCode::OK);
    let audit_json = body_json(audit).await;
    let entries = audit_json["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    assert_eq!(entries[0]["action"], "credit");
    assert_eq!(entries[0]["user_id"], user_id.to_string());
    assert_eq!(entries[0]["details"]["reason"], "Test-Bonus");
}

#[tokio::test]
async fn admin_ban_blocks_watch() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());
    let (user_id, token) = register(&app).await;

    let ban = app
        .clone()
        .oneshot(admin_req(
            "POST",
            &format!("/admin/users/{user_id}/ban"),
            Some(r#"{"banned":true,"reason":"Test-Sperre"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(ban.status(), StatusCode::OK);
    let ban_json = body_json(ban).await;
    assert_eq!(ban_json["banned"], true);

    let watch = app
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token,
            Some(r#"{"watch_duration_secs": 60}"#),
        ))
        .await
        .unwrap();
    assert_eq!(watch.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_stats_extended_fields() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
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

    let response = app
        .oneshot(admin_req("GET", "/admin/stats", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["active_users_today"].as_i64().unwrap() >= 1);
    assert!(json["videos_today"].as_i64().unwrap() >= 1);
    assert!(json["rewards_today_usdt"].is_string());
    assert!(json["avg_trust_score"].is_number());
    assert!(json["revenue_24h"].is_string());
}

/// `DATABASE_URL=postgres://... cargo test -p api-gateway --test integration postgres_health_and_register -- --ignored --nocapture`
#[tokio::test]
#[ignore = "requires DATABASE_URL and running postgres (see scripts/test-postgres.sh)"]
async fn postgres_health_and_register() {
    let url = match std::env::var("DATABASE_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("skip: set DATABASE_URL to run this test");
            return;
        }
    };
    std::env::set_var("DATABASE_URL", &url);

    let state = AppState::connect().await;
    assert!(state.store_healthy().await, "postgres ping failed");

    let app = app(state);
    let response = app
        .clone()
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

    let (_user_id, _token) = register(&app).await;
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

#[tokio::test]
async fn analytics_summary_reflects_watch_earnings() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    let watch = app
        .clone()
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token,
            Some(r#"{"watch_duration_secs":30}"#),
        ))
        .await
        .unwrap();
    assert_eq!(watch.status(), StatusCode::OK);

    let response = app
        .oneshot(authed("GET", "/users/me/analytics/summary", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;

    assert_eq!(json["total_watches"], 1);
    assert_eq!(json["watches_today"], 1);
    assert_eq!(json["streak_days"], 1);
    assert!(json["earnings_today"].as_str().unwrap().parse::<f64>().unwrap() > 0.0);
    assert!(json["earnings_last_7_days_total"]
        .as_str()
        .unwrap()
        .parse::<f64>()
        .unwrap()
        > 0.0);
    assert_eq!(json["earnings_last_7_days"].as_array().unwrap().len(), 7);
    assert_eq!(json["daily_earnings_30d"].as_array().unwrap().len(), 30);
    let last_day = &json["earnings_last_7_days"].as_array().unwrap()[6];
    assert!(last_day["watch_count"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn admin_analytics_summary_returns_chart_data() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
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

    let response = app
        .oneshot(admin_req("GET", "/admin/analytics/summary?days=7", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["days"], 7);
    assert_eq!(json["daily_earnings"].as_array().unwrap().len(), 7);
    assert!(json["total_users"].as_i64().unwrap() >= 1);
    assert!(json["earnings_period_total"].is_string());
    assert!(json["pending_payout_count"].as_i64().unwrap() >= 0);
}

#[tokio::test]
async fn admin_payout_approve_and_reject_with_audit() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    std::env::set_var("PAYOUT_DEMO_MODE", "true");
    let state = AppState::new();
    let app = app(state.clone());
    let (user_id, token) = register(&app).await;

    let min = state.min_payout_usdt().await;
    state.credit(user_id, min).await.unwrap();
    state
        .add_revenue(Decimal::from_str_exact("1000.0").unwrap())
        .await
        .unwrap();
    state.set_trust_score(user_id, 20).await.unwrap();

    let body = format!(r#"{{"amount_usdt":"{min}","payout_method":"crypto"}}"#);
    let payout_res = app
        .clone()
        .oneshot(authed(
            "POST",
            "/users/me/payout/request",
            &token,
            Some(&body),
        ))
        .await
        .unwrap();
    assert_eq!(payout_res.status(), StatusCode::OK);
    let payout_json = body_json(payout_res).await;
    let payout_id = payout_json["payout_id"].as_str().unwrap();
    assert!(
        payout_json["status"] == "pending_validation"
            || payout_json["status"] == "pending_fraud_review"
    );

    let list = app
        .clone()
        .oneshot(admin_req("GET", "/admin/payouts?status=pending", None))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_json = body_json(list).await;
    assert!(list_json["payouts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"].as_str().unwrap() == payout_id));

    let approve = app
        .clone()
        .oneshot(admin_req(
            "POST",
            &format!("/admin/payouts/{payout_id}/approve"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);
    let approve_json = body_json(approve).await;
    assert_eq!(approve_json["action"], "approve");
    assert_eq!(approve_json["payout"]["status"], "approved");

    let audit = app
        .clone()
        .oneshot(admin_req("GET", "/admin/audit-log?limit=10", None))
        .await
        .unwrap();
    let audit_json = body_json(audit).await;
    assert!(audit_json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "payout_approve"));

    // Second payout to test reject + refund
    state.credit(user_id, min).await.unwrap();
    let payout_res2 = app
        .clone()
        .oneshot(authed(
            "POST",
            "/users/me/payout/request",
            &token,
            Some(&body),
        ))
        .await
        .unwrap();
    assert_eq!(payout_res2.status(), StatusCode::OK);
    let payout_id2 = body_json(payout_res2).await["payout_id"]
        .as_str()
        .unwrap()
        .to_string();

    let reject = app
        .clone()
        .oneshot(admin_req(
            "POST",
            &format!("/admin/payouts/{payout_id2}/reject"),
            Some(r#"{"reason":"Test-Ablehnung"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(reject.status(), StatusCode::OK);
    let reject_json = body_json(reject).await;
    assert_eq!(reject_json["action"], "reject");
    assert_eq!(reject_json["payout"]["status"], "rejected");

    let balance = state.balance(user_id).await.unwrap();
    assert_eq!(balance, min);

    let audit2 = app
        .oneshot(admin_req("GET", "/admin/audit-log?limit=10", None))
        .await
        .unwrap();
    let audit2_json = body_json(audit2).await;
    assert!(audit2_json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| {
            e["action"] == "payout_reject"
                && e["details"]["reason"] == "Test-Ablehnung"
        }));
}

#[tokio::test]
async fn admin_payout_list_requires_secret() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());

    let no_secret = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/payouts?status=pending")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_secret.status(), StatusCode::UNAUTHORIZED);

    let ok = app
        .oneshot(admin_req("GET", "/admin/payouts?status=all", None))
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_feature_flags_get_and_patch_with_audit() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());

    let get = app
        .clone()
        .oneshot(admin_req("GET", "/admin/feature-flags", None))
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let flags = body_json(get).await;
    assert_eq!(flags["maintenance_mode"], false);
    assert!(flags["maintenance_message"].is_string());

    let patch = app
        .clone()
        .oneshot(admin_req(
            "PATCH",
            "/admin/feature-flags",
            Some(
                r#"{"maintenance_mode":true,"maintenance_message":"Test-Wartung","watch_duration_secs":45}"#,
            ),
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
    let patched = body_json(patch).await;
    assert_eq!(patched["maintenance_mode"], true);
    assert_eq!(patched["maintenance_message"], "Test-Wartung");
    assert_eq!(patched["watch_duration_secs"], 45);
    assert_eq!(patched["watch_duration_source"], "db");

    let config = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(config.status(), StatusCode::OK);
    let config_json = body_json(config).await;
    assert_eq!(config_json["maintenance_mode"], true);
    assert_eq!(config_json["watch_duration_secs"], 45);

    let audit = app
        .oneshot(admin_req("GET", "/admin/audit-log?limit=10", None))
        .await
        .unwrap();
    let audit_json = body_json(audit).await;
    assert!(audit_json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "feature_flags_update"));
}

#[tokio::test]
async fn maintenance_mode_blocks_watch_complete() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    app.clone()
        .oneshot(admin_req(
            "PATCH",
            "/admin/feature-flags",
            Some(r#"{"maintenance_mode":true,"maintenance_message":"Geplante Wartung"}"#),
        ))
        .await
        .unwrap();

    let watch = app
        .oneshot(authed(
            "POST",
            "/users/me/watch/complete",
            &token,
            Some(r#"{"watch_duration_secs": 60}"#),
        ))
        .await
        .unwrap();
    assert_eq!(watch.status(), StatusCode::BAD_REQUEST);
    let watch_json = body_json(watch).await;
    assert!(watch_json["error"]
        .as_str()
        .unwrap()
        .contains("Geplante Wartung"));
}

#[tokio::test]
async fn admin_bulk_credit_preview_and_execute() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());
    let (user1, token1) = register(&app).await;
    let (user2, token2) = register(&app).await;

    for token in [&token1, &token2] {
        app.clone()
            .oneshot(authed(
                "POST",
                "/users/me/watch/complete",
                token,
                Some(r#"{"watch_duration_secs": 60}"#),
            ))
            .await
            .unwrap();
    }

    let preview = app
        .clone()
        .oneshot(admin_req(
            "POST",
            "/admin/bulk/credit/preview",
            Some(r#"{"amount_usdt":"0.01","reason":"x","filter":"active_7d"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    let preview_json = body_json(preview).await;
    assert!(preview_json["user_count"].as_u64().unwrap() >= 2);
    assert_eq!(preview_json["max_users"], 500);

    let credit = app
        .clone()
        .oneshot(admin_req(
            "POST",
            "/admin/bulk/credit",
            Some(
                r#"{"amount_usdt":"0.05","reason":"Wochenbonus","filter":{"user_ids":[]}}"#,
            ),
        ))
        .await
        .unwrap();
    assert_eq!(credit.status(), StatusCode::BAD_REQUEST);

    let credit = app
        .clone()
        .oneshot(admin_req(
            "POST",
            "/admin/bulk/credit",
            Some(&format!(
                r#"{{"amount_usdt":"0.05","reason":"Wochenbonus","filter":{{"user_ids":["{user1}","{user2}"]}}}}"#
            )),
        ))
        .await
        .unwrap();
    assert_eq!(credit.status(), StatusCode::OK);
    let credit_json = body_json(credit).await;
    assert_eq!(credit_json["user_count"], 2);
    assert_eq!(credit_json["action"], "bulk_credit");

    let audit = app
        .oneshot(admin_req("GET", "/admin/audit-log?limit=10", None))
        .await
        .unwrap();
    let audit_json = body_json(audit).await;
    assert!(audit_json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| {
            e["action"] == "bulk_credit"
                && e["details"]["reason"] == "Wochenbonus"
                && e["details"]["user_count"] == 2
        }));
}

#[tokio::test]
async fn admin_search_live_notes_timeline_export() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());
    let (user_id, _) = register(&app).await;

    let search = app
        .clone()
        .oneshot(admin_req(
            "GET",
            &format!("/admin/search?q={user_id}"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::OK);
    let search_json = body_json(search).await;
    assert!(!search_json["users"].as_array().unwrap().is_empty());

    let live = app
        .clone()
        .oneshot(admin_req("GET", "/admin/live", None))
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::OK);
    let live_json = body_json(live).await;
    assert!(live_json["pending_payouts"].is_number());

    let detail = app
        .clone()
        .oneshot(admin_req(
            "GET",
            &format!("/admin/users/{user_id}"),
            None,
        ))
        .await
        .unwrap();
    let detail_json = body_json(detail).await;
    assert!(detail_json["risk_level"].is_string());
    assert!(detail_json["total_earnings_usdt"].is_string());

    let note = app
        .clone()
        .oneshot(admin_req(
            "POST",
            &format!("/admin/users/{user_id}/notes"),
            Some(r#"{"note":"Test-Notiz","created_by":"qa"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(note.status(), StatusCode::OK);

    let notes = app
        .clone()
        .oneshot(admin_req(
            "GET",
            &format!("/admin/users/{user_id}/notes"),
            None,
        ))
        .await
        .unwrap();
    let notes_json = body_json(notes).await;
    assert_eq!(notes_json["notes"].as_array().unwrap().len(), 1);

    let timeline = app
        .clone()
        .oneshot(admin_req(
            "GET",
            &format!("/admin/users/{user_id}/timeline"),
            None,
        ))
        .await
        .unwrap();
    let timeline_json = body_json(timeline).await;
    assert!(!timeline_json["events"].as_array().unwrap().is_empty());

    let users_list = app
        .clone()
        .oneshot(admin_req("GET", "/admin/users?limit=10", None))
        .await
        .unwrap();
    assert_eq!(users_list.status(), StatusCode::OK);

    let export = app
        .clone()
        .oneshot(admin_req("GET", "/admin/export/users?format=json&limit=5", None))
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);

    let flags = app
        .oneshot(admin_req("GET", "/admin/feature-flags", None))
        .await
        .unwrap();
    let flags_json = body_json(flags).await;
    assert!(flags_json["flags"].as_array().unwrap().len() >= 4);
}

#[tokio::test]
async fn admin_stats_includes_premium_kpis() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
    let app = app(AppState::new());
    let response = app
        .oneshot(admin_req("GET", "/admin/stats", None))
        .await
        .unwrap();
    let json = body_json(response).await;
    assert!(json["pending_withdrawal_count"].is_number());
    assert!(json["approved_payouts_today"].is_number());
    assert!(json["pending_sparkline"].is_array());
}

#[tokio::test]
async fn profile_stats_after_watch() {
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

    let response = app
        .oneshot(authed("GET", "/users/me/profile-stats", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["ads_watched"], 1);
    assert_eq!(json["level"], 1);
    assert!(json["total_xp"].as_i64().unwrap() >= 5);
    assert!(json["achievements"].as_array().unwrap().len() >= 9);
    let unlocked: Vec<_> = json["achievements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| !a["unlocked_at"].is_null())
        .collect();
    assert!(!unlocked.is_empty());
}

#[tokio::test]
async fn missions_progress_and_claim() {
    let app = app(AppState::new());
    let (_user_id, token) = register(&app).await;

    let login_missions = app
        .clone()
        .oneshot(authed("GET", "/users/me/missions", &token, None))
        .await
        .unwrap();
    let missions_json = body_json(login_missions).await;
    let daily_login = missions_json["daily"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["slug"] == "daily_login")
        .expect("daily_login mission");
    assert_eq!(daily_login["progress"], 1);
    assert_eq!(daily_login["completed"], true);

    for _ in 0..5 {
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

    let missions = app
        .clone()
        .oneshot(authed("GET", "/users/me/missions", &token, None))
        .await
        .unwrap();
    let missions_json = body_json(missions).await;
    let watch5 = missions_json["daily"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["slug"] == "daily_watch_5")
        .unwrap();
    assert_eq!(watch5["progress"], 5);
    assert_eq!(watch5["completed"], true);

    let claim = app
        .clone()
        .oneshot(authed(
            "POST",
            &format!("/users/me/missions/{}/claim", watch5["id"].as_i64().unwrap()),
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(claim.status(), StatusCode::OK);
    let claim_json = body_json(claim).await;
    assert_eq!(claim_json["credited_usdt"], "0.001");
}

#[tokio::test]
async fn achievements_list_and_notifications() {
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

    let achievements = app
        .clone()
        .oneshot(authed("GET", "/users/me/achievements", &token, None))
        .await
        .unwrap();
    assert_eq!(achievements.status(), StatusCode::OK);
    let ach_json = body_json(achievements).await;
    assert!(ach_json
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["slug"] == "first_ad" && !a["unlocked_at"].is_null()));

    let notes = app
        .clone()
        .oneshot(authed("GET", "/users/me/notifications", &token, None))
        .await
        .unwrap();
    assert_eq!(notes.status(), StatusCode::OK);
    let notes_json = body_json(notes).await;
    assert!(notes_json["unread_count"].as_i64().unwrap() >= 1);

    let mark_all = app
        .oneshot(authed(
            "PATCH",
            "/users/me/notifications/read-all",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(mark_all.status(), StatusCode::OK);
}

#[tokio::test]
async fn bitlabs_webhook_accepts_get_and_post() {
    std::env::set_var("BITLABS_SECRET_KEY", "test-bitlabs-secret");
    std::env::set_var("BITLABS_APP_TOKEN", "tok");
    std::env::remove_var("OFFERWALL_ENABLED");

    let app = app(AppState::new());
    let (user_id, _token) = register(&app).await;
    let secret = "test-bitlabs-secret";

    for (method, tx) in [("GET", "bitlabs-tx-get"), ("POST", "bitlabs-tx-post")] {
        let base = format!("https://localhost/webhooks/bitlabs?uid={user_id}&val=100&tx={tx}");
        let signed = api_gateway::bitlabs::sign_callback_url(&base, secret);
        let path_query = signed.strip_prefix("https://localhost").unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path_query)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "bitlabs webhook {method} should return 200"
        );
    }
}

#[tokio::test]
async fn bitlabs_webhook_accepts_debug_param_and_proxy_headers() {
    std::env::set_var("BITLABS_SECRET_KEY", "test-bitlabs-secret");
    std::env::set_var("BITLABS_APP_TOKEN", "tok");
    std::env::remove_var("OFFERWALL_ENABLED");

    let app = app(AppState::new());
    let (user_id, _token) = register(&app).await;
    let secret = "test-bitlabs-secret";
    let tx = "bitlabs-tx-debug";

    let base = format!(
        "https://vantage-earn.onrender.com/webhooks/bitlabs?uid={user_id}&val=100&tx={tx}&debug=true"
    );
    let signed = api_gateway::bitlabs::sign_callback_url(&base, secret);
    let path_query = signed
        .strip_prefix("https://vantage-earn.onrender.com")
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path_query)
                .header("host", "internal:10000")
                .header("x-forwarded-host", "vantage-earn.onrender.com")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_insights_endpoint() {
    std::env::set_var("ADMIN_SECRET", "test-admin-secret");
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

    let response = app
        .oneshot(admin_req("GET", "/admin/insights", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["revenue_7d"].is_string());
    assert!(json["avg_reward_usdt"].is_string());
    assert!(json["active_users_7d"].as_i64().unwrap() >= 1);
}
