use std::sync::Arc;

use axum::routing::{get, patch, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::db::SqliteCatalogRepository;
use crate::handlers;

pub fn build_router(repo: Arc<SqliteCatalogRepository>) -> Router {
    Router::new()
        // Media routes
        .route("/media", get(handlers::list_media).post(handlers::create_media))
        .route(
            "/media/{id}",
            get(handlers::get_media)
                .put(handlers::update_media)
                .delete(handlers::delete_media),
        )
        .route("/media/register", post(handlers::register_upload))
        .route("/media/register-smb", post(handlers::register_smb_media))
        .route("/media/{id}/hls-status", patch(handlers::update_hls_status))
        // SMB source routes
        .route(
            "/sources/smb",
            get(handlers::list_smb_sources).post(handlers::create_smb_source),
        )
        .route(
            "/sources/smb/{id}",
            get(handlers::get_smb_source)
                .put(handlers::update_smb_source)
                .delete(handlers::delete_smb_source),
        )
        .route("/sources/smb/{id}/mount", post(handlers::mount_smb_source))
        .route("/sources/smb/{id}/unmount", post(handlers::unmount_smb_source))
        .layer(TraceLayer::new_for_http())
        .with_state(repo)
}
