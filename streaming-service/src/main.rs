mod handlers;
mod range;
mod routes;

use common::config::ServiceConfig;
use handlers::AppState;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "streaming_service=debug,tower_http=debug".into()),
        )
        .init();

    let config = ServiceConfig::from_env();

    // Ensure media store directory exists
    tokio::fs::create_dir_all(&config.media_store_path)
        .await
        .expect("failed to create media store directory");

    let state = AppState {
        client: reqwest::Client::new(),
        catalog_url: config.catalog_url,
        media_store_path: config.media_store_path,
    };

    let app = routes::build_router(state);
    let addr = format!("0.0.0.0:{}", config.streaming_port);
    tracing::info!("streaming-service listening on {addr}");

    let listener = TcpListener::bind(&addr).await.expect("failed to bind");
    axum::serve(listener, app).await.expect("server error");
}
