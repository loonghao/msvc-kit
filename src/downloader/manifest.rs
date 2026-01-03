//! Visual Studio manifest parsing utilities
//!
//! Responsible for downloading both the channel manifest and the actual
//! Visual Studio package manifest (vsman), exposing helpers to look up MSVC
//! toolset and Windows SDK packages.

use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::{MsvcKitError, Result};

/// Visual Studio 2022 channel manifest URL
pub const VS_CHANNEL_URL: &str = "https://aka.ms/vs/17/release/channel";

/// Channel manifest structure (top-level)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelManifest {
    /// Manifest version
    pub manifest_version: String,

    /// Additional info
    #[serde(default)]
    pub info: Option<ChannelInfo>,

    /// Items in channel (products, manifests, bootstrappers)
    #[serde(default)]
    pub channel_items: Vec<ChannelItem>,
}

/// Channel info metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    #[serde(default)]
    pub product_display_version: Option<String>,
    #[serde(default)]
    pub build_version: Option<String>,
}

/// Channel item entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelItem {
    pub id: String,
    pub version: Option<String>,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(default)]
    pub payloads: Vec<Payload>,
}

/// Visual Studio package manifest (.vsman) structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VsManifest {
    pub manifest_version: String,
    #[serde(default)]
    pub engine_version: Option<String>,
    #[serde(default)]
    pub packages: Vec<VsPackage>,
}

/// Package entry in vsman
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VsPackage {
    pub id: String,
    pub version: String,
    #[serde(rename = "type")]
    pub package_type: String,
    #[serde(default)]
    pub chip: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub payloads: Vec<Payload>,
    #[serde(default)]
    pub dependencies: HashMap<String, Value>,
    #[serde(default)]
    pub machine_arch: Option<String>,
    #[serde(default)]
    pub product_arch: Option<String>,
}

/// Payload information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub file_name: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

/// Simplified package info returned to downloaders
#[derive(Debug, Clone)]
pub struct Package {
    pub id: String,
    pub version: String,
    pub package_type: String,
    pub chip: Option<String>,
    pub payloads: Vec<PackagePayload>,
    pub total_size: u64,
}

