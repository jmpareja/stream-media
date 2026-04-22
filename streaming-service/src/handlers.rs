use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, Response, StatusCode};
use axum::Json;
use common::error::AppError;
use common::models::{MediaItem, MediaSource, MediaType, RegisterMediaRequest, SmbSource};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::range;

#[derive(Clone)]
pub struct AppState {
    pub client: reqwest::Client,
    pub catalog_url: String,
    pub media_store_path: PathBuf,
}

pub async fn stream_media(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    // Fetch metadata from catalog service
    let url = format!("{}/media/{id}", state.catalog_url);
    let resp = state
        .client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("catalog request failed: {e}")))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::NotFound(format!("media item {id} not found")));
    }
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "catalog returned status {}",
            resp.status()
        )));
    }

    let item: MediaItem = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("failed to parse catalog response: {e}")))?;

    let file_path = match item.source {
        MediaSource::Smb => {
            let source_id = item.smb_source_id.ok_or_else(|| {
                AppError::Internal("smb media item missing source_id".to_string())
            })?;

            // Fetch source details from catalog to get mount_path
            let source_url = format!("{}/sources/smb/{source_id}", state.catalog_url);
            let source_resp = state
                .client
                .get(&source_url)
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("source lookup failed: {e}")))?;

            if !source_resp.status().is_success() {
                return Err(AppError::Internal(format!(
                    "source lookup returned status {}",
                    source_resp.status()
                )));
            }

            let source: SmbSource = source_resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("failed to parse source response: {e}")))?;

            PathBuf::from(&source.mount_path).join(&item.file_path)
        }
        MediaSource::Local => state.media_store_path.join(&item.file_path),
    };
    if !file_path.exists() {
        return Err(AppError::NotFound(format!(
            "media file not found: {}",
            file_path.display()
        )));
    }

    let metadata = tokio::fs::metadata(&file_path).await?;
    let file_size = metadata.len();

    let content_type = mime_guess::from_ext(&item.format)
        .first_or_octet_stream()
        .to_string();

    if let Some(range_header) = headers.get("range") {
        let range_str = range_header
            .to_str()
            .map_err(|_| AppError::BadRequest("invalid range header encoding".to_string()))?;
        let byte_range = range::parse_range(range_str, file_size)?;
        range::build_range_response(&file_path, byte_range, file_size, &content_type).await
    } else {
        range::build_full_response(&file_path, file_size, &content_type).await
    }
}

pub async fn upload_media(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MediaItem>), AppError> {
    let mut title: Option<String> = None;
    let mut description: Option<String> = None;
    let mut media_type: Option<MediaType> = None;
    let mut format: Option<String> = None;
    let mut duration_secs: Option<f64> = None;
    let mut file_data: Option<(String, u64)> = None; // (relative_path, size)

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "title" => {
                title = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::BadRequest(format!("failed to read title: {e}")))?,
                );
            }
            "description" => {
                description = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| {
                            AppError::BadRequest(format!("failed to read description: {e}"))
                        })?,
                );
            }
            "media_type" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| {
                        AppError::BadRequest(format!("failed to read media_type: {e}"))
                    })?;
                media_type = Some(
                    MediaType::from_str(&val)
                        .ok_or_else(|| AppError::BadRequest(format!("invalid media_type: {val}")))?,
                );
            }
            "format" => {
                format = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| {
                            AppError::BadRequest(format!("failed to read format: {e}"))
                        })?,
                );
            }
            "duration_secs" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| {
                        AppError::BadRequest(format!("failed to read duration_secs: {e}"))
                    })?;
                duration_secs = Some(val.parse().map_err(|_| {
                    AppError::BadRequest("invalid duration_secs".to_string())
                })?);
            }
            "file" => {
                let fmt = format
                    .as_deref()
                    .ok_or_else(|| {
                        AppError::BadRequest(
                            "format field must appear before file field".to_string(),
                        )
                    })?
                    .to_string();

                let file_id = Uuid::new_v4();
                let relative_path = format!("{file_id}.{fmt}");
                let full_path = state.media_store_path.join(&relative_path);

                let mut out_file = tokio::fs::File::create(&full_path).await.map_err(|e| {
                    AppError::Internal(format!("failed to create file: {e}"))
                })?;

                let bytes = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read file data: {e}"))
                })?;
                let size = bytes.len() as u64;
                out_file.write_all(&bytes).await?;
                out_file.flush().await?;

                file_data = Some((relative_path, size));
            }
            _ => {
                // Skip unknown fields
            }
        }
    }

    let title = title.ok_or_else(|| AppError::BadRequest("missing title".to_string()))?;
    let media_type =
        media_type.ok_or_else(|| AppError::BadRequest("missing media_type".to_string()))?;
    let format = format.ok_or_else(|| AppError::BadRequest("missing format".to_string()))?;
    let (file_path, file_size) =
        file_data.ok_or_else(|| AppError::BadRequest("missing file".to_string()))?;

    // Register with catalog service
    let register_req = RegisterMediaRequest {
        title,
        description,
        media_type,
        format,
        duration_secs,
        file_path,
        file_size,
    };

    let url = format!("{}/media/register", state.catalog_url);
    let resp = state
        .client
        .post(&url)
        .json(&register_req)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("catalog register request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "catalog register returned {status}: {body}"
        )));
    }

    let item: MediaItem = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("failed to parse register response: {e}")))?;

    Ok((StatusCode::CREATED, Json(item)))
}
