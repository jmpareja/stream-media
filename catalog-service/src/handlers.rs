use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::error::AppError;
use common::models::{
    CreateMediaRequest, CreateSmbSourceRequest, ListMediaQuery, ListMediaResponse,
    ListSmbSourcesResponse, MediaItem, RegisterMediaRequest, RegisterSmbMediaRequest, SmbSource,
    UpdateTranscodeStatusRequest, UpdateMediaRequest, UpdateSmbSourceRequest,
};
use uuid::Uuid;

use crate::db::SqliteCatalogRepository;

// ── Media handlers ──

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

pub async fn register_smb_media(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Json(req): Json<RegisterSmbMediaRequest>,
) -> Result<(StatusCode, Json<MediaItem>), AppError> {
    let item = repo.register_smb(req).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn update_transcode_status(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTranscodeStatusRequest>,
) -> Result<Json<MediaItem>, AppError> {
    let item = repo.update_transcode_status(id, req).await?;
    Ok(Json(item))
}

// ── SMB Source handlers ──

pub async fn create_smb_source(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Json(req): Json<CreateSmbSourceRequest>,
) -> Result<(StatusCode, Json<SmbSource>), AppError> {
    let source = repo.create_smb_source(req).await?;
    Ok((StatusCode::CREATED, Json(source)))
}

pub async fn get_smb_source(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
) -> Result<Json<SmbSource>, AppError> {
    let source = repo.get_smb_source(id).await?;
    Ok(Json(source))
}

pub async fn list_smb_sources(
    State(repo): State<Arc<SqliteCatalogRepository>>,
) -> Result<Json<ListSmbSourcesResponse>, AppError> {
    let response = repo.list_smb_sources().await?;
    Ok(Json(response))
}

pub async fn update_smb_source(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSmbSourceRequest>,
) -> Result<Json<SmbSource>, AppError> {
    let source = repo.update_smb_source(id, req).await?;
    Ok(Json(source))
}

pub async fn delete_smb_source(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    repo.delete_smb_source(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn mount_smb_source(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
) -> Result<Json<SmbSource>, AppError> {
    let source = repo.mount_smb_source(id).await?;
    Ok(Json(source))
}

pub async fn unmount_smb_source(
    State(repo): State<Arc<SqliteCatalogRepository>>,
    Path(id): Path<Uuid>,
) -> Result<Json<SmbSource>, AppError> {
    let source = repo.unmount_smb_source(id).await?;
    Ok(Json(source))
}
