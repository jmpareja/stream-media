use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use common::error::AppError;
use common::models::{CreateUserRequest, ListUsersQuery, ListUsersResponse, UpdateUserRequest, User};
use rusqlite::{params, Connection};
use uuid::Uuid;

pub struct SqliteUserRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteUserRepository {
    pub fn new(path: &Path) -> Result<Self, AppError> {
        let conn = Connection::open(path)
            .map_err(|e| AppError::Internal(format!("failed to open database: {e}")))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| AppError::Internal(format!("failed to set WAL mode: {e}")))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id           TEXT PRIMARY KEY,
                username     TEXT NOT NULL UNIQUE,
                email        TEXT NOT NULL UNIQUE,
                display_name TEXT,
                created_at   TEXT NOT NULL,
                updated_at   TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_username ON users(username);
            CREATE INDEX IF NOT EXISTS idx_email ON users(email);",
        )
        .map_err(|e| AppError::Internal(format!("failed to create table: {e}")))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub async fn create(&self, req: CreateUserRequest) -> Result<User, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let id = Uuid::new_v4();
            let now = Utc::now();

            conn.execute(
                "INSERT INTO users (id, username, email, display_name, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id.to_string(),
                    req.username,
                    req.email,
                    req.display_name,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )
            .map_err(|e| {
                if let rusqlite::Error::SqliteFailure(err, _) = &e {
                    if err.code == rusqlite::ErrorCode::ConstraintViolation {
                        return AppError::BadRequest("username or email already exists".to_string());
                    }
                }
                AppError::Internal(format!("insert failed: {e}"))
            })?;

            Ok(User {
                id,
                username: req.username,
                email: req.email,
                display_name: req.display_name,
                created_at: now,
                updated_at: now,
            })
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn get(&self, id: Uuid) -> Result<User, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let mut stmt = conn
                .prepare("SELECT id, username, email, display_name, created_at, updated_at FROM users WHERE id = ?1")
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            stmt.query_row(params![id.to_string()], |row| Ok(row_to_user(row)))
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        AppError::NotFound(format!("user {id} not found"))
                    }
                    _ => AppError::Internal(format!("query failed: {e}")),
                })?
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn list(&self, query: ListUsersQuery) -> Result<ListUsersResponse, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;

            let mut where_clauses = Vec::new();
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(ref search) = query.search {
                where_clauses.push("(username LIKE ? OR email LIKE ? OR display_name LIKE ?)");
                let pattern = format!("%{search}%");
                param_values.push(Box::new(pattern.clone()));
                param_values.push(Box::new(pattern.clone()));
                param_values.push(Box::new(pattern));
            }

            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", where_clauses.join(" AND "))
            };

            let limit = query.limit.unwrap_or(50);
            let offset = query.offset.unwrap_or(0);

            // Count total
            let count_sql = format!("SELECT COUNT(*) FROM users {where_sql}");
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();
            let total: u64 = conn
                .query_row(&count_sql, params_refs.as_slice(), |row| row.get(0))
                .map_err(|e| AppError::Internal(format!("count query failed: {e}")))?;

            // Fetch items
            let select_sql = format!(
                "SELECT id, username, email, display_name, created_at, updated_at \
                 FROM users {where_sql} ORDER BY created_at DESC LIMIT ? OFFSET ?"
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
                .query_map(params_refs.as_slice(), |row| Ok(row_to_user(row)))
                .map_err(|e| AppError::Internal(format!("query failed: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| AppError::Internal(format!("row mapping failed: {e}")))?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;

            Ok(ListUsersResponse { items, total })
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn update(&self, id: Uuid, req: UpdateUserRequest) -> Result<User, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let now = Utc::now();

            let mut set_clauses = vec!["updated_at = ?"];
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
                vec![Box::new(now.to_rfc3339())];

            if let Some(ref username) = req.username {
                set_clauses.push("username = ?");
                param_values.push(Box::new(username.clone()));
            }
            if let Some(ref email) = req.email {
                set_clauses.push("email = ?");
                param_values.push(Box::new(email.clone()));
            }
            if let Some(ref display_name) = req.display_name {
                set_clauses.push("display_name = ?");
                param_values.push(Box::new(display_name.clone()));
            }

            let sql = format!(
                "UPDATE users SET {} WHERE id = ?",
                set_clauses.join(", ")
            );
            param_values.push(Box::new(id.to_string()));
            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let changed = conn
                .execute(&sql, params_refs.as_slice())
                .map_err(|e| {
                    if let rusqlite::Error::SqliteFailure(err, _) = &e {
                        if err.code == rusqlite::ErrorCode::ConstraintViolation {
                            return AppError::BadRequest(
                                "username or email already exists".to_string(),
                            );
                        }
                    }
                    AppError::Internal(format!("update failed: {e}"))
                })?;

            if changed == 0 {
                return Err(AppError::NotFound(format!("user {id} not found")));
            }

            // Re-fetch the updated row
            let mut stmt = conn
                .prepare("SELECT id, username, email, display_name, created_at, updated_at FROM users WHERE id = ?1")
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            stmt.query_row(params![id.to_string()], |row| Ok(row_to_user(row)))
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
                .execute("DELETE FROM users WHERE id = ?1", params![id.to_string()])
                .map_err(|e| AppError::Internal(format!("delete failed: {e}")))?;

            if changed == 0 {
                return Err(AppError::NotFound(format!("user {id} not found")));
            }
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }
}

fn row_to_user(row: &rusqlite::Row) -> Result<User, AppError> {
    let id_str: String = row
        .get(0)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let created_str: String = row
        .get(4)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let updated_str: String = row
        .get(5)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;

    Ok(User {
        id: Uuid::parse_str(&id_str)
            .map_err(|e| AppError::Internal(format!("invalid uuid: {e}")))?,
        username: row
            .get(1)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        email: row
            .get(2)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        display_name: row
            .get(3)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
    })
}
