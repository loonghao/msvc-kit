//! Manifest cache management
//!
//! Provides caching utilities for VS manifests using ETag/Last-Modified
//! and digest validation of cached bytes.

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
    /// Digest of cached bytes, checked before a conditional request can reuse them.
    #[serde(default)]
    pub sha256: Option<String>,
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
///
/// Mirrors [`crate::config::default_cache_root`] so a configuration that never
/// set `cache_dir` keeps caching manifests where it always has.
pub fn default_manifest_cache_dir() -> PathBuf {
    crate::config::default_cache_root().join("manifests")
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
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || crate::storage::atomic_write(&path, &bytes))
        .await
        .map_err(|error| MsvcKitError::Other(error.to_string()))??;
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
/// Verifies cached bytes and asks the server using ETag/Last-Modified validators.
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
    // A channel has no independently supplied digest. Fetch its current body;
    // local metadata and a server's 304 cannot authenticate cached contents.
    fetch_verified_bytes_with_cache(
        client,
        url,
        cache_file,
        spinner,
        label,
        fingerprint_name,
        None,
    )
    .await
}

/// Cache a manifest only when its bytes match a digest from a fresh channel.
/// Without an external digest, always fetch a full response and never fall back
/// to cached contents after a transport failure.
pub async fn fetch_verified_bytes_with_cache(
    client: &reqwest::Client,
    url: &str,
    cache_file: &Path,
    spinner: &ProgressBar,
    label: &str,
    fingerprint_name: &str,
    expected_sha256: Option<&str>,
) -> Result<(Vec<u8>, bool)> {
    if expected_sha256.is_some_and(|digest| {
        digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err(MsvcKitError::Config(
            "Invalid official manifest SHA256".into(),
        ));
    }
    if let Some(parent) = cache_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let lock_path = cache_file.with_file_name(format!(
        "{}.lock",
        cache_file.file_name().unwrap_or_default().to_string_lossy()
    ));
    let _cache_lock = tokio::task::spawn_blocking(move || crate::storage::lock_file(&lock_path))
        .await
        .map_err(|error| MsvcKitError::Other(error.to_string()))??;
    let meta_path = meta_path_for(cache_file);
    let mut cached_bytes = tokio::fs::read(cache_file).await.ok();
    let meta = read_meta(&meta_path).await;

    // A size match cannot establish freshness. Validate cached bytes, then ask
    // the server using its validators; without validators fetch a fresh body.
    if let Some(cached) = &cached_bytes {
        let digest = super::hash::compute_hash(cached);
        let valid = meta.as_ref().is_some_and(|meta| {
            meta.url == url
                && meta.name.as_deref() == Some(fingerprint_name)
                && meta.size == Some(cached.len() as u64)
                && meta.sha256.as_deref() == Some(digest.as_str())
                && expected_sha256.is_some_and(|expected| digest.eq_ignore_ascii_case(expected))
                && (meta.etag.is_some() || meta.last_modified.is_some())
        });
        if !valid {
            cached_bytes = None;
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
                verify_manifest_digest(&bytes, expected_sha256, fingerprint_name)?;
                publish_cached_bytes(cache_file, &bytes).await?;
                let size = bytes.len() as u64;
                let meta = ManifestCacheMeta {
                    url: url.to_string(),
                    name: Some(fingerprint_name.to_string()),
                    size: Some(size),
                    fingerprint: Some(compute_fingerprint(fingerprint_name, size)),
                    sha256: Some(super::hash::compute_hash(&bytes)),
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

            return Err(http_status_error(url, resp.status()));
        }
    }

    // No usable cache: fetch fully
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(http_status_error(url, resp.status()));
    }

    let headers = resp.headers().clone();
    let bytes = download_response_bytes_with_progress(resp, spinner, label).await?;
    verify_manifest_digest(&bytes, expected_sha256, fingerprint_name)?;
    publish_cached_bytes(cache_file, &bytes).await?;

    let size = bytes.len() as u64;
    let meta = ManifestCacheMeta {
        url: url.to_string(),
        name: Some(fingerprint_name.to_string()),
        size: Some(size),
        fingerprint: Some(compute_fingerprint(fingerprint_name, size)),
        sha256: Some(super::hash::compute_hash(&bytes)),
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

fn verify_manifest_digest(bytes: &[u8], expected: Option<&str>, name: &str) -> Result<()> {
    if let Some(expected) = expected {
        let actual = super::hash::compute_hash(bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(MsvcKitError::HashMismatch {
                file: name.into(),
                expected: expected.into(),
                actual,
            });
        }
    }
    Ok(())
}

async fn publish_cached_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let path = path.to_path_buf();
    let bytes = bytes.to_vec();
    tokio::task::spawn_blocking(move || crate::storage::atomic_write(&path, &bytes))
        .await
        .map_err(|error| MsvcKitError::Other(error.to_string()))??;
    Ok(())
}

/// Build the error for a response msvc-kit cannot use
///
/// The status travels with the error so callers can classify it (a 404 means
/// something completely different from a 503) instead of parsing a message.
fn http_status_error(url: &str, status: reqwest::StatusCode) -> MsvcKitError {
    MsvcKitError::HttpStatus {
        url: url.to_string(),
        status: status.as_u16(),
    }
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

    #[tokio::test]
    async fn equal_length_manifests_still_check_server_freshness() {
        let mut server = mockito::Server::new_async().await;
        let first = server
            .mock("GET", "/manifest")
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_header("ETag", "version-1")
            .with_body("AAAA")
            .expect(1)
            .create_async()
            .await;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("manifest.json");
        let client = reqwest::Client::new();
        let url = format!("{}/manifest", server.url());
        let spinner = ProgressBar::hidden();
        let (first_body, _) =
            fetch_bytes_with_cache(&client, &url, &path, &spinner, "fixture", "manifest.json")
                .await
                .unwrap();
        first.assert_async().await;
        first.remove_async().await;
        let changed = server
            .mock("GET", "/manifest")
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_header("ETag", "version-2")
            .with_body("BBBB")
            .expect(1)
            .create_async()
            .await;
        let head = server
            .mock("HEAD", "/manifest")
            .with_header("content-length", "4")
            .expect(0)
            .create_async()
            .await;
        let (second_body, cached) =
            fetch_bytes_with_cache(&client, &url, &path, &spinner, "fixture", "manifest.json")
                .await
                .unwrap();
        assert_eq!(first_body, b"AAAA");
        assert_eq!(second_body, b"BBBB");
        assert!(!cached);
        changed.assert_async().await;
        head.assert_async().await;
    }

    #[tokio::test]
    async fn altered_manifest_cache_does_not_send_stale_validators() {
        let mut server = mockito::Server::new_async().await;
        let full = server
            .mock("GET", "/manifest")
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_header("ETag", "version-1")
            .with_body("AAAA")
            .expect(2)
            .create_async()
            .await;
        let conditional = server
            .mock("GET", "/manifest")
            .match_header("if-none-match", "version-1")
            .with_status(304)
            .expect(0)
            .create_async()
            .await;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("manifest.json");
        let client = reqwest::Client::new();
        let url = format!("{}/manifest", server.url());
        let spinner = ProgressBar::hidden();
        fetch_bytes_with_cache(&client, &url, &path, &spinner, "fixture", "manifest.json")
            .await
            .unwrap();
        std::fs::write(&path, b"CCCC").unwrap();
        let (body, cached) =
            fetch_bytes_with_cache(&client, &url, &path, &spinner, "fixture", "manifest.json")
                .await
                .unwrap();
        assert_eq!(body, b"AAAA");
        assert!(!cached);
        full.assert_async().await;
        conditional.assert_async().await;
    }
}
