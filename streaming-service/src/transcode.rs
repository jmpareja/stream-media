use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::models::{TranscodeStatus, MediaItem, UpdateTranscodeStatusRequest};
use tokio::sync::Semaphore;

pub struct TranscodeVariant {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub video_bitrate: &'static str,
    pub audio_bitrate: &'static str,
    pub bandwidth: u32,
}

pub const VARIANTS: &[TranscodeVariant] = &[
    TranscodeVariant {
        name: "360p",
        width: 640,
        height: 360,
        video_bitrate: "800k",
        audio_bitrate: "96k",
        bandwidth: 896_000,
    },
    TranscodeVariant {
        name: "720p",
        width: 1280,
        height: 720,
        video_bitrate: "2500k",
        audio_bitrate: "128k",
        bandwidth: 2_628_000,
    },
    TranscodeVariant {
        name: "1080p",
        width: 1920,
        height: 1080,
        video_bitrate: "5000k",
        audio_bitrate: "192k",
        bandwidth: 5_192_000,
    },
];

pub const VARIANT_NAMES: &[&str] = &["360p", "720p", "1080p"];

// ── HLS ──

pub fn generate_hls_master_playlist() -> String {
    let mut m3u8 = String::from("#EXTM3U\n");
    for v in VARIANTS {
        m3u8.push_str(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={},RESOLUTION={}x{}\n{}/playlist.m3u8\n",
            v.bandwidth, v.width, v.height, v.name
        ));
    }
    m3u8
}

async fn transcode_hls_variant(
    input_path: &Path,
    output_dir: &Path,
    variant: &TranscodeVariant,
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
        .arg("-i").arg(input_path)
        .arg("-vf").arg(&scale_filter)
        .arg("-c:v").arg("libx264").arg("-preset").arg("medium")
        .arg("-b:v").arg(variant.video_bitrate)
        .arg("-maxrate").arg(variant.video_bitrate)
        .arg("-bufsize").arg(&bufsize)
        .arg("-c:a").arg("aac").arg("-b:a").arg(variant.audio_bitrate)
        .arg("-r").arg("30")
        .arg("-f").arg("hls")
        .arg("-hls_time").arg("6")
        .arg("-hls_list_size").arg("0")
        .arg("-hls_segment_filename").arg(&segment_pattern)
        .arg(&playlist_path)
        .arg("-y")
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg HLS failed for {}: {stderr}", variant.name));
    }
    Ok(())
}

// ── DASH ──

async fn transcode_dash_variant(
    input_path: &Path,
    output_dir: &Path,
    variant: &TranscodeVariant,
) -> Result<(), String> {
    let variant_dir = output_dir.join(variant.name);
    tokio::fs::create_dir_all(&variant_dir)
        .await
        .map_err(|e| format!("failed to create variant dir: {e}"))?;

    let output_path = variant_dir.join("stream.mp4");
    let scale_filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
        variant.width, variant.height, variant.width, variant.height
    );
    let bufsize = format!(
        "{}k",
        variant.video_bitrate.trim_end_matches('k').parse::<u32>().unwrap_or(2000) * 2
    );

    // First pass: encode each variant as a fragmented MP4
    let output = tokio::process::Command::new("ffmpeg")
        .arg("-i").arg(input_path)
        .arg("-vf").arg(&scale_filter)
        .arg("-c:v").arg("libx264").arg("-preset").arg("medium")
        .arg("-b:v").arg(variant.video_bitrate)
        .arg("-maxrate").arg(variant.video_bitrate)
        .arg("-bufsize").arg(&bufsize)
        .arg("-c:a").arg("aac").arg("-b:a").arg(variant.audio_bitrate)
        .arg("-r").arg("30")
        .arg("-movflags").arg("+faststart")
        .arg(&output_path)
        .arg("-y")
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg DASH encode failed for {}: {stderr}", variant.name));
    }
    Ok(())
}

async fn generate_dash_manifest(output_dir: &Path) -> Result<(), String> {
    // Use ffmpeg to package all variant MP4s into a DASH manifest
    let mut cmd = tokio::process::Command::new("ffmpeg");

    for variant in VARIANTS {
        let stream_path = output_dir.join(variant.name).join("stream.mp4");
        cmd.arg("-i").arg(&stream_path);
    }

    // Map all inputs
    for i in 0..VARIANTS.len() {
        cmd.arg("-map").arg(format!("{i}:v")).arg("-map").arg(format!("{i}:a"));
    }

    // Copy codecs (already encoded) and set DASH output
    cmd.arg("-c").arg("copy");

    // Set adaptation sets: one for video, one for audio
    let video_streams: String = (0..VARIANTS.len())
        .map(|i| (i * 2).to_string())
        .collect::<Vec<_>>()
        .join(",");
    let audio_streams: String = (0..VARIANTS.len())
        .map(|i| (i * 2 + 1).to_string())
        .collect::<Vec<_>>()
        .join(",");

    let adaptation_sets = format!(
        "id=0,streams={video_streams} id=1,streams={audio_streams}"
    );

    cmd.arg("-f").arg("dash")
        .arg("-seg_duration").arg("6")
        .arg("-use_template").arg("1")
        .arg("-use_timeline").arg("1")
        .arg("-init_seg_name").arg("$RepresentationID$/init.$ext$")
        .arg("-media_seg_name").arg("$RepresentationID$/chunk-$Number%05d$.$ext$")
        .arg("-adaptation_sets").arg(&adaptation_sets)
        .arg(output_dir.join("manifest.mpd"))
        .arg("-y");

    let output = cmd
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("failed to run ffmpeg DASH packaging: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg DASH manifest generation failed: {stderr}"));
    }
    Ok(())
}

