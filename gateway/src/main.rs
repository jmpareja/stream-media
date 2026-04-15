mod proxy;
mod routes;

use common::config::ServiceConfig;
use routes::GatewayState;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gateway=debug,tower_http=debug".into()),
        )
        .init();

    let config = ServiceConfig::from_env();

    let state = GatewayState {
        client: reqwest::Client::new(),
        catalog_url: config.catalog_url,
        streaming_url: config.streaming_url,
        user_url: config.user_url,
    };

    let app = routes::build_router(state);
    let addr = format!("0.0.0.0:{}", config.gateway_port);
    tracing::info!("gateway listening on {addr}");

    let listener = TcpListener::bind(&addr).await.expect("failed to bind");
    axum::serve(listener, app).await.expect("server error");
}
