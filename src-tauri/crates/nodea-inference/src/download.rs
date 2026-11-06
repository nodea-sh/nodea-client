//! Downloading a model file into the cache directory.

use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

#[derive(Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percentage: f32,
    pub status: String,
}

// `LoadModel.model_name` comes from the backend and ends up in
// `cache_dir.join(model_name)`. Unchecked, a name like
// `../../Library/LaunchAgents/x.plist` (or an absolute path) would write a
// download anywhere the user can write, so it is held to the shape a real
// registry entry has: a plain `*.gguf` file name.

/// One path segment of plain ASCII, never `.` or `..`.
pub fn is_safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Ok for a plain GGUF file name that stays inside the cache directory.
pub fn validate_model_file_name(name: &str) -> Result<(), String> {
    let ok = is_safe_segment(name)
        && !name.starts_with('.')
        && name.to_ascii_lowercase().ends_with(".gguf");
    if ok {
        Ok(())
    } else {
        Err(format!(
            "Refusing model file name {:?}: not a plain .gguf file name",
            name
        ))
    }
}

/// Timeouts for a model download.
///
/// A single whole-request timeout used to cap the entire multi-GB download at
/// 5 minutes, so any connection slower than ~15 MB/s failed partway through.
/// `idle` catches a stalled connection without penalising a slow but steady
/// one; `total` is only a backstop.
#[derive(Clone, Copy, Debug)]
pub struct DownloadTimeouts {
    pub connect: Duration,
    /// Max wait for the response headers or for the next body chunk.
    pub idle: Duration,
    pub total: Duration,
}

impl Default for DownloadTimeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(30),
            idle: Duration::from_secs(60),
            total: Duration::from_secs(900),
        }
    }
}

/// Download `url` to `dest_path` via a sibling `.tmp` file, renamed into place
/// only once the whole body has arrived.
async fn download_to_path<F>(
    url: &str,
    dest_path: &Path,
    timeouts: DownloadTimeouts,
    progress_callback: F,
) -> Result<(), String>
where
    F: Fn(DownloadProgress),
{
    tokio::time::timeout(
        timeouts.total,
        fetch_to_path(url, dest_path, timeouts, progress_callback),
    )
    .await
    .map_err(|_| format!("Download timed out after {}s", timeouts.total.as_secs()))?
}

async fn fetch_to_path<F>(
    url: &str,
    dest_path: &Path,
    timeouts: DownloadTimeouts,
    progress_callback: F,
) -> Result<(), String>
where
    F: Fn(DownloadProgress),
{
    progress_callback(DownloadProgress {
        downloaded_bytes: 0,
        total_bytes: 0,
        percentage: 0.0,
        status: "Starting download...".to_string(),
    });

    // reqwest 0.11 has no read timeout, so the idle timeout wraps each await below.
    let client = reqwest::Client::builder()
        .connect_timeout(timeouts.connect)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let stalled = || format!("Download stalled: no data for {}s", timeouts.idle.as_secs());

    let response = tokio::time::timeout(timeouts.idle, client.get(url).send())
        .await
        .map_err(|_| stalled())?
        .map_err(|e| format!("Failed to start download: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    println!(
        "Total download size: {} bytes ({:.2} GB)",
        total_bytes,
        total_bytes as f64 / 1_073_741_824.0
    );

    let temp_path = dest_path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded_bytes = 0u64;
    let mut stream = response.bytes_stream();
    let mut last_update = Instant::now();

    // Download chunks and report progress (throttled to once per second)
    while let Some(chunk_result) = tokio::time::timeout(timeouts.idle, stream.next())
        .await
        .map_err(|_| stalled())?
    {
        let chunk = chunk_result.map_err(|e| format!("Download error: {}", e))?;

        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write to file: {}", e))?;

        downloaded_bytes += chunk.len() as u64;

        // Only send progress updates once per second
        let now = Instant::now();
        if now.duration_since(last_update) >= Duration::from_secs(1) {
            let percentage = if total_bytes > 0 {
                (downloaded_bytes as f64 / total_bytes as f64 * 100.0) as f32
            } else {
                0.0
            };

            progress_callback(DownloadProgress {
                downloaded_bytes,
                total_bytes,
                percentage,
                status: format!(
                    "Downloading: {:.2} GB / {:.2} GB",
                    downloaded_bytes as f64 / 1_073_741_824.0,
                    total_bytes as f64 / 1_073_741_824.0
                ),
            });

            last_update = now;
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush file: {}", e))?;
    drop(file);

    tokio::fs::rename(&temp_path, dest_path)
        .await
        .map_err(|e| format!("Failed to finalize file: {}", e))?;

    progress_callback(DownloadProgress {
        downloaded_bytes: total_bytes,
        total_bytes,
        percentage: 100.0,
        status: "Download complete".to_string(),
    });
    Ok(())
}

/// Download a model into `cache_dir`, unless it is already there.
///
/// A free function so it needs no ModelManager - and therefore no
/// model_manager lock. A node serves generations under that lock, so a
/// download that held it would take the node out of service for minutes.
pub async fn download_model_to<F>(
    cache_dir: &Path,
    url: &str,
    model_name: &str,
    timeouts: DownloadTimeouts,
    progress_callback: F,
) -> Result<PathBuf, String>
where
    F: Fn(DownloadProgress) + Send + 'static,
{
    validate_model_file_name(model_name)?;
    let dest_path = cache_dir.join(model_name);

    // Check if already exists
    if dest_path.exists() {
        progress_callback(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: 0,
            percentage: 100.0,
            status: "Model already cached".to_string(),
        });
        return Ok(dest_path);
    }

    println!("Downloading model from: {}", url);
    download_to_path(url, &dest_path, timeouts, progress_callback).await?;
    println!("Download complete: {:?}", dest_path);
    Ok(dest_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn download_model_to_skips_a_file_already_cached() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.gguf"), b"x").unwrap();

        let path = download_model_to(
            dir.path(),
            "http://127.0.0.1:9/unreachable",
            "model.gguf",
            DownloadTimeouts::default(),
            |_| {},
        )
        .await
        .unwrap();

        assert_eq!(path, dir.path().join("model.gguf"));
    }
}
