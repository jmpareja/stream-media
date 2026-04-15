use std::path::PathBuf;

pub struct ServiceConfig {
    pub catalog_url: String,
    pub streaming_url: String,
    pub gateway_port: u16,
    pub catalog_port: u16,
    pub streaming_port: u16,
    pub media_store_path: PathBuf,
    pub database_path: PathBuf,
}

impl ServiceConfig {
    pub fn from_env() -> Self {
        Self {
            catalog_url: std::env::var("CATALOG_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3001".to_string()),
            streaming_url: std::env::var("STREAMING_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3002".to_string()),
            gateway_port: std::env::var("GATEWAY_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3000),
            catalog_port: std::env::var("CATALOG_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3001),
            streaming_port: std::env::var("STREAMING_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3002),
            media_store_path: std::env::var("MEDIA_STORE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./media-store")),
            database_path: std::env::var("DATABASE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./catalog.db")),
        }
    }
}
