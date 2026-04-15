use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::handlers::{self, AppState};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/stream/{id}", get(handlers::stream_media))
        .route("/upload", post(handlers::upload_media))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
