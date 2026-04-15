use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::error::AppError;
use common::models::{
    CreateMediaRequest, ListMediaQuery, ListMediaResponse, MediaItem, RegisterMediaRequest,
    UpdateMediaRequest,
};
use uuid::Uuid;

use crate::db::SqliteCatalogRepository;

pub async fn create_media(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Json(req): Json<CreateMediaRequest>,
) -> Result<(StatusCode, Json<MediaItem>), AppError> {
    let item = repo.create(req, String::new(), 0).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn get_media(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
) -> Result<Json<MediaItem>, AppError> {
    let item = repo.get(id).await?;
    Ok(Json(item))
}

pub async fn list_media(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Query(query): Query<ListMediaQuery>,
) -> Result<Json<ListMediaResponse>, AppError> {
    let response = repo.list(query).await?;
    Ok(Json(response))
}

pub async fn update_media(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMediaRequest>,
) -> Result<Json<MediaItem>, AppError> {
    let item = repo.update(id, req).await?;
    Ok(Json(item))
}

pub async fn delete_media(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    repo.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn register_upload(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Json(req): Json<RegisterMediaRequest>,
) -> Result<(StatusCode, Json<MediaItem>), AppError> {
    let item = repo.register(req).await?;
    Ok((StatusCode::CREATED, Json(item)))
}
