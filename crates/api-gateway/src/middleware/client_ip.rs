use axum::http::HeaderMap;
use std::net::SocketAddr;

/// Best-effort client IP from proxy headers or connect info.
pub fn client_ip(headers: &HeaderMap, connect_info: Option<SocketAddr>) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        })
        .or_else(|| connect_info.map(|a| a.ip().to_string()))
}

/// Short actor label for audit logs (hash prefix of configured admin secret).
pub fn admin_actor(headers: &HeaderMap) -> String {
    let secret = headers
        .get("X-Admin-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if secret.is_empty() {
        return "admin".into();
    }
    let digest = simple_hash(secret);
    format!("admin#{}", &digest[..digest.len().min(8)])
}

fn simple_hash(input: &str) -> String {
    let mut hash: u64 = 5381;
    for b in input.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(b));
    }
    format!("{:016x}", hash)
}
