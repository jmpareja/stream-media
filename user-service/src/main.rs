mod db;
mod handlers;
mod routes;

use std::sync::Arc;

use common::config::ServiceConfig;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "user_service=debug,tower_http=debug".into()),
        )
        .init();

    let config = ServiceConfig::from_env();

    let repo = Arc::new(
        db::SqliteUserRepository::new(&config.user_database_path)
            .expect("failed to initialize database"),
    );

    // Seed admin user from environment if configured
    if let (Ok(username), Ok(email), Ok(password)) = (
        std::env::var("ADMIN_USERNAME"),
        std::env::var("ADMIN_EMAIL"),
        std::env::var("ADMIN_PASSWORD"),
    ) {
        if let Err(e) = repo.seed_admin(username, email, password).await {
            tracing::error!("failed to seed admin user: {e}");
        }
    }

    let app = routes::build_router(repo);
    let addr = format!("0.0.0.0:{}", config.user_port);
    tracing::info!("user-service listening on {addr}");

    let listener = TcpListener::bind(&addr).await.expect("failed to bind");
    axum::serve(listener, app).await.expect("server error");
}
