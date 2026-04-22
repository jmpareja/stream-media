use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Video,
    Audio,
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Video => "video",
            MediaType::Audio => "audio",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "video" => Some(MediaType::Video),
            "audio" => Some(MediaType::Audio),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaSource {
    Local,
    Smb,
}

impl MediaSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaSource::Local => "local",
            MediaSource::Smb => "smb",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "local" => Some(MediaSource::Local),
            "smb" => Some(MediaSource::Smb),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HlsStatus {
    Pending,
    Processing,
    Ready,
    Failed,
    NotApplicable,
}

impl HlsStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HlsStatus::Pending => "pending",
            HlsStatus::Processing => "processing",
            HlsStatus::Ready => "ready",
            HlsStatus::Failed => "failed",
            HlsStatus::NotApplicable => "not_applicable",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(HlsStatus::Pending),
            "processing" => Some(HlsStatus::Processing),
            "ready" => Some(HlsStatus::Ready),
            "failed" => Some(HlsStatus::Failed),
            "not_applicable" => Some(HlsStatus::NotApplicable),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub media_type: MediaType,
    pub format: String,
    pub file_path: String,
    pub file_size: u64,
    pub duration_secs: Option<f64>,
    pub source: MediaSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_source_id: Option<Uuid>,
    pub hls_status: HlsStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hls_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMediaRequest {
    pub title: String,
    pub description: Option<String>,
    pub media_type: MediaType,
    pub format: String,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterMediaRequest {
    pub title: String,
    pub description: Option<String>,
    pub media_type: MediaType,
    pub format: String,
    pub duration_secs: Option<f64>,
    pub file_path: String,
    pub file_size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterSmbMediaRequest {
    pub title: String,
    pub description: Option<String>,
    pub media_type: MediaType,
    pub format: String,
    pub duration_secs: Option<f64>,
    pub source_id: Uuid,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMediaRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ListMediaResponse {
    pub items: Vec<MediaItem>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListMediaQuery {
    pub search: Option<String>,
    pub media_type: Option<MediaType>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

// ── HLS models ──

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateHlsStatusRequest {
    pub hls_status: HlsStatus,
    pub hls_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TranscodeJobStatus {
    pub media_id: Uuid,
    pub hls_status: HlsStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hls_error: Option<String>,
    pub variants: Vec<String>,
}

// ── SMB source models ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmbSource {
    pub id: Uuid,
    pub name: String,
    pub server: String,
    pub share_name: String,
    pub username: Option<String>,
    #[serde(skip_serializing, default)]
    pub password: Option<String>,
    pub mount_path: String,
    pub is_mounted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSmbSourceRequest {
    pub name: String,
    pub server: String,
    pub share_name: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSmbSourceRequest {
    pub name: Option<String>,
    pub server: Option<String>,
    pub share_name: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListSmbSourcesResponse {
    pub items: Vec<SmbSource>,
    pub total: u64,
}

// ── User models ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListUsersResponse {
    pub items: Vec<User>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
