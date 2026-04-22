use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use common::error::AppError;
use common::models::{
    CreateMediaRequest, CreateSmbSourceRequest, HlsStatus, ListMediaQuery, ListMediaResponse,
    ListSmbSourcesResponse, MediaItem, MediaSource, MediaType, RegisterMediaRequest,
    RegisterSmbMediaRequest, SmbSource, UpdateHlsStatusRequest, UpdateMediaRequest,
    UpdateSmbSourceRequest,
};
use rusqlite::{params, Connection};
use uuid::Uuid;

pub struct SqliteCatalogRepository {
    conn: Arc<Mutex<Connection>>,
    smb_mount_base: PathBuf,
}

impl SqliteCatalogRepository {
    pub fn new(path: &Path, smb_mount_base: PathBuf) -> Result<Self, AppError> {
        let conn = Connection::open(path)
            .map_err(|e| AppError::Internal(format!("failed to open database: {e}")))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| AppError::Internal(format!("failed to set pragmas: {e}")))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS smb_sources (
                id         TEXT PRIMARY KEY,
                name       TEXT NOT NULL,
                server     TEXT NOT NULL,
                share_name TEXT NOT NULL,
                username   TEXT,
                password   TEXT,
                mount_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS media_items (
                id            TEXT PRIMARY KEY,
                title         TEXT NOT NULL,
                description   TEXT,
                media_type    TEXT NOT NULL CHECK(media_type IN ('video', 'audio')),
                format        TEXT NOT NULL,
                file_path     TEXT NOT NULL,
                file_size     INTEGER NOT NULL,
                duration_secs REAL,
                source_type   TEXT NOT NULL DEFAULT 'local' CHECK(source_type IN ('local', 'smb')),
                smb_source_id TEXT REFERENCES smb_sources(id),
                hls_status    TEXT NOT NULL DEFAULT 'not_applicable' CHECK(hls_status IN ('pending', 'processing', 'ready', 'failed', 'not_applicable')),
                hls_error     TEXT,
                created_at    TEXT NOT NULL,
                updated_at    TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_media_type ON media_items(media_type);
            CREATE INDEX IF NOT EXISTS idx_title ON media_items(title);
            CREATE INDEX IF NOT EXISTS idx_source_type ON media_items(source_type);",
        )
        .map_err(|e| AppError::Internal(format!("failed to create tables: {e}")))?;

        // Migrate: add HLS columns if missing
        let has_hls: bool = conn
            .prepare("SELECT hls_status FROM media_items LIMIT 0")
            .is_ok();
        if !has_hls {
            conn.execute_batch(
                "ALTER TABLE media_items ADD COLUMN hls_status TEXT NOT NULL DEFAULT 'not_applicable' CHECK(hls_status IN ('pending', 'processing', 'ready', 'failed', 'not_applicable'));
                 ALTER TABLE media_items ADD COLUMN hls_error TEXT;",
            )
            .map_err(|e| AppError::Internal(format!("failed to migrate hls columns: {e}")))?;
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            smb_mount_base,
        })
    }

    // ── Media CRUD ──

    pub async fn create(&self, req: CreateMediaRequest, file_path: String, file_size: u64) -> Result<MediaItem, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let id = Uuid::new_v4();
            let now = Utc::now();

            let hls_status = if req.media_type == MediaType::Video {
                HlsStatus::Pending
            } else {
                HlsStatus::NotApplicable
            };

            conn.execute(
                "INSERT INTO media_items (id, title, description, media_type, format, file_path, file_size, duration_secs, source_type, hls_status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id.to_string(),
                    req.title,
                    req.description,
                    req.media_type.as_str(),
                    req.format,
                    file_path,
                    file_size as i64,
                    req.duration_secs,
                    MediaSource::Local.as_str(),
                    hls_status.as_str(),
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
                source: MediaSource::Local,
                smb_source_id: None,
                hls_status,
                hls_error: None,
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

            let hls_status = if req.media_type == MediaType::Video {
                HlsStatus::Pending
            } else {
                HlsStatus::NotApplicable
            };

            conn.execute(
                "INSERT INTO media_items (id, title, description, media_type, format, file_path, file_size, duration_secs, source_type, hls_status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id.to_string(),
                    req.title,
                    req.description,
                    req.media_type.as_str(),
                    req.format,
                    req.file_path,
                    req.file_size as i64,
                    req.duration_secs,
                    MediaSource::Local.as_str(),
                    hls_status.as_str(),
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
                source: MediaSource::Local,
                smb_source_id: None,
                hls_status,
                hls_error: None,
                created_at: now,
                updated_at: now,
            })
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn register_smb(&self, req: RegisterSmbMediaRequest) -> Result<MediaItem, AppError> {
        // Validate source exists and is mounted
        let source = self.get_smb_source(req.source_id).await?;
        if !source.is_mounted {
            return Err(AppError::BadRequest(format!(
                "smb source '{}' is not mounted",
                source.name
            )));
        }

        // Validate file exists on the mounted share
        let full_path = PathBuf::from(&source.mount_path).join(&req.path);
        if !full_path.exists() {
            return Err(AppError::BadRequest(format!(
                "file not found on share: {}",
                req.path
            )));
        }

        let file_size = std::fs::metadata(&full_path)
            .map_err(|e| AppError::Internal(format!("failed to stat file: {e}")))?
            .len();

        let conn = self.conn.clone();
        let source_id = req.source_id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let id = Uuid::new_v4();
            let now = Utc::now();

            let hls_status = if req.media_type == MediaType::Video {
                HlsStatus::Pending
            } else {
                HlsStatus::NotApplicable
            };

            conn.execute(
                "INSERT INTO media_items (id, title, description, media_type, format, file_path, file_size, duration_secs, source_type, smb_source_id, hls_status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    id.to_string(),
                    req.title,
                    req.description,
                    req.media_type.as_str(),
                    req.format,
                    req.path,
                    file_size as i64,
                    req.duration_secs,
                    MediaSource::Smb.as_str(),
                    source_id.to_string(),
                    hls_status.as_str(),
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
                file_path: req.path,
                file_size,
                duration_secs: req.duration_secs,
                source: MediaSource::Smb,
                smb_source_id: Some(source_id),
                hls_status,
                hls_error: None,
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
                .prepare(
                    "SELECT id, title, description, media_type, format, file_path, file_size, \
                     duration_secs, source_type, smb_source_id, hls_status, hls_error, \
                     created_at, updated_at \
                     FROM media_items WHERE id = ?1",
                )
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
                "SELECT id, title, description, media_type, format, file_path, file_size, \
                 duration_secs, source_type, smb_source_id, hls_status, hls_error, \
                 created_at, updated_at \
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
                .prepare(
                    "SELECT id, title, description, media_type, format, file_path, file_size, \
                     duration_secs, source_type, smb_source_id, hls_status, hls_error, \
                     created_at, updated_at \
                     FROM media_items WHERE id = ?1",
                )
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

    // ── HLS Status ──

    pub async fn update_hls_status(&self, id: Uuid, req: UpdateHlsStatusRequest) -> Result<MediaItem, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let now = Utc::now();

            let changed = conn
                .execute(
                    "UPDATE media_items SET hls_status = ?1, hls_error = ?2, updated_at = ?3 WHERE id = ?4",
                    params![
                        req.hls_status.as_str(),
                        req.hls_error,
                        now.to_rfc3339(),
                        id.to_string(),
                    ],
                )
                .map_err(|e| AppError::Internal(format!("update failed: {e}")))?;

            if changed == 0 {
                return Err(AppError::NotFound(format!("media item {id} not found")));
            }

            let mut stmt = conn
                .prepare(
                    "SELECT id, title, description, media_type, format, file_path, file_size, \
                     duration_secs, source_type, smb_source_id, hls_status, hls_error, \
                     created_at, updated_at \
                     FROM media_items WHERE id = ?1",
                )
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            stmt.query_row(params![id.to_string()], |row| Ok(row_to_media_item(row)))
                .map_err(|e| AppError::Internal(format!("query failed: {e}")))?
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    // ── SMB Source CRUD ──

    pub async fn create_smb_source(&self, req: CreateSmbSourceRequest) -> Result<SmbSource, AppError> {
        let mount_base = self.smb_mount_base.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let id = Uuid::new_v4();
            let now = Utc::now();
            let mount_path = mount_base.join(id.to_string());

            conn.execute(
                "INSERT INTO smb_sources (id, name, server, share_name, username, password, mount_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id.to_string(),
                    req.name,
                    req.server,
                    req.share_name,
                    req.username,
                    req.password,
                    mount_path.to_string_lossy().to_string(),
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )
            .map_err(|e| AppError::Internal(format!("insert failed: {e}")))?;

            Ok(SmbSource {
                id,
                name: req.name,
                server: req.server,
                share_name: req.share_name,
                username: req.username,
                password: req.password,
                mount_path: mount_path.to_string_lossy().to_string(),
                is_mounted: false,
                created_at: now,
                updated_at: now,
            })
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn get_smb_source(&self, id: Uuid) -> Result<SmbSource, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, server, share_name, username, password, mount_path, \
                     created_at, updated_at FROM smb_sources WHERE id = ?1",
                )
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            stmt.query_row(params![id.to_string()], |row| {
                Ok(row_to_smb_source(row))
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("smb source {id} not found"))
                }
                _ => AppError::Internal(format!("query failed: {e}")),
            })?
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn list_smb_sources(&self) -> Result<ListSmbSourcesResponse, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;

            let total: u64 = conn
                .query_row("SELECT COUNT(*) FROM smb_sources", [], |row| row.get(0))
                .map_err(|e| AppError::Internal(format!("count query failed: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, name, server, share_name, username, password, mount_path, \
                     created_at, updated_at FROM smb_sources ORDER BY created_at DESC",
                )
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            let items = stmt
                .query_map([], |row| Ok(row_to_smb_source(row)))
                .map_err(|e| AppError::Internal(format!("query failed: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| AppError::Internal(format!("row mapping failed: {e}")))?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;

            Ok(ListSmbSourcesResponse { items, total })
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn update_smb_source(&self, id: Uuid, req: UpdateSmbSourceRequest) -> Result<SmbSource, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let now = Utc::now();

            let mut set_clauses = vec!["updated_at = ?"];
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
                vec![Box::new(now.to_rfc3339())];

            if let Some(ref name) = req.name {
                set_clauses.push("name = ?");
                param_values.push(Box::new(name.clone()));
            }
            if let Some(ref server) = req.server {
                set_clauses.push("server = ?");
                param_values.push(Box::new(server.clone()));
            }
            if let Some(ref share_name) = req.share_name {
                set_clauses.push("share_name = ?");
                param_values.push(Box::new(share_name.clone()));
            }
            if let Some(ref username) = req.username {
                set_clauses.push("username = ?");
                param_values.push(Box::new(username.clone()));
            }
            if let Some(ref password) = req.password {
                set_clauses.push("password = ?");
                param_values.push(Box::new(password.clone()));
            }

            let sql = format!(
                "UPDATE smb_sources SET {} WHERE id = ?",
                set_clauses.join(", ")
            );
            param_values.push(Box::new(id.to_string()));
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let changed = conn
                .execute(&sql, params_refs.as_slice())
                .map_err(|e| AppError::Internal(format!("update failed: {e}")))?;

            if changed == 0 {
                return Err(AppError::NotFound(format!("smb source {id} not found")));
            }

            let mut stmt = conn
                .prepare(
                    "SELECT id, name, server, share_name, username, password, mount_path, \
                     created_at, updated_at FROM smb_sources WHERE id = ?1",
                )
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            stmt.query_row(params![id.to_string()], |row| Ok(row_to_smb_source(row)))
                .map_err(|e| AppError::Internal(format!("query failed: {e}")))?
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn delete_smb_source(&self, id: Uuid) -> Result<(), AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;

            // Check if any media items reference this source
            let count: u64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM media_items WHERE smb_source_id = ?1",
                    params![id.to_string()],
                    |row| row.get(0),
                )
                .map_err(|e| AppError::Internal(format!("count query failed: {e}")))?;
            if count > 0 {
                return Err(AppError::BadRequest(format!(
                    "cannot delete source: {count} media items still reference it"
                )));
            }

            let changed = conn
                .execute("DELETE FROM smb_sources WHERE id = ?1", params![id.to_string()])
                .map_err(|e| AppError::Internal(format!("delete failed: {e}")))?;

            if changed == 0 {
                return Err(AppError::NotFound(format!("smb source {id} not found")));
            }
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    // ── SMB Mount / Unmount ──

    pub async fn mount_smb_source(&self, id: Uuid) -> Result<SmbSource, AppError> {
        let source = self.get_smb_source(id).await?;
        if source.is_mounted {
            return Ok(source);
        }

        let mount_path = PathBuf::from(&source.mount_path);
        tokio::fs::create_dir_all(&mount_path).await.map_err(|e| {
            AppError::Internal(format!("failed to create mount directory: {e}"))
        })?;

        // Write a temporary credentials file (avoids passwords visible in /proc)
        let cred_path = mount_path.with_extension("credentials");
        let mut cred_content = String::new();
        if let Some(ref username) = source.username {
            cred_content.push_str(&format!("username={username}\n"));
        }
        if let Some(ref password) = source.password {
            cred_content.push_str(&format!("password={password}\n"));
        }

        if !cred_content.is_empty() {
            tokio::fs::write(&cred_path, &cred_content).await.map_err(|e| {
                AppError::Internal(format!("failed to write credentials file: {e}"))
            })?;

            // Restrict permissions on credentials file
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o600);
                tokio::fs::set_permissions(&cred_path, perms).await.map_err(|e| {
                    AppError::Internal(format!("failed to set credentials permissions: {e}"))
                })?;
            }
        }

        let smb_uri = format!("//{}/{}", source.server, source.share_name);
        let mount_str = mount_path.to_string_lossy().to_string();

        let mut cmd = tokio::process::Command::new("mount");
        cmd.arg("-t").arg("cifs").arg(&smb_uri).arg(&mount_str);

        if !cred_content.is_empty() {
            cmd.arg("-o").arg(format!("credentials={}", cred_path.display()));
        } else {
            cmd.arg("-o").arg("guest");
        }

        let output = cmd.output().await.map_err(|e| {
            AppError::Internal(format!("failed to execute mount command: {e}"))
        })?;

        // Clean up credentials file regardless of mount result
        let _ = tokio::fs::remove_file(&cred_path).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Internal(format!(
                "mount failed: {stderr}"
            )));
        }

        self.get_smb_source(id).await
    }

    pub async fn unmount_smb_source(&self, id: Uuid) -> Result<SmbSource, AppError> {
        let source = self.get_smb_source(id).await?;
        if !source.is_mounted {
            return Ok(source);
        }

        let output = tokio::process::Command::new("umount")
            .arg(&source.mount_path)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("failed to execute umount command: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Internal(format!(
                "unmount failed: {stderr}"
            )));
        }

        // Remove the mount directory
        let _ = tokio::fs::remove_dir(&source.mount_path).await;

        self.get_smb_source(id).await
    }
}

