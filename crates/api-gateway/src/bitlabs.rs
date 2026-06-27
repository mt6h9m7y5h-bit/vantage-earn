//! BitLabs offerwall S2S callbacks (Reward & Reconciliation).

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::OriginalUri;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use serde::Deserialize;
use sha1::Sha1;
use shared::Currency;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::AppState;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone)]
pub struct BitlabsConfig {
    pub app_token: Option<String>,
    pub secret_key: Option<String>,
    pub s2s_key: Option<String>,
    pub enabled: bool,
}

impl BitlabsConfig {
    pub fn from_env() -> Self {
        let app_token = non_empty_env("BITLABS_APP_TOKEN");
        let secret_key = non_empty_env("BITLABS_SECRET_KEY");
        let s2s_key = non_empty_env("BITLABS_S2S_KEY");
        let enabled = std::env::var("OFFERWALL_ENABLED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
            && app_token.is_some()
            && secret_key.is_some();

        Self {
            app_token,
            secret_key,
            s2s_key,
            enabled,
        }
    }

    pub fn public_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "provider": if self.enabled { Some("bitlabs") } else { None },
            "bitlabs_app_token": self.app_token.clone(),
            "iframe_base_url": "https://web.bitlabs.ai/",
        })
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn callback_hash_hex(url_without_hash: &str, secret_key: &str) -> Option<String> {
    let Ok(mut mac) = HmacSha1::new_from_slice(secret_key.as_bytes()) else {
        return None;
    };
    mac.update(url_without_hash.as_bytes());
    Some(
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
}

/// Append `&hash=` for BitLabs S2S callbacks (URL must not already include hash).
pub fn sign_callback_url(url_without_hash: &str, secret_key: &str) -> String {
    let hash = callback_hash_hex(url_without_hash, secret_key).unwrap_or_default();
    format!("{url_without_hash}&hash={hash}")
}

/// Verify BitLabs callback hash (HEX SHA1-HMAC of URL without `&hash=` suffix).
pub fn verify_callback_hash(callback_url: &str, secret_key: &str) -> bool {
    let Some(received) = extract_hash_from_url(callback_url) else {
        return false;
    };
    verify_callback_hash_explicit(callback_url, received, secret_key)
}

/// Verify when hash value is known (e.g. from `query.hash` or URL parsing).
pub fn verify_callback_hash_explicit(
    callback_url: &str,
    received_hash: &str,
    secret_key: &str,
) -> bool {
    let received_hash = received_hash.split('&').next().unwrap_or(received_hash);
    if received_hash.is_empty() {
        return false;
    }
    let Some(url_without_hash) = url_without_hash_param(callback_url, received_hash) else {
        return false;
    };
    let Some(expected) = callback_hash_hex(&url_without_hash, secret_key) else {
        return false;
    };
    constant_time_eq(received_hash.as_bytes(), expected.as_bytes())
}

fn extract_hash_from_url(callback_url: &str) -> Option<&str> {
    if let Some((_, pos)) = callback_url.rsplit_once("&hash=") {
        let received = pos.split('&').next().filter(|h| !h.is_empty())?;
        let start = callback_url.len() - pos.len();
        return Some(&callback_url[start..start + received.len()]);
    }
    if let Some((prefix, pos)) = callback_url.rsplit_once("?hash=") {
        let received = pos.split('&').next().filter(|h| !h.is_empty())?;
        if prefix.contains('?') {
            return None;
        }
        let start = callback_url.len() - pos.len();
        return Some(&callback_url[start..start + received.len()]);
    }
    None
}

/// Strip trailing `&hash=` / `?hash=` from raw path+query (preserve order/encoding).
fn strip_hash_from_path_and_query(path_and_query: &str) -> Option<(String, String)> {
    if let Some(pos) = path_and_query.rfind("&hash=") {
        let hash_start = pos + "&hash=".len();
        let received = path_and_query[hash_start..]
            .split('&')
            .next()
            .filter(|h| !h.is_empty())?;
        return Some((path_and_query[..pos].to_string(), received.to_string()));
    }
    if let Some(pos) = path_and_query.rfind("?hash=") {
        let hash_start = pos + "?hash=".len();
        let received = path_and_query[hash_start..]
            .split('&')
            .next()
            .filter(|h| !h.is_empty())?;
        let without = if pos == 0 {
            String::new()
        } else {
            path_and_query[..pos].to_string()
        };
        return Some((without, received.to_string()));
    }
    None
}

/// Strip trailing `&hash=` / `?hash=` per BitLabs PHP reference (hash appended last).
fn url_without_hash_param(callback_url: &str, _hash_value: &str) -> Option<String> {
    let path_and_query = callback_url
        .find("://")
        .map(|i| &callback_url[i + 3..])
        .and_then(|rest| rest.find('/').map(|j| &rest[j..]))
        .unwrap_or(callback_url);
    let (without, _) = strip_hash_from_path_and_query(path_and_query)?;
    let base = callback_url.strip_suffix(path_and_query)?;
    Some(format!("{base}{without}"))
}

fn normalize_host(host: &str) -> String {
    let host = host.split(',').next().unwrap_or(host).trim();
    host.strip_suffix(":443")
        .or_else(|| host.strip_suffix(":80"))
        .unwrap_or(host)
        .to_string()
}

fn callback_base_url_candidates(headers: &HeaderMap) -> Vec<String> {
    let mut bases = Vec::new();

    if let Some(configured) = non_empty_env("BITLABS_CALLBACK_BASE_URL") {
        bases.push(configured.trim_end_matches('/').to_string());
    }

    let hosts: Vec<String> = ["x-forwarded-host", "host"]
        .iter()
        .filter_map(|name| headers.get(*name).and_then(|v| v.to_str().ok()))
        .map(normalize_host)
        .collect();

    let schemes: Vec<&str> = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|p| vec![p])
        .unwrap_or_else(|| vec!["https", "http"]);

    for host in &hosts {
        for scheme in &schemes {
            let base = format!("{scheme}://{host}");
            if !bases.iter().any(|b| b == &base) {
                bases.push(base);
            }
        }
    }

    if bases.is_empty() {
        bases.push("https://localhost".to_string());
    }

    bases
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

struct HashVerifyAttempt {
    ok: bool,
    received_hash: Option<String>,
    candidate_bases: Vec<String>,
    computed: Vec<(String, String)>,
}

fn verify_callback_hash_for_request(
    headers: &HeaderMap,
    uri_path_and_query: &str,
    secret_key: &str,
) -> HashVerifyAttempt {
    let candidate_bases = callback_base_url_candidates(headers);
    let Some((path_without_hash, received_hash)) =
        strip_hash_from_path_and_query(uri_path_and_query)
    else {
        return HashVerifyAttempt {
            ok: false,
            received_hash: None,
            candidate_bases,
            computed: Vec::new(),
        };
    };

    let mut computed = Vec::with_capacity(candidate_bases.len());
    for base in &candidate_bases {
        let url_without_hash = format!("{base}{path_without_hash}");
        let expected = callback_hash_hex(&url_without_hash, secret_key).unwrap_or_default();
        let matches = constant_time_eq(received_hash.as_bytes(), expected.as_bytes());
        computed.push((url_without_hash, expected));
        if matches {
            return HashVerifyAttempt {
                ok: true,
                received_hash: Some(received_hash),
                candidate_bases,
                computed,
            };
        }
    }

    HashVerifyAttempt {
        ok: false,
        received_hash: Some(received_hash),
        candidate_bases,
        computed,
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[derive(Debug, Deserialize)]
pub struct BitlabsCallbackQuery {
    #[serde(default, alias = "UID")]
    pub uid: String,
    #[serde(default)]
    pub val: Option<String>,
    #[serde(default, alias = "RAW")]
    pub raw: Option<String>,
    #[serde(default, alias = "TX")]
    pub tx: String,
    #[serde(default, alias = "type")]
    pub activity_type: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default, rename = "OFFER:TASK:STATE")]
    pub offer_task_state: Option<String>,
    #[serde(default)]
    pub debug: Option<String>,
}

#[derive(Clone, Default)]
pub struct BitlabsCallbackState {
    processed_tx: Arc<Mutex<HashSet<String>>>,
}

impl BitlabsCallbackState {
    pub fn new() -> Self {
        Self {
            processed_tx: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    async fn mark_processed(&self, tx: &str) -> bool {
        let mut set = self.processed_tx.lock().await;
        set.insert(tx.to_string())
    }
}

pub fn callback_state() -> &'static BitlabsCallbackState {
    static STATE: std::sync::OnceLock<BitlabsCallbackState> = std::sync::OnceLock::new();
    STATE.get_or_init(BitlabsCallbackState::new)
}

async fn parse_reward_usdt(
    state: &AppState,
    val: Option<&str>,
    raw: Option<&str>,
) -> Option<Decimal> {
    if let Some(raw) = raw.and_then(|s| s.parse::<f64>().ok()) {
        if raw > 0.0 {
            return Decimal::try_from(raw).ok().map(|d| d.round_dp(6));
        }
    }
    let cents = val.and_then(|s| s.parse::<f64>().ok())?;
    if cents <= 0.0 {
        return None;
    }
    let eur = Decimal::try_from(cents / 100.0).ok()?;
    state
        .currency
        .local_to_usdt(eur, Currency::Eur)
        .await
        .map(|d| d.round_dp(6))
}

fn is_reconciliation(activity_type: Option<&str>, offer_state: Option<&str>) -> bool {
    let t = activity_type.unwrap_or("").to_ascii_uppercase();
    if matches!(t.as_str(), "RECONCILIATION" | "RECONCILED") {
        return true;
    }
    offer_state
        .map(|s| s.eq_ignore_ascii_case("RECONCILED"))
        .unwrap_or(false)
}

fn is_reward(activity_type: Option<&str>, offer_state: Option<&str>) -> bool {
    let t = activity_type.unwrap_or("").to_ascii_uppercase();
    if matches!(
        t.as_str(),
        "COMPLETE" | "SCREENOUT" | "START_BONUS" | "COMPLETED"
    ) {
        return true;
    }
    if t.is_empty() {
        return offer_state
            .map(|s| s.eq_ignore_ascii_case("COMPLETED"))
            .unwrap_or(true);
    }
    false
}

pub async fn handle_callback(
    state: &AppState,
    method: Method,
    headers: HeaderMap,
    uri_path_and_query: String,
    query: BitlabsCallbackQuery,
) -> impl IntoResponse {
    let config = BitlabsConfig::from_env();
    let Some(secret) = config.secret_key.as_deref() else {
        warn!("bitlabs callback received but BITLABS_SECRET_KEY not set");
        return (StatusCode::SERVICE_UNAVAILABLE, "not configured");
    };

    info!(
        method = %method,
        uri = %uri_path_and_query,
        host = ?header_value(&headers, "host"),
        x_forwarded_host = ?header_value(&headers, "x-forwarded-host"),
        x_forwarded_proto = ?header_value(&headers, "x-forwarded-proto"),
        x_forwarded_for = ?header_value(&headers, "x-forwarded-for"),
        "bitlabs callback received"
    );

    let hash_verify = verify_callback_hash_for_request(&headers, &uri_path_and_query, secret);
    if !hash_verify.ok {
        warn!(
            method = %method,
            uri = %uri_path_and_query,
            host = ?header_value(&headers, "host"),
            x_forwarded_host = ?header_value(&headers, "x-forwarded-host"),
            x_forwarded_proto = ?header_value(&headers, "x-forwarded-proto"),
            x_forwarded_for = ?header_value(&headers, "x-forwarded-for"),
            candidate_bases = ?hash_verify.candidate_bases,
            received_hash = ?hash_verify.received_hash,
            computed_hashes = ?hash_verify.computed,
            "bitlabs callback hash mismatch"
        );
        return (StatusCode::FORBIDDEN, "invalid hash");
    }

    if query
        .debug
        .as_deref()
        .is_some_and(|d| d == "true" || d == "1")
    {
        info!(
            uid = %query.uid,
            tx = %query.tx,
            "bitlabs debug callback accepted (no wallet change)"
        );
        return (StatusCode::OK, "OK");
    }

    let user_id = match Uuid::parse_str(query.uid.trim()) {
        Ok(id) => id,
        Err(_) => {
            warn!(uid = %query.uid, "bitlabs callback invalid uid");
            return (StatusCode::BAD_REQUEST, "invalid uid");
        }
    };

    if !state.user_exists(user_id).await {
        warn!(%user_id, "bitlabs callback unknown user");
        return (StatusCode::NOT_FOUND, "user not found");
    }

    if state.is_user_banned(user_id).await {
        warn!(%user_id, "bitlabs callback for banned user");
        return (StatusCode::FORBIDDEN, "user banned");
    }

    let tx = query.tx.trim();
    if tx.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing tx");
    }

    if !callback_state().mark_processed(tx).await {
        info!(%user_id, %tx, "bitlabs duplicate callback ignored");
        return (StatusCode::OK, "OK");
    }

    if !config.enabled {
        info!(%user_id, %tx, "bitlabs callback verified (offerwall disabled, no wallet change)");
        return (StatusCode::OK, "OK");
    }

    let amount_usdt =
        match parse_reward_usdt(state, query.val.as_deref(), query.raw.as_deref()).await {
            Some(a) if a > Decimal::ZERO => a,
            _ => {
                info!(%user_id, %tx, "bitlabs callback zero reward");
                return (StatusCode::OK, "OK");
            }
        };

    let activity = query.activity_type.as_deref();
    let offer_state = query.offer_task_state.as_deref();

    let result = if is_reconciliation(activity, offer_state) {
        state.debit(user_id, amount_usdt).await
    } else if is_reward(activity, offer_state) {
        state.credit(user_id, amount_usdt).await
    } else {
        info!(%user_id, %tx, ?activity, ?offer_state, "bitlabs callback unhandled type");
        return (StatusCode::OK, "OK");
    };

    match result {
        Ok(balance) => {
            info!(
                %user_id,
                %tx,
                %amount_usdt,
                %balance,
                ?activity,
                "bitlabs wallet updated"
            );
            (StatusCode::OK, "OK")
        }
        Err(e) => {
            warn!(%user_id, %tx, error = %e, "bitlabs wallet update failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "wallet error")
        }
    }
}

pub async fn webhook(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    Query(query): Query<BitlabsCallbackQuery>,
) -> impl IntoResponse {
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/webhooks/bitlabs");
    handle_callback(
        &state,
        method,
        headers,
        path_and_query.to_string(),
        query,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_documented_hash_example() {
        let url = "https://publisher.com/complete?uid=8cc877ee-af19-488d-b28d-216fb866b996&val=500&hash=dbcd6bb8ca677344592842a52b4fca9bec36cd4b";
        let secret = "JLOIAUNMHFli7ZJOQVEzm98rzqnm9";
        assert!(verify_callback_hash(url, secret));
    }

    #[test]
    fn rejects_tampered_hash() {
        let url = "https://publisher.com/complete?uid=8cc877ee-af19-488d-b28d-216fb866b996&val=500&hash=deadbeef";
        assert!(!verify_callback_hash(url, "secret"));
    }

    #[test]
    fn verifies_with_explicit_hash_and_php_style_strip() {
        let url_without =
            "https://vantage-earn.onrender.com/webhooks/bitlabs?uid=abc&val=100&tx=t1&debug=true";
        let secret = "test-secret";
        let signed = sign_callback_url(url_without, secret);
        let hash = signed.rsplit_once("&hash=").unwrap().1;
        assert!(verify_callback_hash_explicit(&signed, hash, secret));
    }

    #[test]
    fn tries_configured_callback_base_url() {
        std::env::set_var(
            "BITLABS_CALLBACK_BASE_URL",
            "https://vantage-earn.onrender.com",
        );
        let secret = "proxy-secret";
        let path = "/webhooks/bitlabs?uid=abc&val=1&tx=t2";
        let signed_path = format!(
            "{path}&hash={}",
            callback_hash_hex(&format!("https://vantage-earn.onrender.com{path}"), secret,)
                .unwrap()
        );

        let mut headers = HeaderMap::new();
        headers.insert("host", "internal:10000".parse().unwrap());
        headers.insert("x-forwarded-proto", "http".parse().unwrap());

        let result = verify_callback_hash_for_request(&headers, &signed_path, secret);
        assert!(result.ok);
        std::env::remove_var("BITLABS_CALLBACK_BASE_URL");
    }

    #[test]
    fn preserves_raw_query_encoding() {
        let secret = "encoding-secret";
        let path = "/webhooks/bitlabs?uid=abc%2Bdef&val=1&tx=t4";
        let signed_path = format!(
            "{path}&hash={}",
            callback_hash_hex(&format!("https://example.com{path}"), secret).unwrap()
        );

        let mut headers = HeaderMap::new();
        headers.insert("host", "example.com".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());

        let result = verify_callback_hash_for_request(&headers, &signed_path, secret);
        assert!(result.ok);
    }

    #[test]
    fn normalizes_forwarded_host_port() {
        let secret = "port-secret";
        let path = "/webhooks/bitlabs?uid=abc&val=1&tx=t3";
        let signed_path = format!(
            "{path}&hash={}",
            callback_hash_hex(&format!("https://example.com{path}"), secret,).unwrap()
        );

        let mut headers = HeaderMap::new();
        headers.insert("host", "example.com:443".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());

        let result = verify_callback_hash_for_request(&headers, &signed_path, secret);
        assert!(result.ok);
    }

    #[test]
    fn enabled_requires_token_and_secret() {
        std::env::set_var("BITLABS_APP_TOKEN", "tok");
        std::env::set_var("BITLABS_SECRET_KEY", "sec");
        std::env::set_var("OFFERWALL_ENABLED", "true");
        let cfg = BitlabsConfig::from_env();
        assert!(cfg.enabled);
        std::env::remove_var("BITLABS_APP_TOKEN");
        std::env::remove_var("BITLABS_SECRET_KEY");
        std::env::remove_var("OFFERWALL_ENABLED");
    }
}
