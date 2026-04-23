use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::error::AppError;
use common::models::{
    ChangePasswordRequest, CreateUserRequest, ListUsersQuery, ListUsersResponse, LoginRequest,
    PasswordResetConfirmRequest, PasswordResetRequest, PasswordResetResponse, UpdateUserRequest,
    User,
};
use uuid::Uuid;

use crate::db::SqliteUserRepository;

const MIN_PASSWORD_LEN: usize = 8;

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

pub async fn create_user(
    State(repo): State<Arc<SqliteUserRepository>>,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>), AppError> {
    let user = repo.create(req).await?;
    Ok((StatusCode::CREATED, Json(user)))
}

pub async fn get_user(
    State(repo): State<Arc<SqliteUserRepository>>,
    Path(id): Path<Uuid>,
) -> Result<Json<User>, AppError> {
    let user = repo.get(id).await?;
    Ok(Json(user))
}

pub async fn list_users(
    State(repo): State<Arc<SqliteUserRepository>>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<ListUsersResponse>, AppError> {
    let response = repo.list(query).await?;
    Ok(Json(response))
}

pub async fn update_user(
    State(repo): State<Arc<SqliteUserRepository>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<User>, AppError> {
    let user = repo.update(id, req).await?;
    Ok(Json(user))
}

pub async fn delete_user(
    State(repo): State<Arc<SqliteUserRepository>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    repo.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn request_password_reset(
    State(repo): State<Arc<SqliteUserRepository>>,
    Json(req): Json<PasswordResetRequest>,
) -> Result<Json<PasswordResetResponse>, AppError> {
    let generic_message =
        "If an account matches, a reset token has been generated.".to_string();

    let issued = repo.request_password_reset(req.identifier).await?;

    match issued {
        Some(token) => {
            tracing::info!(
                expires_at = %token.expires_at,
                "password reset token issued: {}",
                token.token,
            );
            Ok(Json(PasswordResetResponse {
                message: generic_message,
                reset_token: Some(token.token),
                expires_at: Some(token.expires_at),
            }))
        }
        None => Ok(Json(PasswordResetResponse {
            message: generic_message,
            reset_token: None,
            expires_at: None,
        })),
    }
}

pub async fn confirm_password_reset(
    State(repo): State<Arc<SqliteUserRepository>>,
    Json(req): Json<PasswordResetConfirmRequest>,
) -> Result<StatusCode, AppError> {
    validate_password(&req.new_password)?;
    repo.confirm_password_reset(req.token, req.new_password).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_password(
    State(repo): State<Arc<SqliteUserRepository>>,
    Path(id): Path<Uuid>,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    validate_password(&req.new_password)?;
    repo.change_password(id, req.current_password, req.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn login(
    State(repo): State<Arc<SqliteUserRepository>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<User>, AppError> {
    let user = repo.authenticate(req.identifier, req.password).await?;
    Ok(Json(user))
}
