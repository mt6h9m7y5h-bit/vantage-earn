use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use api_gateway::{routes, state::AppState};

fn listen_port() -> u16 {
    std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000)
}

fn local_lan_urls(port: u16) -> Vec<String> {
    local_ip_address::list_afinet_netifas()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, ip)| ip.is_ipv4() && !ip.is_loopback())
        .map(|(_, ip)| format!("http://{ip}:{port}/demo"))
        .collect()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_gateway=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::connect().await;
    let app = Router::new()
        .merge(routes::router())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = listen_port();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("VANTAGE-EARN API listening on http://127.0.0.1:{port}");
    for url in local_lan_urls(port) {
        tracing::info!("Phone / LAN: open {url} (same Wi‑Fi as this Mac)");
    }

    let listener = TcpListener::bind(addr).await.expect("bind failed");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server failed");
}
