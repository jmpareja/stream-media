use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
use axum::Json;
use common::error::AppError;
use common::models::{
    HlsStatus, MediaItem, MediaSource, MediaType, RegisterMediaRequest, SmbSource,
    TranscodeJobStatus,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::range;
use crate::transcode;

#[derive(Clone)]
pub struct AppState {
    pub client: reqwest::Client,
    pub catalog_url: String,
    pub media_store_path: PathBuf,
    pub transcode_semaphore: Arc<Semaphore>,
}

// ── Helpers ──

async fn fetch_media_item(state: &AppState, id: Uuid) -> Result<MediaItem, AppError> {
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

    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("failed to parse catalog response: {e}")))
}

async fn resolve_media_path(state: &AppState, item: &MediaItem) -> Result<PathBuf, AppError> {
    match item.source {
        MediaSource::Smb => {
            let source_id = item.smb_source_id.ok_or_else(|| {
                AppError::Internal("smb media item missing source_id".to_string())
            })?;

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

            Ok(PathBuf::from(&source.mount_path).join(&item.file_path))
        }
        MediaSource::Local => Ok(state.media_store_path.join(&item.file_path)),
    }
}

fn spawn_transcode(state: &AppState, item: &MediaItem, input_path: PathBuf) {
    let client = state.client.clone();
    let catalog_url = state.catalog_url.clone();
    let media_store_path = state.media_store_path.clone();
    let semaphore = state.transcode_semaphore.clone();
    let item = item.clone();

    tokio::spawn(transcode::run_transcode_job(
        client,
        catalog_url,
        media_store_path,
        item,
        input_path,
        semaphore,
    ));
}

// ── Direct streaming ──

pub async fn stream_media(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    let item = fetch_media_item(&state, id).await?;
    let file_path = resolve_media_path(&state, &item).await?;

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

// ── Upload ──

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
        file_path: file_path.clone(),
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

    // Auto-trigger HLS transcode for video uploads
    if item.media_type == MediaType::Video {
        let input_path = state.media_store_path.join(&file_path);
        spawn_transcode(&state, &item, input_path);
    }

    Ok((StatusCode::CREATED, Json(item)))
}

// ── HLS serving ──

pub async fn serve_hls_master(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response<Body>, AppError> {
    let master_path = state.media_store_path.join(id.to_string()).join("master.m3u8");
    if !master_path.exists() {
        return Err(AppError::NotFound(
            "HLS not available for this media item".to_string(),
        ));
    }

    let file = tokio::fs::File::open(&master_path).await?;
    let stream = ReaderStream::new(file);

    let mut response = Response::new(Body::from_stream(stream));
    response.headers_mut().insert(
        "content-type",
        HeaderValue::from_static("application/vnd.apple.mpegurl"),
    );
    Ok(response)
}

pub async fn serve_hls_playlist(
    State(state): State<AppState>,
    Path((id, variant)): Path<(Uuid, String)>,
) -> Result<Response<Body>, AppError> {
    if !transcode::VARIANT_NAMES.contains(&variant.as_str()) {
        return Err(AppError::BadRequest(format!("invalid variant: {variant}")));
    }

    let playlist_path = state
        .media_store_path
        .join(id.to_string())
        .join(&variant)
        .join("playlist.m3u8");

    if !playlist_path.exists() {
        return Err(AppError::NotFound(format!(
            "HLS variant playlist not found: {variant}"
        )));
    }

    let file = tokio::fs::File::open(&playlist_path).await?;
    let stream = ReaderStream::new(file);

    let mut response = Response::new(Body::from_stream(stream));
    response.headers_mut().insert(
        "content-type",
        HeaderValue::from_static("application/vnd.apple.mpegurl"),
    );
    Ok(response)
}

pub async fn serve_hls_segment(
    State(state): State<AppState>,
    Path((id, variant, segment)): Path<(Uuid, String, String)>,
) -> Result<Response<Body>, AppError> {
    if !transcode::VARIANT_NAMES.contains(&variant.as_str()) {
        return Err(AppError::BadRequest(format!("invalid variant: {variant}")));
    }

    // Validate segment filename to prevent path traversal
    let valid_segment = segment.starts_with("segment_")
        && segment.ends_with(".ts")
        && segment.len() <= 16
        && segment[8..segment.len() - 3]
            .chars()
            .all(|c| c.is_ascii_digit());
    if !valid_segment {
        return Err(AppError::BadRequest(format!(
            "invalid segment name: {segment}"
        )));
    }

    let segment_path = state
        .media_store_path
        .join(id.to_string())
        .join(&variant)
        .join(&segment);

    if !segment_path.exists() {
        return Err(AppError::NotFound(format!(
            "HLS segment not found: {variant}/{segment}"
        )));
    }

    let metadata = tokio::fs::metadata(&segment_path).await?;
    let file = tokio::fs::File::open(&segment_path).await?;
    let stream = ReaderStream::new(file);

    let mut response = Response::new(Body::from_stream(stream));
    response
        .headers_mut()
        .insert("content-type", HeaderValue::from_static("video/mp2t"));
    response
        .headers_mut()
        .insert("content-length", HeaderValue::from(metadata.len()));
    Ok(response)
}

// ── Transcode control ──

pub async fn start_transcode(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let item = fetch_media_item(&state, id).await?;

    if item.media_type != MediaType::Video {
        return Err(AppError::BadRequest(
            "HLS transcoding is only supported for video".to_string(),
        ));
    }

    let input_path = resolve_media_path(&state, &item).await?;
    if !input_path.exists() {
        return Err(AppError::NotFound(format!(
            "source file not found: {}",
            input_path.display()
        )));
    }

    spawn_transcode(&state, &item, input_path);

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "media_id": id,
            "hls_status": "pending"
        })),
    ))
}

pub async fn transcode_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TranscodeJobStatus>, AppError> {
    let item = fetch_media_item(&state, id).await?;

    let mut variants = Vec::new();
    if item.hls_status == HlsStatus::Ready {
        let hls_dir = state.media_store_path.join(id.to_string());
        for name in transcode::VARIANT_NAMES {
            if hls_dir.join(name).join("playlist.m3u8").exists() {
                variants.push((*name).to_string());
            }
        }
    }

    Ok(Json(TranscodeJobStatus {
        media_id: item.id,
        hls_status: item.hls_status,
        hls_error: item.hls_error,
        variants,
    }))
}
