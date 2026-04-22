mod handlers;
mod range;
mod routes;
mod transcode;

use std::sync::Arc;

use common::config::ServiceConfig;
use handlers::AppState;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

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

    let max_transcode_jobs: usize = std::env::var("TRANSCODE_MAX_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);

    tracing::info!(streaming_method = %config.streaming_method, "configured streaming method");

    let state = AppState {
        client: reqwest::Client::new(),
        catalog_url: config.catalog_url,
        media_store_path: config.media_store_path,
        transcode_semaphore: Arc::new(Semaphore::new(max_transcode_jobs)),
        streaming_method: config.streaming_method,
    };

    let app = routes::build_router(state);
    let addr = format!("0.0.0.0:{}", config.streaming_port);
    tracing::info!("streaming-service listening on {addr}");

    let listener = TcpListener::bind(&addr).await.expect("failed to bind");
    axum::serve(listener, app).await.expect("server error");
}
