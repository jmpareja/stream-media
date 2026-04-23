use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::db::SqliteUserRepository;
use crate::handlers;

pub fn build_router(repo: Arc<SqliteUserRepository>) -> Router {
    Router::new()
        .route("/users", get(handlers::list_users).post(handlers::create_user))
        .route("/users/login", post(handlers::login))
        .route(
            "/users/password-reset/request",
            post(handlers::request_password_reset),
        )
        .route(
            "/users/password-reset/confirm",
            post(handlers::confirm_password_reset),
        )
        .route(
            "/users/{id}",
            get(handlers::get_user)
                .put(handlers::update_user)
                .delete(handlers::delete_user),
        )
        .route("/users/{id}/password", post(handlers::change_password))
        .layer(TraceLayer::new_for_http())
        .with_state(repo)
}
