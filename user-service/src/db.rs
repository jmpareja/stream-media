use std::path::Path;
use std::sync::{Arc, Mutex};

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use chrono::{Duration, Utc};
use common::error::AppError;
use common::models::{CreateUserRequest, ListUsersQuery, ListUsersResponse, UpdateUserRequest, User};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const RESET_TOKEN_TTL_SECS: i64 = 3600;

pub struct SqliteUserRepository {
    conn: Arc<Mutex<Connection>>,
}

fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("password hash failed: {e}")))
}

fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| AppError::Internal(format!("stored hash is invalid: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn generate_reset_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct IssuedResetToken {
    pub token: String,
    pub expires_at: chrono::DateTime<Utc>,
}

impl SqliteUserRepository {
    pub fn new(path: &Path) -> Result<Self, AppError> {
        let conn = Connection::open(path)
            .map_err(|e| AppError::Internal(format!("failed to open database: {e}")))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| AppError::Internal(format!("failed to set WAL mode: {e}")))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id            TEXT PRIMARY KEY,
                username      TEXT NOT NULL UNIQUE,
                email         TEXT NOT NULL UNIQUE,
                display_name  TEXT,
                is_admin      INTEGER NOT NULL DEFAULT 0,
                password_hash TEXT,
                created_at    TEXT NOT NULL,
                updated_at    TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_username ON users(username);
            CREATE INDEX IF NOT EXISTS idx_email ON users(email);
            CREATE TABLE IF NOT EXISTS password_reset_tokens (
                token_hash TEXT PRIMARY KEY,
                user_id    TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                used_at    TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_reset_user ON password_reset_tokens(user_id);",
        )
        .map_err(|e| AppError::Internal(format!("failed to create table: {e}")))?;

        // Migrate: add new columns if missing
        let has_password_hash: bool = conn
            .prepare("SELECT password_hash FROM users LIMIT 0")
            .is_ok();
        if !has_password_hash {
            conn.execute_batch(
                "ALTER TABLE users ADD COLUMN is_admin INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE users ADD COLUMN password_hash TEXT;",
            )
            .map_err(|e| AppError::Internal(format!("failed to migrate columns: {e}")))?;
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub async fn seed_admin(
        &self,
        username: String,
        email: String,
        password: String,
    ) -> Result<(), AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;

            // Check if admin already exists
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM users WHERE username = ?1",
                    params![username],
                    |row| row.get(0),
                )
                .map_err(|e| AppError::Internal(format!("query failed: {e}")))?;

            if exists {
                tracing::info!(username = %username, "admin user already exists, skipping seed");
                return Ok(());
            }

            let id = Uuid::new_v4();
            let now = Utc::now();
            let password_hash = hash_password(&password)?;

            conn.execute(
                "INSERT INTO users (id, username, email, display_name, is_admin, password_hash, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7)",
                params![
                    id.to_string(),
                    username,
                    email,
                    "Administrator",
                    password_hash,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )
            .map_err(|e| AppError::Internal(format!("admin seed insert failed: {e}")))?;

            tracing::info!(username = %username, "admin user created");
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn create(&self, req: CreateUserRequest) -> Result<User, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let id = Uuid::new_v4();
            let now = Utc::now();

            let password_hash = req
                .password
                .as_deref()
                .map(hash_password)
                .transpose()?;

            conn.execute(
                "INSERT INTO users (id, username, email, display_name, is_admin, password_hash, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7)",
                params![
                    id.to_string(),
                    req.username,
                    req.email,
                    req.display_name,
                    password_hash,
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
                is_admin: false,
                password_hash,
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
                .prepare(
                    "SELECT id, username, email, display_name, is_admin, password_hash, \
                     created_at, updated_at FROM users WHERE id = ?1",
                )
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
                "SELECT id, username, email, display_name, is_admin, password_hash, \
                 created_at, updated_at \
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
                .prepare(
                    "SELECT id, username, email, display_name, is_admin, password_hash, \
                     created_at, updated_at FROM users WHERE id = ?1",
                )
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

    /// Issue a password-reset token for the user matching `identifier`
    /// (username OR email). Returns None when no user matches — callers
    /// should respond identically in both cases to avoid user enumeration.
    pub async fn request_password_reset(
        &self,
        identifier: String,
    ) -> Result<Option<IssuedResetToken>, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;

            let user_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM users WHERE username = ?1 OR email = ?1",
                    params![identifier],
                    |row| row.get(0),
                )
                .ok();

            let Some(user_id) = user_id else {
                return Ok(None);
            };

            let token = generate_reset_token();
            let token_hash = hash_token(&token);
            let now = Utc::now();
            let expires_at = now + Duration::seconds(RESET_TOKEN_TTL_SECS);

            // Invalidate any previous outstanding tokens for this user so only
            // the most recently issued token is usable.
            conn.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1 AND used_at IS NULL",
                params![user_id],
            )
            .map_err(|e| AppError::Internal(format!("clear prior tokens failed: {e}")))?;

            conn.execute(
                "INSERT INTO password_reset_tokens (token_hash, user_id, expires_at, used_at, created_at)
                 VALUES (?1, ?2, ?3, NULL, ?4)",
                params![
                    token_hash,
                    user_id,
                    expires_at.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )
            .map_err(|e| AppError::Internal(format!("token insert failed: {e}")))?;

            Ok(Some(IssuedResetToken { token, expires_at }))
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn confirm_password_reset(
        &self,
        token: String,
        new_password: String,
    ) -> Result<(), AppError> {
        let new_hash = hash_password(&new_password)?;
        let token_hash = hash_token(&token);
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let tx = conn
                .transaction()
                .map_err(|e| AppError::Internal(format!("tx begin failed: {e}")))?;

            let row: Option<(String, String, Option<String>)> = tx
                .query_row(
                    "SELECT user_id, expires_at, used_at FROM password_reset_tokens \
                     WHERE token_hash = ?1",
                    params![token_hash],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            let Some((user_id, expires_at, used_at)) = row else {
                return Err(AppError::BadRequest("invalid reset token".to_string()));
            };

            if used_at.is_some() {
                return Err(AppError::BadRequest("reset token already used".to_string()));
            }

            let expires = chrono::DateTime::parse_from_rfc3339(&expires_at)
                .map_err(|e| AppError::Internal(format!("invalid expiry: {e}")))?
                .with_timezone(&Utc);
            if expires < Utc::now() {
                return Err(AppError::BadRequest("reset token has expired".to_string()));
            }

            let now = Utc::now();
            tx.execute(
                "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_hash, now.to_rfc3339(), user_id],
            )
            .map_err(|e| AppError::Internal(format!("password update failed: {e}")))?;

            tx.execute(
                "UPDATE password_reset_tokens SET used_at = ?1 WHERE token_hash = ?2",
                params![now.to_rfc3339(), token_hash],
            )
            .map_err(|e| AppError::Internal(format!("token mark-used failed: {e}")))?;

            tx.commit()
                .map_err(|e| AppError::Internal(format!("tx commit failed: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    pub async fn change_password(
        &self,
        id: Uuid,
        current_password: String,
        new_password: String,
    ) -> Result<(), AppError> {
        let new_hash = hash_password(&new_password)?;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;

            let stored: Option<String> = conn
                .query_row(
                    "SELECT password_hash FROM users WHERE id = ?1",
                    params![id.to_string()],
                    |row| row.get(0),
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        AppError::NotFound(format!("user {id} not found"))
                    }
                    _ => AppError::Internal(format!("query failed: {e}")),
                })?;

            let Some(stored_hash) = stored else {
                return Err(AppError::BadRequest(
                    "user has no password set; use password-reset flow".to_string(),
                ));
            };

            if !verify_password(&current_password, &stored_hash)? {
                return Err(AppError::BadRequest(
                    "current password is incorrect".to_string(),
                ));
            }

            let now = Utc::now();
            conn.execute(
                "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_hash, now.to_rfc3339(), id.to_string()],
            )
            .map_err(|e| AppError::Internal(format!("password update failed: {e}")))?;

            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }

    /// Verify credentials and return the matching user. Same error for
    /// unknown identifier and bad password so callers can't enumerate.
    pub async fn authenticate(
        &self,
        identifier: String,
        password: String,
    ) -> Result<User, AppError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| AppError::Internal(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, username, email, display_name, is_admin, password_hash, \
                     created_at, updated_at \
                     FROM users WHERE username = ?1 OR email = ?1",
                )
                .map_err(|e| AppError::Internal(format!("prepare failed: {e}")))?;

            let user = stmt
                .query_row(params![identifier], |row| Ok(row_to_user(row)))
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        AppError::Unauthorized("invalid credentials".to_string())
                    }
                    _ => AppError::Internal(format!("query failed: {e}")),
                })??;

            let Some(ref stored_hash) = user.password_hash else {
                return Err(AppError::Unauthorized("invalid credentials".to_string()));
            };

            if !verify_password(&password, stored_hash)? {
                return Err(AppError::Unauthorized("invalid credentials".to_string()));
            }

            Ok(user)
        })
        .await
        .map_err(|e| AppError::Internal(format!("task join error: {e}")))?
    }
}

// Column indices: id(0), username(1), email(2), display_name(3),
// is_admin(4), password_hash(5), created_at(6), updated_at(7)
fn row_to_user(row: &rusqlite::Row) -> Result<User, AppError> {
    let id_str: String = row
        .get(0)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let is_admin_int: i32 = row
        .get(4)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let created_str: String = row
        .get(6)
        .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?;
    let updated_str: String = row
        .get(7)
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
        is_admin: is_admin_int != 0,
        password_hash: row
            .get(5)
            .map_err(|e| AppError::Internal(format!("row get failed: {e}")))?,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
            .map_err(|e| AppError::Internal(format!("invalid datetime: {e}")))?
            .with_timezone(&chrono::Utc),
    })
}
