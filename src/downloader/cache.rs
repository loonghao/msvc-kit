//! Manifest cache management
//!
//! Provides caching utilities for VS manifests using ETag/Last-Modified
//! and fingerprint-based validation.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::progress as progress_const;
use crate::error::{MsvcKitError, Result};

/// Metadata for cached manifest files
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestCacheMeta {
    /// Original URL of the manifest
    pub url: String,
    /// A human-friendly name used to build the fingerprint (e.g., file name)
    #[serde(default)]
    pub name: Option<String>,
    /// Cached body size
    #[serde(default)]
    pub size: Option<u64>,
    /// Fingerprint built from (name + size)
    /// Note: size match alone is best-effort, not cryptographically strong
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// ETag header value for conditional requests
    #[serde(default)]
    pub etag: Option<String>,
    /// Last-Modified header value for conditional requests
    #[serde(default)]
    pub last_modified: Option<String>,
}

/// Compute a fingerprint from name and size
///
/// Note: This is a best-effort fast skip mechanism. Size match alone
/// does not guarantee content identity.
pub fn compute_fingerprint(name: &str, size: u64) -> String {
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    h.update(b"|");
    h.update(size.to_le_bytes());
    hex::encode(h.finalize())
}

/// Get the default manifest cache directory
pub fn default_manifest_cache_dir() -> PathBuf {
    if let Some(proj) = directories::ProjectDirs::from("com", "loonghao", "msvc-kit") {
        proj.cache_dir().join("manifests")
    } else {
        std::env::temp_dir().join("msvc-kit").join("manifests")
    }
}

/// Get the metadata file path for a cache file
pub fn meta_path_for(cache_file: &Path) -> PathBuf {
    let name = cache_file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("manifest");
    cache_file.with_file_name(format!("{}.meta.json", name))
}

/// Read cache metadata from disk
pub async fn read_meta(path: &Path) -> Option<ManifestCacheMeta> {
    let data = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice(&data).ok()
}

/// Write cache metadata to disk
pub async fn write_meta(path: &Path, meta: &ManifestCacheMeta) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = serde_json::to_vec_pretty(meta)?;
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

/// Create a spinner progress bar with consistent style
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(progress_const::SPINNER_TICK_MS));
    pb
}

/// Extract basename from URL (removing query string and fragment)
pub fn url_basename(url: &str) -> String {
    let mut s = url;
    if let Some((left, _)) = s.split_once('#') {
        s = left;
    }
    if let Some((left, _)) = s.split_once('?') {
        s = left;
    }
    let name = s.rsplit('/').next().unwrap_or(s).trim();
    if name.is_empty() {
        url.to_string()
    } else {
        name.to_string()
    }
}

