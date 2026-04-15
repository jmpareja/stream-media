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
