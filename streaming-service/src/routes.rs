use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::handlers::{self, AppState};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Direct streaming
        .route("/stream/{id}", get(handlers::stream_media))
        // HLS streaming
        .route("/stream/{id}/hls/master.m3u8", get(handlers::serve_hls_master))
        .route("/stream/{id}/hls/{variant}/playlist.m3u8", get(handlers::serve_hls_playlist))
        .route("/stream/{id}/hls/{variant}/{segment}", get(handlers::serve_hls_segment))
        // DASH streaming
        .route("/stream/{id}/dash/manifest.mpd", get(handlers::serve_dash_manifest))
        .route("/stream/{id}/dash/{repr}/{file}", get(handlers::serve_dash_file))
        // Upload
        .route("/upload", post(handlers::upload_media))
        // Transcode control
        .route("/transcode/{id}", post(handlers::start_transcode))
        .route("/transcode/{id}/status", get(handlers::transcode_status))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