/// Fetch bytes from URL with caching support
///
/// Uses ETag/Last-Modified for conditional requests and fingerprint-based
/// validation as a fast path.
///
/// # Arguments
///
/// * `client` - HTTP client to use
/// * `url` - URL to fetch
/// * `cache_file` - Path to cache the response
/// * `spinner` - Progress spinner for UI feedback
/// * `label` - Label for progress messages
/// * `fingerprint_name` - Name to use for fingerprint computation
///
/// # Returns
///
/// Tuple of (bytes, was_cached) where was_cached indicates if the response
/// came from cache.
pub async fn fetch_bytes_with_cache(
    client: &reqwest::Client,
    url: &str,
    cache_file: &Path,
    spinner: &ProgressBar,
    label: &str,
    fingerprint_name: &str,
) -> Result<(Vec<u8>, bool)> {
    if let Some(parent) = cache_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let meta_path = meta_path_for(cache_file);
    let cached_bytes = tokio::fs::read(cache_file).await.ok();
    let meta = read_meta(&meta_path).await;

    // Fast path: if we already have a cached body, try a cheap HEAD and compare size.
    // This follows the "file name + size" fingerprint idea (best-effort; not cryptographically strong).
    if let Some(ref cached) = cached_bytes {
        let cached_len = cached.len() as u64;
        if let Ok(head) = client.head(url).send().await {
            if head.status().is_success() {
                if let Some(remote_len) = head.content_length() {
                    if remote_len == cached_len {
                        let fp = compute_fingerprint(fingerprint_name, remote_len);
                        // If meta exists and matches, great; if not, we still accept size match and refresh meta.
                        let ok = meta
                            .as_ref()
                            .map(|m| m.url == url && m.fingerprint.as_deref() == Some(fp.as_str()))
                            .unwrap_or(true);
                        if ok {
                            spinner.set_message(format!("{} (cached, size match)", label));
                            let new_meta = ManifestCacheMeta {
                                url: url.to_string(),
                                name: Some(fingerprint_name.to_string()),
                                size: Some(remote_len),
                                fingerprint: Some(fp),
                                etag: meta.as_ref().and_then(|m| m.etag.clone()),
                                last_modified: meta.as_ref().and_then(|m| m.last_modified.clone()),
                            };
                            let _ = write_meta(&meta_path, &new_meta).await;
                            return Ok((cached.clone(), true));
                        }
                    }
                }
            }
        }
    }

    // Conditional request: prefer ETag/Last-Modified if we have it.
    if let (Some(meta), Some(cached)) = (meta, cached_bytes.clone()) {
        if meta.url == url {
            let mut req = client.get(url);
            if let Some(ref etag) = meta.etag {
                req = req.header(IF_NONE_MATCH, etag);
            }
            if let Some(ref lm) = meta.last_modified {
                req = req.header(IF_MODIFIED_SINCE, lm);
            }

            let resp = req.send().await?;
            if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
                spinner.set_message(format!("{} (cached)", label));
                return Ok((cached, true));
            }

            if resp.status().is_success() {
                let headers = resp.headers().clone();
                let bytes = download_response_bytes_with_progress(resp, spinner, label).await?;

                tokio::fs::write(cache_file, &bytes).await?;
                let size = bytes.len() as u64;
                let meta = ManifestCacheMeta {
                    url: url.to_string(),
                    name: Some(fingerprint_name.to_string()),
                    size: Some(size),
                    fingerprint: Some(compute_fingerprint(fingerprint_name, size)),
                    etag: headers
                        .get(ETAG)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string()),
                    last_modified: headers
                        .get(LAST_MODIFIED)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string()),
                };
                let _ = write_meta(&meta_path, &meta).await;

                return Ok((bytes, false));
            }

            return Err(MsvcKitError::Other(format!(
                "Failed to fetch {}: HTTP {}",
                url,
                resp.status()
            )));
        }
    }

    // No usable cache: fetch fully
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(MsvcKitError::Other(format!(
            "Failed to fetch {}: HTTP {}",
            url,
            resp.status()
        )));
    }

    let headers = resp.headers().clone();
    let bytes = download_response_bytes_with_progress(resp, spinner, label).await?;
    tokio::fs::write(cache_file, &bytes).await?;

    let size = bytes.len() as u64;
    let meta = ManifestCacheMeta {
        url: url.to_string(),
        name: Some(fingerprint_name.to_string()),
        size: Some(size),
        fingerprint: Some(compute_fingerprint(fingerprint_name, size)),
        etag: headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        last_modified: headers
            .get(LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
    };
    let _ = write_meta(&meta_path, &meta).await;

    Ok((bytes, false))
}

/// Download response bytes with progress updates
pub async fn download_response_bytes_with_progress(
    response: reqwest::Response,
    spinner: &ProgressBar,
    label: &str,
) -> Result<Vec<u8>> {
    let total = response.content_length();
    let mut buf = Vec::with_capacity(total.unwrap_or(0) as usize);

    let start = Instant::now();
    let mut downloaded: u64 = 0;
    let mut last_update = Instant::now();

    let mut stream = response.bytes_stream();
    while let Some(item) = stream.next().await {
        let chunk = item?;
        downloaded += chunk.len() as u64;
        buf.extend_from_slice(&chunk);

        if last_update.elapsed() >= Duration::from_millis(200) {
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let speed = (downloaded as f64 / elapsed) as u64;
            let speed_h = humansize::format_size(speed, humansize::BINARY);

            if let Some(total) = total {
                let pct = (downloaded as f64 * 100.0 / total as f64).clamp(0.0, 100.0);
                spinner.set_message(format!(
                    "{} {}/{} ({:.1}%) @ {}/s",
                    label,
                    humansize::format_size(downloaded, humansize::BINARY),
                    humansize::format_size(total, humansize::BINARY),
                    pct,
                    speed_h
                ));
            } else {
                spinner.set_message(format!(
                    "{} {} @ {}/s",
                    label,
                    humansize::format_size(downloaded, humansize::BINARY),
                    speed_h
                ));
            }

            last_update = Instant::now();
        }
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_basename() {
        assert_eq!(
            url_basename("https://example.com/path/file.json"),
            "file.json"
        );
        assert_eq!(
            url_basename("https://example.com/path/file.json?query=1"),
            "file.json"
        );
        assert_eq!(
            url_basename("https://example.com/path/file.json#fragment"),
            "file.json"
        );
        assert_eq!(url_basename("https://example.com/"), "https://example.com/");
    }

    #[test]
    fn test_compute_fingerprint() {
        let fp1 = compute_fingerprint("file.json", 1024);
        let fp2 = compute_fingerprint("file.json", 1024);
        let fp3 = compute_fingerprint("file.json", 2048);

        assert_eq!(fp1, fp2);
        assert_ne!(fp1, fp3);
    }

    #[test]
    fn test_meta_path_for() {
        let cache_file = PathBuf::from("/cache/manifest.json");
        let meta_path = meta_path_for(&cache_file);
        assert_eq!(meta_path, PathBuf::from("/cache/manifest.json.meta.json"));
    }
}
