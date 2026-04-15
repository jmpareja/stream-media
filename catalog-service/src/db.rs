use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use common::error::AppError;
use common::models::{
    CreateMediaRequest, ListMediaQuery, ListMediaResponse, MediaItem, MediaType,
    RegisterMediaRequest, UpdateMediaRequest,
};
use rusqlite::{params, Connection};
use uuid::Uuid;

pub struct SqliteCatalogRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteCatalogRepository {
    pub fn new(path: &Path) -> Result<Self, AppError> {
        let conn = Connection::open(path)
            .map_err(|e| AppError::Internal(format!("failed to open database: {e}")))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| AppError::Internal(format!("failed to set WAL mode: {e}")))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS media_items (
                id            TEXT PRIMARY KEY,
                title         TEXT NOT NULL,
                description   TEXT,
                media_type    TEXT NOT NULL CHECK(media_type IN ('video', 'audio')),
                format        TEXT NOT NULL,
                file_path     TEXT NOT NULL,
                file_size     INTEGER NOT NULL,
                duration_secs REAL,
                created_at    TEXT NOT NULL,
                updated_at    TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_media_type ON media_items(media_type);
            CREATE INDEX IF NOT EXISTS idx_title ON media_items(title);",
        )
        .map_err(|e| AppError::Internal(format!("failed to create table: {e}")))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub async fn create(&self, req: CreateMediaRequest, file_path: String, file_size: u64) -> Result<MediaItem, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let id = Uuid::new_v4();
            let now = Utc::now();

            conn.execute(
                "INSERT INTO media_items (id, title, description, media_type, format, file_path, file_size, duration_secs, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id.to_string(),
                    req.title,
                    req.description,
                    req.media_type.as_str(),
                    req.format,
                    file_path,
                    file_size as i64,
                    req.duration_secs,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )
            .map_err(|e| AppError::Internal(format!("insert failed: {e}")))?;

            Ok(MediaItem {
                id,
                title: req.title,
                description: req.description,
                media_type: req.media_type,
                format: req.format,
                file_path,
                file_size,
                duration_secs: req.duration_secs,
                created_at: now,
                updated_at: now,
            })
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn register(&self, req: RegisterMediaRequest) -> Result<MediaItem, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let id = Uuid::new_v4();
            let now = Utc::now();

            conn.execute(
                "INSERT INTO media_items (id, title, description, media_type, format, file_path, file_size, duration_secs, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id.to_string(),
                    req.title,
                    req.description,
                    req.media_type.as_str(),
                    req.format,
                    req.file_path,
                    req.file_size as i64,
                    req.duration_secs,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )
            .map_err(|e| AppError::Internal(format!("insert failed: {e}")))?;

            Ok(MediaItem {
                id,
                title: req.title,
                description: req.description,
                media_type: req.media_type,
                format: req.format,
                file_path: req.file_path,
                file_size: req.file_size,
                duration_secs: req.duration_secs,
                created_at: now,
                updated_at: now,
            })
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn get(&self, id: Uuid) -> Result<MediaItem, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let mut stmt = conn
                .prepare("SELECT id, title, description, media_type, format, file_path, file_size, duration_secs, created_at, updated_at FROM media_items WHERE id = ?1")
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            stmt.query_row(params![id.to_string()], |row| {
                Ok(row_to_media_item(row))
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("media item {id} not found"))
                }
                _ => AppError::Internal(format!("query failed: {e}")),
            })?
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn list(&self, query: ListMediaQuery) -> Result<ListMediaResponse, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;

            let mut where_clauses = Vec::new();
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(ref search) = query.search {
                where_clauses.push("title LIKE ?");
                param_values.push(Box::new(format!("%{search}%")));
            }
            if let Some(ref media_type) = query.media_type {
                where_clauses.push("media_type = ?");
                param_values.push(Box::new(media_type.as_str().to_string()));
            }

            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", where_clauses.join(" AND "))
            };

            let limit = query.limit.unwrap_or(50);
            let offset = query.offset.unwrap_or(0);

            // Count total
            let count_sql = format!("SELECT COUNT(*) FROM media_items {where_sql}");
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();
            let total: u64 = conn
                .query_row(&count_sql, params_refs.as_slice(), |row| row.get(0))
                .map_err(|e| AppError::Internal(format!("count query failed: {e}")))?;

            // Fetch items
            let select_sql = format!(
                "SELECT id, title, description, media_type, format, file_path, file_size, duration_secs, created_at, updated_at \
                 FROM media_items {where_sql} ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = param_values;
            all_params.push(Box::new(limit));
            all_params.push(Box::new(offset));
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                all_params.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn
                .prepare(&select_sql)
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;
            let items = stmt
                .query_map(params_refs.as_slice(), |row| Ok(row_to_media_item(row)))
                .map_err(|e| AppError::Internal(format!("query failed: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| AppError::Internal(format!("row mapping failed: {e}")))?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;

            Ok(ListMediaResponse { items, total })
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn update(&self, id: Uuid, req: UpdateMediaRequest) -> Result<MediaItem, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let now = Utc::now();

            let mut set_clauses = vec!["updated_at = ?"];
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
                vec![Box::new(now.to_rfc3339())];

            if let Some(ref title) = req.title {
                set_clauses.push("title = ?");
                param_values.push(Box::new(title.clone()));
            }
            if let Some(ref description) = req.description {
                set_clauses.push("description = ?");
                param_values.push(Box::new(description.clone()));
            }
            if let Some(duration) = req.duration_secs {
                set_clauses.push("duration_secs = ?");
                param_values.push(Box::new(duration));
            }

            let sql = format!(
                "UPDATE media_items SET {} WHERE id = ?",
                set_clauses.join(", ")
            );
            param_values.push(Box::new(id.to_string()));
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let changed = conn
                .execute(&sql, params_refs.as_slice())
                .map_err(|e| AppError::Internal(format!("update failed: {e}")))?;

            if changed == 0 {
                return Err(AppError::NotFound(format!("media item {id} not found")));
            }

            // Re-fetch the updated row
            let mut stmt = conn
                .prepare("SELECT id, title, description, media_type, format, file_path, file_size, duration_secs, created_at, updated_at FROM media_items WHERE id = ?1")
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            stmt.query_row(params![id.to_string()], |row| Ok(row_to_media_item(row)))
                .map_err(|e| AppError::Internal(format!("query failed: {e}")))?
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let changed = conn
                .execute("DELETE FROM media_items WHERE id = ?1", params![id.to_string()])
                .map_err(|e| AppError::Internal(format!("delete failed: {e}")))?;

            if changed == 0 {
                return Err(AppError::NotFound(format!("media item {id} not found")));
            }
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }
}

fn row_to_media_item(row: &rusqlite::Row) -> Result<MediaItem, AppError> {
    let id_str: String = row
        .get(0)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let media_type_str: String = row
        .get(3)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let file_size: i64 = row
        .get(6)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let created_str: String = row
        .get(8)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let updated_str: String = row
        .get(9)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;

    Ok(MediaItem {
        id: Uuid::parse_str(&id_str)
            .map_err(|e| AppError::Internal(format!("invalid uuid: {e}")))?,
        title: row
            .get(1)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        description: row
            .get(2)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        media_type: MediaType::from_str(&media_type_str)
            .ok_or_else(|| AppError::Internal(format!("invalid media_type: {media_type_str}")))?,
        format: row
            .get(4)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        file_path: row
            .get(5)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        file_size: file_size as u64,
        duration_secs: row
            .get(7)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
    })
}
