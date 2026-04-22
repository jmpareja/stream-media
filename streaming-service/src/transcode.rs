use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::models::{HlsStatus, MediaItem, UpdateHlsStatusRequest};
use tokio::sync::Semaphore;

pub struct HlsVariant {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub video_bitrate: &'static str,
    pub audio_bitrate: &'static str,
    pub bandwidth: u32,
}

pub const VARIANTS: &[HlsVariant] = &[
    HlsVariant {
        name: "360p",
        width: 640,
        height: 360,
        video_bitrate: "800k",
        audio_bitrate: "96k",
        bandwidth: 896_000,
    },
    HlsVariant {
        name: "720p",
        width: 1280,
        height: 720,
        video_bitrate: "2500k",
        audio_bitrate: "128k",
        bandwidth: 2_628_000,
    },
    HlsVariant {
        name: "1080p",
        width: 1920,
        height: 1080,
        video_bitrate: "5000k",
        audio_bitrate: "192k",
        bandwidth: 5_192_000,
    },
];

pub const VARIANT_NAMES: &[&str] = &["360p", "720p", "1080p"];

pub fn generate_master_playlist() -> String {
    let mut m3u8 = String::from("#EXTM3U\n");
    for v in VARIANTS {
        m3u8.push_str(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={},RESOLUTION={}x{}\n{}/playlist.m3u8\n",
            v.bandwidth, v.width, v.height, v.name
        ));
    }
    m3u8
}

async fn update_catalog_hls_status(
    client: &reqwest::Client,
    catalog_url: &str,
    media_id: &uuid::Uuid,
    status: HlsStatus,
    error: Option<String>,
) {
    let url = format!("{catalog_url}/media/{media_id}/hls-status");
    let req = UpdateHlsStatusRequest {
        hls_status: status,
        hls_error: error,
    };
    let _ = client.patch(&url).json(&req).send().await;
}

async fn transcode_variant(
    input_path: &Path,
    output_dir: &Path,
    variant: &HlsVariant,
) -> Result<(), String> {
    let variant_dir = output_dir.join(variant.name);
    tokio::fs::create_dir_all(&variant_dir)
        .await
        .map_err(|e| format!("failed to create variant dir: {e}"))?;

    let segment_pattern = variant_dir.join("segment_%03d.ts");
    let playlist_path = variant_dir.join("playlist.m3u8");

    let scale_filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
        variant.width, variant.height, variant.width, variant.height
    );

    let bufsize = format!(
        "{}k",
        variant.video_bitrate.trim_end_matches('k').parse::<u32>().unwrap_or(2000) * 2
    );

    let output = tokio::process::Command::new("ffmpeg")
        .arg("-i")
        .arg(input_path)
        .arg("-vf")
        .arg(&scale_filter)
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("medium")
        .arg("-b:v")
        .arg(variant.video_bitrate)
        .arg("-maxrate")
        .arg(variant.video_bitrate)
        .arg("-bufsize")
        .arg(&bufsize)
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg(variant.audio_bitrate)
        .arg("-r")
        .arg("30")
        .arg("-f")
        .arg("hls")
        .arg("-hls_time")
        .arg("6")
        .arg("-hls_list_size")
        .arg("0")
        .arg("-hls_segment_filename")
        .arg(&segment_pattern)
        .arg(&playlist_path)
        .arg("-y")
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed for {}: {stderr}", variant.name));
    }

    Ok(())
}

pub async fn run_transcode_job(
    client: reqwest::Client,
    catalog_url: String,
    media_store_path: PathBuf,
    item: MediaItem,
    input_path: PathBuf,
    semaphore: Arc<Semaphore>,
) {
    let media_id = item.id;

    // Acquire semaphore permit to limit concurrent jobs
    let _permit = match semaphore.acquire().await {
        Ok(permit) => permit,
        Err(_) => {
            tracing::error!(media_id = %media_id, "transcode semaphore closed");
            return;
        }
    };

    tracing::info!(media_id = %media_id, "starting HLS transcode");

    update_catalog_hls_status(
        &client,
        &catalog_url,
        &media_id,
        HlsStatus::Processing,
        None,
    )
    .await;

    let output_dir = media_store_path.join(media_id.to_string());
    if let Err(e) = tokio::fs::create_dir_all(&output_dir).await {
        tracing::error!(media_id = %media_id, "failed to create HLS output dir: {e}");
        update_catalog_hls_status(
            &client,
            &catalog_url,
            &media_id,
            HlsStatus::Failed,
            Some(format!("failed to create output dir: {e}")),
        )
        .await;
        return;
    }

    // Transcode each variant sequentially to avoid CPU overload
    for variant in VARIANTS {
        tracing::info!(media_id = %media_id, variant = variant.name, "transcoding variant");
        if let Err(e) = transcode_variant(&input_path, &output_dir, variant).await {
            tracing::error!(media_id = %media_id, variant = variant.name, "transcode failed: {e}");
            update_catalog_hls_status(
                &client,
                &catalog_url,
                &media_id,
                HlsStatus::Failed,
                Some(e),
            )
            .await;
            return;
        }
    }

    // Write master playlist
    let master_playlist = generate_master_playlist();
    let master_path = output_dir.join("master.m3u8");
    if let Err(e) = tokio::fs::write(&master_path, &master_playlist).await {
        tracing::error!(media_id = %media_id, "failed to write master playlist: {e}");
        update_catalog_hls_status(
            &client,
            &catalog_url,
            &media_id,
            HlsStatus::Failed,
            Some(format!("failed to write master playlist: {e}")),
        )
        .await;
        return;
    }

    tracing::info!(media_id = %media_id, "HLS transcode complete");
    update_catalog_hls_status(
        &client,
        &catalog_url,
        &media_id,
        HlsStatus::Ready,
        None,
    )
    .await;
}