/// Payload ready for download
#[derive(Debug, Clone)]
pub struct PackagePayload {
    pub file_name: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

/// Create a spinner progress bar with consistent style
fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

fn url_basename(url: &str) -> String {
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ManifestCacheMeta {
    url: String,
    /// A human-friendly name we used to build the fingerprint (e.g. file name)
    #[serde(default)]
    name: Option<String>,
    /// Cached body size
    #[serde(default)]
    size: Option<u64>,
    /// Fingerprint built from (name + size)
    #[serde(default)]
    fingerprint: Option<String>,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    last_modified: Option<String>,
}

fn compute_fingerprint(name: &str, size: u64) -> String {
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    h.update(b"|");
    h.update(size.to_le_bytes());
    hex::encode(h.finalize())
}

fn default_manifest_cache_dir() -> PathBuf {
    if let Some(proj) = directories::ProjectDirs::from("com", "loonghao", "msvc-kit") {
        proj.cache_dir().join("manifests")
    } else {
        std::env::temp_dir().join("msvc-kit").join("manifests")
    }
}

fn meta_path_for(cache_file: &Path) -> PathBuf {
    let name = cache_file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("manifest");
    cache_file
        .with_file_name(format!("{}.meta.json", name))
}

async fn read_meta(path: &Path) -> Option<ManifestCacheMeta> {
    let data = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice(&data).ok()
}

async fn write_meta(path: &Path, meta: &ManifestCacheMeta) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = serde_json::to_vec_pretty(meta)?;
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

async fn fetch_bytes_with_cache(
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

async fn download_response_bytes_with_progress(
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

impl VsManifest {
    /// Fetch and parse the latest VS manifest (cached).
    ///
    /// The cache is stored under the OS-specific cache directory.
    pub async fn fetch() -> Result<Self> {
        let cache_dir = default_manifest_cache_dir();
        Self::fetch_with_cache_dir(&cache_dir).await
    }

    /// Fetch and parse the latest VS manifest using a specific cache directory.
    pub async fn fetch_with_cache_dir(cache_dir: &Path) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("msvc-kit/0.1.0")
            .build()
            .map_err(|e| MsvcKitError::Other(format!("Failed to create HTTP client: {}", e)))?;

        // Step 1: Fetch channel manifest (cached)
        let channel_name = url_basename(VS_CHANNEL_URL);
        let spinner = create_spinner(&format!("Fetching channel manifest: {}", channel_name));
        tracing::debug!("Fetching channel manifest from {}", VS_CHANNEL_URL);

        let channel_cache = cache_dir.join("channel.json");
        let (channel_bytes, channel_cached) = fetch_bytes_with_cache(
            &client,
            VS_CHANNEL_URL,
            &channel_cache,
            &spinner,
            &format!("Downloading channel manifest: {}", channel_name),
            &channel_name,
        )
        .await?;

        if channel_cached {
            tracing::debug!("Using cached channel manifest from {:?}", channel_cache);
        }

        spinner.set_message(format!("Parsing channel manifest: {}", channel_name));
        let channel_manifest: ChannelManifest = serde_json::from_slice(&channel_bytes)?;

        // Show channel info if available
        if let Some(ref info) = channel_manifest.info {
            if let Some(ref version) = info.product_display_version {
                spinner.set_message(format!("Found Visual Studio {} channel", version));
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }

        let manifest_item = channel_manifest
            .channel_items
            .iter()
            .find(|item| item.id == "Microsoft.VisualStudio.Manifests.VisualStudio")
            .ok_or_else(|| {
                spinner.finish_and_clear();
                MsvcKitError::Other("Manifest entry missing in channel".to_string())
            })?;

        let manifest_url = manifest_item
            .payloads
            .first()
            .map(|p| p.url.clone())
            .ok_or_else(|| {
                spinner.finish_and_clear();
                MsvcKitError::Other("Manifest URL missing".to_string())
            })?;

        let manifest_file_name = manifest_item
            .payloads
            .first()
            .and_then(|p| {
                if p.file_name.trim().is_empty() {
                    None
                } else {
                    Some(p.file_name.clone())
                }
            })
            .unwrap_or_else(|| url_basename(&manifest_url));

        tracing::info!(
            "VS package manifest: {} ({})",
            manifest_file_name,
            manifest_url
        );

        // Step 2: Fetch the main VS manifest (cached)
        let vsman_cache = cache_dir.join("vsman").join(&manifest_file_name);
        let download_label = format!("Downloading {}:", manifest_file_name);
        spinner.set_message(format!(
            "Downloading package manifest: {} (this may take a moment)...",
            manifest_file_name
        ));

        let (manifest_bytes, vsman_cached) = fetch_bytes_with_cache(
            &client,
            &manifest_url,
            &vsman_cache,
            &spinner,
            &download_label,
            &manifest_file_name,
        )
        .await?;

        if vsman_cached {
            tracing::info!("Using cached VS package manifest: {:?}", vsman_cache);
        }

        // Step 3: Parse the manifest (can take a while)
        let manifest_size = manifest_bytes.len() as u64;
        let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();
        let parsing_spinner = spinner.clone();
        tokio::spawn(async move {
            let start = Instant::now();
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(250)) => {
                        parsing_spinner.set_message(format!(
                            "Parsing package manifest ({})... {}s",
                            humansize::format_size(manifest_size, humansize::BINARY),
                            start.elapsed().as_secs()
                        ));
                    }
                    _ = &mut done_rx => {
                        break;
                    }
                }
            }
        });

        let manifest: VsManifest = tokio::task::spawn_blocking(move || {
            // Use simd-json for faster parsing (2-5x faster than serde_json)
            let mut bytes = manifest_bytes;
            simd_json::from_slice(&mut bytes)
        })
            .await
            .map_err(|e| MsvcKitError::Other(format!("Failed to join parsing task: {}", e)))??;

        let _ = done_tx.send(());

        spinner.finish_with_message(format!(
            "✓ Loaded manifest with {} packages",
            manifest.packages.len()
        ));

        tracing::info!(
            "Loaded VS manifest with {} packages",
            manifest.packages.len()
        );
        Ok(manifest)
    }

    /// Get latest MSVC toolset version prefix (e.g. "14.42")
    pub fn get_latest_msvc_version(&self) -> Option<String> {
        let mut versions: Vec<String> = self
            .packages
            .iter()
            .filter(|pkg| pkg.id.starts_with("Microsoft.VC.") && pkg.id.contains("Tools"))
            .filter_map(|pkg| {
                let parts: Vec<&str> = pkg.id.split('.').collect();
                if parts.len() >= 4 {
                    Some(format!("{}.{}", parts[2], parts[3]))
                } else {
                    None
                }
            })
            .collect();

        versions.sort();
        versions.dedup();
        versions.last().cloned()
    }

    /// Get latest Windows SDK version (e.g. "10.0.26100.0")
    pub fn get_latest_sdk_version(&self) -> Option<String> {
        let mut versions: Vec<String> = self
            .packages
            .iter()
            .filter(|pkg| pkg.id.starts_with("Win10SDK_") || pkg.id.starts_with("Win11SDK_"))
            .filter_map(|pkg| {
                pkg.id
                    .split('_')
                    .nth(1)
                    .and_then(|token| normalize_sdk_version(token))
            })
            .collect();

        versions.sort();
        versions.dedup();
        versions.last().cloned()
    }

    /// Find MSVC packages (tools, CRT, ATL, MFC) for target architecture
    pub fn find_msvc_packages(
        &self,
        version_prefix: &str,
        host_arch: &str,
        target_arch: &str,
    ) -> Vec<Package> {
        let version_prefix = format!("Microsoft.VC.{}.", version_prefix);
        let host = host_arch.to_lowercase();
        let target = target_arch.to_lowercase();

        self.packages
            .iter()
            .filter(|pkg| {
                pkg.id
                    .to_lowercase()
                    .starts_with(&version_prefix.to_lowercase())
            })
            .filter(|pkg| {
                let id = pkg.id.to_lowercase();
                let is_tool = id.contains("tools")
                    && id.contains(&format!("host{}", host))
                    && id.contains(&format!("target{}", target));
                let is_crt = id.contains(".crt.") || id.contains(".crt") || id.contains("ucrt");
                let is_runtime = id.contains(".mfc") || id.contains(".atl") || id.contains(".asan");
                is_tool || is_crt || is_runtime
            })
            .map(|pkg| self.vs_package_to_package(pkg))
            .collect()
    }

    /// Find Windows SDK packages matching version and architecture
    pub fn find_sdk_packages(&self, version: &str, target_arch: &str) -> Vec<Package> {
        let target = target_arch.to_lowercase();
        let build_number = version.split('.').nth(2).unwrap_or(version);

        self.packages
            .iter()
            .filter(|pkg| {
                let id = pkg.id.to_lowercase();
                (id.contains("win10sdk") || id.contains("win11sdk") || id.contains("windows sdk"))
                    && id.contains(build_number)
            })
            .filter(|pkg| {
                pkg.chip
                    .as_ref()
                    .map(|chip| {
                        let chip = chip.to_lowercase();
                        chip == target || chip == "neutral" || (chip == "x86" && target == "x64")
                    })
                    .unwrap_or(true)
            })
            .map(|pkg| self.vs_package_to_package(pkg))
            .collect()
    }

    /// List all available MSVC version prefixes
    pub fn list_msvc_versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = self
            .packages
            .iter()
            .filter(|pkg| pkg.id.starts_with("Microsoft.VC.") && pkg.id.contains("Tools"))
            .filter_map(|pkg| {
                let parts: Vec<&str> = pkg.id.split('.').collect();
                if parts.len() >= 4 {
                    Some(format!("{}.{}", parts[2], parts[3]))
                } else {
                    None
                }
            })
            .collect();

        versions.sort();
        versions.dedup();
        versions
    }

