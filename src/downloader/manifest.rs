//! Visual Studio manifest parsing utilities
//!
//! Responsible for downloading both the channel manifest and the actual
//! Visual Studio package manifest (vsman), exposing helpers to look up MSVC
//! toolset and Windows SDK packages.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use super::cache::{
    create_spinner, default_manifest_cache_dir, fetch_bytes_with_cache, url_basename,
};
use crate::constants::{USER_AGENT, VS_CHANNEL_URL};
use crate::error::{MsvcKitError, Result};

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
            .user_agent(USER_AGENT)
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
            .filter_map(|pkg| pkg.id.split('_').nth(1).and_then(normalize_sdk_version))
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
            .filter_map(|pkg| pkg.id.split('_').nth(1).and_then(normalize_sdk_version))
            .collect();

        versions.sort();
        versions.dedup();
        versions
    }

    /// Resolve a partial MSVC version prefix to a full version
    ///
    /// For example, "14.44" might resolve to "14.44.33807"
    ///
    /// # Arguments
    /// * `prefix` - Version prefix to resolve (e.g., "14.44" or "14")
    ///
    /// # Returns
    /// The full version string if found, None otherwise
    pub fn resolve_msvc_version(&self, prefix: &str) -> Option<String> {
        // First, try to find an exact match in the tools packages
        let mut matching_versions: Vec<String> = self
            .packages
            .iter()
            .filter(|pkg| {
                pkg.id.starts_with("Microsoft.VC.")
                    && pkg.id.contains("Tools")
                    && pkg.id.contains(&format!(".{}.", prefix))
            })
            .map(|pkg| pkg.version.clone())
            .collect();

        matching_versions.sort();
        matching_versions.dedup();

        // Return the latest matching version
        matching_versions.last().cloned()
    }

    /// Resolve a partial SDK version to a full version
    ///
    /// For example, "26100" might resolve to "10.0.26100.0"
    ///
    /// # Arguments
    /// * `prefix` - Version prefix or build number to resolve
    ///
    /// # Returns
    /// The full version string if found, None otherwise
    pub fn resolve_sdk_version(&self, prefix: &str) -> Option<String> {
        let versions = self.list_sdk_versions();

        // Try exact match first
        if versions.contains(&prefix.to_string()) {
            return Some(prefix.to_string());
        }

        // Try to match by build number
        versions.into_iter().find(|v| {
            v.contains(prefix) || v.split('.').nth(2).map(|b| b == prefix).unwrap_or(false)
        })
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
