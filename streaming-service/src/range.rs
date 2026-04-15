use std::path::Path;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
use common::error::AppError;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

pub fn parse_range(header_value: &str, file_size: u64) -> Result<ByteRange, AppError> {
    let s = header_value
        .strip_prefix("bytes=")
        .ok_or_else(|| AppError::BadRequest("invalid range header".to_string()))?;

    // Only support single ranges
    if s.contains(',') {
        return Err(AppError::BadRequest(
            "multiple ranges not supported".to_string(),
        ));
    }

    let parts: Vec<&str> = s.splitn(2, '-').collect();
    if parts.len() != 2 {
        return Err(AppError::BadRequest("malformed range".to_string()));
    }

    let start = if parts[0].is_empty() {
        // Suffix range: -500 means last 500 bytes
        let suffix_len: u64 = parts[1]
            .parse()
            .map_err(|_| AppError::BadRequest("invalid range number".to_string()))?;
        file_size.saturating_sub(suffix_len)
    } else {
        parts[0]
            .parse()
            .map_err(|_| AppError::BadRequest("invalid range start".to_string()))?
    };

    let end = if parts[0].is_empty() {
        file_size - 1
    } else if parts[1].is_empty() {
        file_size - 1
    } else {
        let e: u64 = parts[1]
            .parse()
            .map_err(|_| AppError::BadRequest("invalid range end".to_string()))?;
        e.min(file_size - 1)
    };

    if start >= file_size || start > end {
        return Err(AppError::BadRequest(format!(
            "range not satisfiable: {start}-{end}/{file_size}"
        )));
    }

    Ok(ByteRange { start, end })
}

pub async fn build_range_response(
    file_path: &Path,
    range: ByteRange,
    file_size: u64,
    content_type: &str,
) -> Result<Response<Body>, AppError> {
    let mut file = tokio::fs::File::open(file_path).await?;
    file.seek(std::io::SeekFrom::Start(range.start)).await?;

    let chunk_size = range.end - range.start + 1;
    let limited = file.take(chunk_size);
    let stream = ReaderStream::new(limited);

    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_str(content_type).unwrap());
    headers.insert("content-length", HeaderValue::from(chunk_size));
    headers.insert(
        "content-range",
        HeaderValue::from_str(&format!("bytes {}-{}/{}", range.start, range.end, file_size))
            .unwrap(),
    );
    headers.insert("accept-ranges", HeaderValue::from_static("bytes"));

    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::PARTIAL_CONTENT;
    *response.headers_mut() = headers;
    Ok(response)
}

pub async fn build_full_response(
    file_path: &Path,
    file_size: u64,
    content_type: &str,
) -> Result<Response<Body>, AppError> {
    let file = tokio::fs::File::open(file_path).await?;
    let stream = ReaderStream::new(file);

    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_str(content_type).unwrap());
    headers.insert("content-length", HeaderValue::from(file_size));
    headers.insert("accept-ranges", HeaderValue::from_static("bytes"));

    let mut response = Response::new(Body::from_stream(stream));
    *response.headers_mut() = headers;
    Ok(response)
}