// Column indices for media_items:
// id(0), title(1), description(2), media_type(3), format(4), file_path(5),
// file_size(6), duration_secs(7), source_type(8), smb_source_id(9),
// hls_status(10), hls_error(11), created_at(12), updated_at(13)
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
    let source_type_str: String = row
        .get(8)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let smb_source_id_str: Option<String> = row
        .get(9)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let hls_status_str: String = row
        .get(10)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let hls_error: Option<String> = row
        .get(11)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let created_str: String = row
        .get(12)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let updated_str: String = row
        .get(13)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;

    let smb_source_id = smb_source_id_str
        .map(|s| {
            Uuid::parse_str(&s).map_err(|e| AppError::Internal(format!("invalid uuid: {e}")))
        })
        .transpose()?;

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
        source: MediaSource::from_str(&source_type_str)
            .ok_or_else(|| AppError::Internal(format!("invalid source_type: {source_type_str}")))?,
        smb_source_id,
        hls_status: HlsStatus::from_str(&hls_status_str)
            .ok_or_else(|| AppError::Internal(format!("invalid hls_status: {hls_status_str}")))?,
        hls_error,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
    })
}

// Column indices for smb_sources:
// id(0), name(1), server(2), share_name(3), username(4), password(5),
// mount_path(6), created_at(7), updated_at(8)
fn row_to_smb_source(row: &rusqlite::Row) -> Result<SmbSource, AppError> {
    let id_str: String = row
        .get(0)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let mount_path: String = row
        .get(6)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let created_str: String = row
        .get(7)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let updated_str: String = row
        .get(8)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;

    // Check actual mount status at runtime
    let is_mounted = Path::new(&mount_path).is_dir()
        && std::process::Command::new("mountpoint")
            .arg("-q")
            .arg(&mount_path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

    Ok(SmbSource {
        id: Uuid::parse_str(&id_str)
            .map_err(|e| AppError::Internal(format!("invalid uuid: {e}")))?,
        name: row
            .get(1)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        server: row
            .get(2)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        share_name: row
            .get(3)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        username: row
            .get(4)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        password: row
            .get(5)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        mount_path,
        is_mounted,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
    })
}
