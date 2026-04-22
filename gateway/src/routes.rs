use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, Method, Response};
use axum::routing::any;
use axum::Router;
use common::error::AppError;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::proxy;

#[derive(Clone)]
pub struct GatewayState {
    pub client: reqwest::Client,
    pub catalog_url: String,
    pub streaming_url: String,
    pub user_url: String,
}

async fn proxy_to_catalog(
    State(state): State<GatewayState>,
    req: Request<Body>,
) -> Result<Response<Body>, AppError> {
    proxy::proxy_request(&state.client, &state.catalog_url, req, "/api").await
}

async fn proxy_to_user(
    State(state): State<GatewayState>,
    req: Request<Body>,
) -> Result<Response<Body>, AppError> {
    proxy::proxy_request(&state.client, &state.user_url, req, "/api").await
}

async fn proxy_to_streaming(
    State(state): State<GatewayState>,
    req: Request<Body>,
) -> Result<Response<Body>, AppError> {
    proxy::proxy_request(&state.client, &state.streaming_url, req, "").await
}

pub fn build_router(state: GatewayState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .expose_headers([header::CONTENT_RANGE, header::ACCEPT_RANGES]);

    Router::new()
        .route("/api/media", any(proxy_to_catalog))
        .route("/api/media/{*rest}", any(proxy_to_catalog))
        .route("/api/sources/{*rest}", any(proxy_to_catalog))
        .route("/api/users", any(proxy_to_user))
        .route("/api/users/{*rest}", any(proxy_to_user))
        .route("/stream/{*rest}", any(proxy_to_streaming))
        .route("/upload", any(proxy_to_streaming))
        .route("/transcode/{*rest}", any(proxy_to_streaming))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