// ── Catalog status helper ──

async fn update_catalog_transcode_status(
    client: &reqwest::Client,
    catalog_url: &str,
    media_id: &uuid::Uuid,
    status: TranscodeStatus,
    format: Option<String>,
    error: Option<String>,
) {
    let url = format!("{catalog_url}/media/{media_id}/transcode-status");
    let req = UpdateTranscodeStatusRequest {
        transcode_status: status,
        transcode_format: format,
        transcode_error: error,
    };
    let _ = client.patch(&url).json(&req).send().await;
}

// ── Main job runner ──

pub async fn run_transcode_job(
    client: reqwest::Client,
    catalog_url: String,
    media_store_path: PathBuf,
    item: MediaItem,
    input_path: PathBuf,
    semaphore: Arc<Semaphore>,
    transcode_format: String,
) {
    let media_id = item.id;

    let _permit = match semaphore.acquire().await {
        Ok(permit) => permit,
        Err(_) => {
            tracing::error!(media_id = %media_id, "transcode semaphore closed");
            return;
        }
    };

    tracing::info!(media_id = %media_id, format = %transcode_format, "starting transcode");

    update_catalog_transcode_status(
        &client, &catalog_url, &media_id,
        TranscodeStatus::Processing,
        Some(transcode_format.clone()),
        None,
    ).await;

    let output_dir = media_store_path.join(media_id.to_string());
    if let Err(e) = tokio::fs::create_dir_all(&output_dir).await {
        tracing::error!(media_id = %media_id, "failed to create output dir: {e}");
        update_catalog_transcode_status(
            &client, &catalog_url, &media_id,
            TranscodeStatus::Failed,
            Some(transcode_format),
            Some(format!("failed to create output dir: {e}")),
        ).await;
        return;
    }

    match transcode_format.as_str() {
        "hls" => {
            for variant in VARIANTS {
                tracing::info!(media_id = %media_id, variant = variant.name, "transcoding HLS variant");
                if let Err(e) = transcode_hls_variant(&input_path, &output_dir, variant).await {
                    tracing::error!(media_id = %media_id, variant = variant.name, "HLS transcode failed: {e}");
                    update_catalog_transcode_status(
                        &client, &catalog_url, &media_id,
                        TranscodeStatus::Failed, Some(transcode_format), Some(e),
                    ).await;
                    return;
                }
            }

            let master_playlist = generate_hls_master_playlist();
            if let Err(e) = tokio::fs::write(output_dir.join("master.m3u8"), &master_playlist).await {
                tracing::error!(media_id = %media_id, "failed to write HLS master playlist: {e}");
                update_catalog_transcode_status(
                    &client, &catalog_url, &media_id,
                    TranscodeStatus::Failed, Some(transcode_format), Some(e.to_string()),
                ).await;
                return;
            }
        }
        "dash" => {
            for variant in VARIANTS {
                tracing::info!(media_id = %media_id, variant = variant.name, "transcoding DASH variant");
                if let Err(e) = transcode_dash_variant(&input_path, &output_dir, variant).await {
                    tracing::error!(media_id = %media_id, variant = variant.name, "DASH transcode failed: {e}");
                    update_catalog_transcode_status(
                        &client, &catalog_url, &media_id,
                        TranscodeStatus::Failed, Some(transcode_format), Some(e),
                    ).await;
                    return;
                }
            }

            if let Err(e) = generate_dash_manifest(&output_dir).await {
                tracing::error!(media_id = %media_id, "DASH manifest generation failed: {e}");
                update_catalog_transcode_status(
                    &client, &catalog_url, &media_id,
                    TranscodeStatus::Failed, Some(transcode_format), Some(e),
                ).await;
                return;
            }
        }
        other => {
            tracing::warn!(media_id = %media_id, format = other, "unknown transcode format, skipping");
            return;
        }
    }

    tracing::info!(media_id = %media_id, format = %transcode_format, "transcode complete");
    update_catalog_transcode_status(
        &client, &catalog_url, &media_id,
        TranscodeStatus::Ready,
        Some(transcode_format),
        None,
    ).await;
}
