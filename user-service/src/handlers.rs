use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::error::AppError;
use common::models::{CreateUserRequest, ListUsersQuery, ListUsersResponse, UpdateUserRequest, User};
use uuid::Uuid;

use crate::db::SqliteUserRepository;

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