    /// List all available SDK versions
    pub fn list_sdk_versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = self
            .packages
            .iter()
            .filter(|pkg| pkg.id.starts_with("Win10SDK_") || pkg.id.starts_with("Win11SDK_"))
            .filter_map(|pkg| {
                pkg.id
                    .split('_')
                    .nth(1)
                    .and_then(|token| normalize_sdk_version(token))
            })
            .collect();

        versions.sort();
        versions.dedup();
        versions
    }

    fn vs_package_to_package(&self, pkg: &VsPackage) -> Package {
        let payloads: Vec<PackagePayload> = pkg
            .payloads
            .iter()
            .map(|p| PackagePayload {
                file_name: p.file_name.clone(),
                url: p.url.clone(),
                size: p.size.unwrap_or(0),
                sha256: p.sha256.clone(),
            })
            .collect();

        let total_size = payloads.iter().map(|p| p.size).sum();

        Package {
            id: pkg.id.clone(),
            version: pkg.version.clone(),
            package_type: pkg.package_type.clone(),
            chip: pkg.chip.clone(),
            payloads,
            total_size,
        }
    }
}

fn normalize_sdk_version(token: &str) -> Option<String> {
    let starts_with_digit = token
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false);

    if !starts_with_digit {
        return None;
    }

    Some(if token.ends_with(".0") {
        token.to_string()
    } else {
        format!("{}.0", token)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_payload_basic() {
        let payload = PackagePayload {
            file_name: "test.vsix".into(),
            url: "https://example.com/test.vsix".into(),
            size: 2048,
            sha256: Some("abc123".into()),
        };

        assert_eq!(payload.file_name, "test.vsix");
        assert_eq!(payload.size, 2048);
    }
}
