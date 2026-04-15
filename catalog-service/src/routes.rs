use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::db::SqliteCatalogRepository;
use crate::handlers;

pub fn build_router(repo: Arc<SqliteCatalogRepository>) -> Router {
    Router::new()
        .route("/media", get(handlers::list_media).post(handlers::create_media))
        .route(
            "/media/{id}",
            get(handlers::get_media)
                .put(handlers::update_media)
                .delete(handlers::delete_media),
        )
        .route("/media/register", post(handlers::register_upload))
        .layer(TraceLayer::new_for_http())
        .with_state(repo)
}
