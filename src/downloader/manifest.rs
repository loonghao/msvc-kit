//! Visual Studio manifest parsing utilities
//!
//! Responsible for downloading both the channel manifest and the actual
//! Visual Studio package manifest (vsman), exposing helpers to look up MSVC
//! toolset and Windows SDK packages.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use super::cache::{
    create_spinner, default_manifest_cache_dir, fetch_bytes_with_cache, meta_path_for, url_basename,
};
use super::MsvcComponent;
use crate::constants::USER_AGENT;
use crate::error::{MsvcKitError, Result};
use crate::vs_channel::{self, VsChannelSelection, VsChannelSpec};

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
    /// Fetch and parse the VS manifest from the newest available channel (cached).
    ///
    /// The cache is stored under the OS-specific cache directory.
    ///
    /// Channels are tried newest first; a channel whose manifest has not been
    /// published upstream yet is skipped instead of failing the whole request.
    /// Use [`VsManifest::fetch_with_selection`] to pin a channel.
    pub async fn fetch() -> Result<Self> {
        let cache_dir = default_manifest_cache_dir();
        Self::fetch_with_cache_dir(&cache_dir).await
    }

    /// Fetch and parse the VS manifest using a specific cache directory.
    ///
    /// Uses the newest available channel, see [`VsManifest::fetch`].
    pub async fn fetch_with_cache_dir(cache_dir: &Path) -> Result<Self> {
        Ok(
            Self::fetch_with_selection(VsChannelSelection::Auto, cache_dir)
                .await?
                .0,
        )
    }

    /// Fetch the VS manifest from one specific channel, without any fallback.
    ///
    /// Returns [`MsvcKitError::ChannelUnavailable`] when the channel does not
    /// serve a usable manifest (for example because upstream has not published
    /// it yet) instead of failing with an opaque parse error.
    pub async fn fetch_with_channel(channel: &VsChannelSpec, cache_dir: &Path) -> Result<Self> {
        Self::fetch_channel(channel, cache_dir).await
    }

    /// Fetch the VS manifest for a channel selection.
    ///
    /// * [`VsChannelSelection::Auto`] walks the channel registry newest first
    ///   and uses the first channel that serves a usable manifest, so adding a
    ///   new Visual Studio release to the registry is enough to pick it up once
    ///   upstream publishes it.
    /// * [`VsChannelSelection::Pinned`] uses exactly that channel.
    ///
    /// Returns the manifest together with the channel that served it.
    pub async fn fetch_with_selection(
        selection: VsChannelSelection,
        cache_dir: &Path,
    ) -> Result<(Self, VsChannelSpec)> {
        match selection {
            VsChannelSelection::Auto => Self::fetch_auto(cache_dir).await,
            VsChannelSelection::Pinned(spec) => {
                let manifest = Self::fetch_channel(&spec, cache_dir).await?;
                Ok((manifest, spec))
            }
        }
    }

    /// Try every known channel newest first until one serves a usable manifest
    async fn fetch_auto(cache_dir: &Path) -> Result<(Self, VsChannelSpec)> {
        Self::fetch_auto_from(&vs_channel::known_channels(), cache_dir).await
    }

    /// Try the given channels in order until one serves a usable manifest
    ///
    /// Channels that upstream has not published yet (HTML instead of JSON) are
    /// skipped with a warning instead of failing the whole request.
    async fn fetch_auto_from(
        candidates: &[VsChannelSpec],
        cache_dir: &Path,
    ) -> Result<(Self, VsChannelSpec)> {
        let mut skipped: Vec<String> = Vec::new();

        for candidate in candidates {
            match Self::fetch_channel(candidate, cache_dir).await {
                Ok(manifest) => {
                    if !skipped.is_empty() {
                        tracing::warn!(
                            "Visual Studio channel(s) skipped as unavailable: {}",
                            skipped.join("; ")
                        );
                    }
                    return Ok((manifest, candidate.clone()));
                }
                Err(err @ MsvcKitError::ChannelUnavailable { .. }) => {
                    tracing::debug!("Channel {} skipped: {}", candidate, err);
                    skipped.push(format!("{} ({})", candidate, err));
                }
                Err(err) => return Err(err),
            }
        }

        Err(MsvcKitError::ChannelUnavailable {
            channel: "auto-selection".to_string(),
            url: candidates
                .iter()
                .map(|c| c.channel_url.clone())
                .collect::<Vec<_>>()
                .join(", "),
            reason: if skipped.is_empty() {
                "no Visual Studio channel configured".to_string()
            } else {
                format!(
                    "no channel serves a usable manifest (attempts: {})",
                    skipped.join("; ")
                )
            },
        })
    }

    /// Fetch and parse the manifest of a single channel (cached)
    async fn fetch_channel(channel: &VsChannelSpec, cache_dir: &Path) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| MsvcKitError::Other(format!("Failed to create HTTP client: {}", e)))?;

        // Step 1: Fetch channel manifest (cached)
        let channel_label = format!("{} channel manifest", channel);
        let spinner = create_spinner(&format!("Fetching {}", channel_label));
        tracing::debug!("Fetching {} from {}", channel_label, channel.channel_url);

        let channel_cache = cache_dir.join(channel.channel_cache_file());
        let (channel_bytes, channel_cached) = fetch_bytes_with_cache(
            &client,
            &channel.channel_url,
            &channel_cache,
            &spinner,
            &format!("Downloading {}", channel_label),
            &channel.cache_slug(),
        )
        .await
        .map_err(|err| classify_channel_error(channel, err))?;

        if channel_cached {
            tracing::debug!("Using cached channel manifest from {:?}", channel_cache);
        }

        spinner.set_message(format!("Parsing {}", channel_label));
        let channel_manifest = match parse_channel_manifest(&channel_bytes) {
            Ok(manifest) => manifest,
            Err(reason) => {
                // Never keep a non-manifest body (an unpublished channel often
                // serves an HTML page) in the cache: it would mask the channel
                // once upstream publishes the real manifest.
                discard_cached_channel(&channel_cache).await;
                spinner.finish_and_clear();
                return Err(channel_unavailable(channel, reason));
            }
        };

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
                channel_unavailable(
                    channel,
                    "the channel manifest has no Microsoft.VisualStudio.Manifests.VisualStudio entry",
                )
            })?;

        let manifest_url = manifest_item
            .payloads
            .first()
            .map(|p| p.url.clone())
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| {
                spinner.finish_and_clear();
                channel_unavailable(channel, "the channel manifest has no manifest payload URL")
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

        if manifest.packages.is_empty() {
            spinner.finish_and_clear();
            return Err(channel_unavailable(
                channel,
                "the package manifest does not expose any packages",
            ));
        }

        spinner.finish_with_message(format!(
            "Loaded {} with {} packages",
            channel,
            manifest.packages.len()
        ));

        tracing::info!(
            "Loaded {} with {} packages",
            channel,
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
    ///
    /// This function filters packages based on the specified host and target architectures,
    /// optional component selection, and exclude patterns.
    ///
    /// By default, the standard toolchain (Tools, CRT, MFC, ATL) is included.
    /// Spectre-mitigated libraries are excluded unless explicitly requested via
    /// `include_components`.
    ///
    /// # Arguments
    /// * `version_prefix` - MSVC version prefix (e.g., "14.44")
    /// * `host_arch` - Host architecture (e.g., "x64")
    /// * `target_arch` - Target architecture (e.g., "x64")
    /// * `include_components` - Optional components to include (e.g., Spectre)
    /// * `exclude_patterns` - Package ID patterns to exclude
    pub fn find_msvc_packages(
        &self,
        version_prefix: &str,
        host_arch: &str,
        target_arch: &str,
        include_components: &HashSet<MsvcComponent>,
        exclude_patterns: &[String],
    ) -> Vec<Package> {
        let version_prefix = format!("Microsoft.VC.{}.", version_prefix);
        let host = host_arch.to_lowercase();
        let target = target_arch.to_lowercase();

        // Define all known architectures for exclusion filtering
        let all_archs = ["x64", "x86", "arm64", "arm"];

        self.packages
            .iter()
            .filter(|pkg| {
                pkg.id
                    .to_lowercase()
                    .starts_with(&version_prefix.to_lowercase())
            })
            .filter(|pkg| {
                let id = pkg.id.to_lowercase();

                // Apply user-defined exclude patterns
                for pattern in exclude_patterns {
                    if id.contains(&pattern.to_lowercase()) {
                        return false;
                    }
                }

                // Skip Spectre-mitigated libraries unless explicitly requested
                if id.contains(".spectre") && !include_components.contains(&MsvcComponent::Spectre)
                {
                    return false;
                }

                // Tool packages: must match both host and target architecture
                // e.g., Microsoft.VC.14.44.Tools.HostX64.TargetX64
                let is_tool = id.contains("tools")
                    && id.contains(&format!("host{}", host))
                    && id.contains(&format!("target{}", target));

                if is_tool {
                    return true;
                }

                // CRT packages: need architecture filtering
                // e.g., Microsoft.VC.14.44.CRT.x64.Desktop, Microsoft.VC.14.44.CRT.Headers
                let is_crt = id.contains(".crt.");

                // Runtime packages (MFC, ATL, ASAN): need architecture filtering
                // e.g., Microsoft.VC.14.44.MFC.x64, Microsoft.VC.14.44.ATL.x64
                let is_runtime = id.contains(".mfc") || id.contains(".atl") || id.contains(".asan");

                // Optional opt-in components (only included when explicitly requested)
                let is_cli =
                    id.contains(".cli") && include_components.contains(&MsvcComponent::Cli);
                let is_modules =
                    id.contains(".modules") && include_components.contains(&MsvcComponent::Modules);
                let is_redist =
                    id.contains(".redist") && include_components.contains(&MsvcComponent::Redist);

                let is_arch_filtered = is_crt || is_runtime || is_cli || is_modules || is_redist;

                if is_arch_filtered {
                    // Check if package ID contains architecture suffix
                    // Architecture-neutral packages (like CRT.Headers, CRT.Source) should be included
                    let has_arch_in_id = all_archs.iter().any(|arch| {
                        id.contains(&format!(".{}", arch))
                            || id.contains(&format!(".{}.desktop", arch))
                            || id.contains(&format!(".{}.store", arch))
                            || id.contains(&format!(".{}.uwp", arch))
                    });

                    if has_arch_in_id {
                        // Package has architecture in ID - must match target
                        let matches_target = id.contains(&format!(".{}", target))
                            || id.contains(&format!(".{}.desktop", target))
                            || id.contains(&format!(".{}.store", target))
                            || id.contains(&format!(".{}.uwp", target));
                        return matches_target;
                    }

                    // Also check chip field if present
                    if let Some(ref chip) = pkg.chip {
                        let chip_lower = chip.to_lowercase();
                        // Allow: matching target, neutral, or x86 when targeting x64 (for compatibility)
                        let chip_matches = chip_lower == target
                            || chip_lower == "neutral"
                            || (chip_lower == "x86" && target == "x64");
                        return chip_matches;
                    }

                    // Architecture-neutral package (e.g., CRT.Headers, CRT.Source)
                    return true;
                }

                // Check for custom component patterns
                for component in include_components {
                    if let MsvcComponent::Custom(pattern) = component {
                        if id.contains(&pattern.to_lowercase()) {
                            return true;
                        }
                    }
                }

                false
            })
            .map(|pkg| self.vs_package_to_package(pkg))
            .collect()
    }

    /// Find Windows SDK packages matching version and architecture
    ///
    /// This function filters SDK packages based on the specified target architecture.
    /// It uses both the `chip` field and package ID patterns to ensure only
    /// relevant architecture packages are downloaded.
    pub fn find_sdk_packages(&self, version: &str, target_arch: &str) -> Vec<Package> {
        let target = target_arch.to_lowercase();
        let build_number = version.split('.').nth(2).unwrap_or(version);

        // Define all known architectures for exclusion filtering
        let all_archs = ["x64", "x86", "arm64", "arm"];

        self.packages
            .iter()
            .filter(|pkg| {
                let id = pkg.id.to_lowercase();
                (id.contains("win10sdk") || id.contains("win11sdk") || id.contains("windows sdk"))
                    && id.contains(build_number)
            })
            .filter(|pkg| {
                let id = pkg.id.to_lowercase();

                // Check if package ID contains architecture suffix (e.g., _x64, _arm64)
                let has_arch_in_id = all_archs
                    .iter()
                    .any(|arch| id.contains(&format!("_{}", arch)));

                if has_arch_in_id {
                    // Package has architecture in ID - must match target
                    // Allow x86 packages when targeting x64 (needed for 32-bit compatibility)
                    let matches_target = id.contains(&format!("_{}", target))
                        || (target == "x64" && id.contains("_x86"));
                    if !matches_target {
                        return false;
                    }
                }

                // Check chip field for additional filtering
                pkg.chip
                    .as_ref()
                    .map(|chip| {
                        let chip = chip.to_lowercase();
                        // Allow: matching target, neutral, or x86 when targeting x64
                        chip == target || chip == "neutral" || (chip == "x86" && target == "x64")
                    })
                    .unwrap_or_else(|| {
                        // If no chip field, check if package ID has architecture info
                        // If ID also has no architecture, it's likely a neutral/common package
                        !has_arch_in_id
                    })
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

/// Parse a channel manifest body, reporting *why* it is unusable
///
/// Upstream serves an HTML page instead of JSON while a channel has not been
/// published yet; that must not surface as an opaque serde error.
fn parse_channel_manifest(bytes: &[u8]) -> std::result::Result<ChannelManifest, String> {
    let first = bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .copied();

    match first {
        Some(b'<') => Err(
            "the server returned an HTML page instead of a JSON manifest \
            (the channel is probably not published yet)"
                .to_string(),
        ),
        Some(b'{') => serde_json::from_slice(bytes)
            .map_err(|e| format!("the response is not a valid channel manifest: {}", e)),
        _ => Err("the server returned an empty or non-JSON response".to_string()),
    }
}

/// Remove a cached channel manifest that turned out to be unusable
async fn discard_cached_channel(cache_file: &Path) {
    let _ = tokio::fs::remove_file(cache_file).await;
    let _ = tokio::fs::remove_file(meta_path_for(cache_file)).await;
}

/// Build a [`MsvcKitError::ChannelUnavailable`] for a channel
fn channel_unavailable(channel: &VsChannelSpec, reason: impl Into<String>) -> MsvcKitError {
    MsvcKitError::ChannelUnavailable {
        channel: channel.display_name(),
        url: channel.channel_url.clone(),
        reason: reason.into(),
    }
}

/// Decide whether a channel fetch error means "this channel is unusable"
///
/// Transport and IO failures say nothing about the channel itself, so they stay
/// fatal. HTTP level problems (404, non-success status, ...) are reported as an
/// unavailable channel so auto-selection can fall back to an older release.
fn classify_channel_error(channel: &VsChannelSpec, err: MsvcKitError) -> MsvcKitError {
    match err {
        MsvcKitError::Network(_) | MsvcKitError::Io(_) => err,
        other => channel_unavailable(channel, other.to_string()),
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

    #[test]
    fn test_normalize_sdk_version() {
        // Normal version with trailing .0
        assert_eq!(
            normalize_sdk_version("10.0.26100.0"),
            Some("10.0.26100.0".to_string())
        );

        // Version without trailing .0 should get it appended
        assert_eq!(
            normalize_sdk_version("10.0.26100"),
            Some("10.0.26100.0".to_string())
        );

        // Non-version strings should return None
        assert_eq!(normalize_sdk_version("Headers"), None);
        assert_eq!(normalize_sdk_version("Desktop"), None);
    }

    /// Helper to create a mock VsManifest for testing
    fn create_test_manifest() -> VsManifest {
        VsManifest {
            manifest_version: "1.0".to_string(),
            engine_version: None,
            packages: vec![
                // MSVC Tools packages (simulate real package IDs)
                VsPackage {
                    id: "Microsoft.VC.14.44.Tools.HostX64.TargetX64.base".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // Tools for other architectures (should be filtered out for x64)
                VsPackage {
                    id: "Microsoft.VC.14.44.Tools.HostX64.TargetARM64.base".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("arm64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Microsoft.VC.14.44.Tools.HostX64.TargetX86.base".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x86".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // CRT Headers (architecture-neutral, should always be included)
                VsPackage {
                    id: "Microsoft.VC.14.44.CRT.Headers".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: None,
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // CRT with architecture suffix (should be filtered)
                VsPackage {
                    id: "Microsoft.VC.14.44.CRT.x64.Desktop".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Microsoft.VC.14.44.CRT.ARM64.Desktop".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("arm64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Microsoft.VC.14.44.CRT.x86.Desktop".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x86".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // MFC packages with architecture
                VsPackage {
                    id: "Microsoft.VC.14.44.MFC.x64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Microsoft.VC.14.44.MFC.ARM64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("arm64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // ATL packages with architecture
                VsPackage {
                    id: "Microsoft.VC.14.44.ATL.x64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Microsoft.VC.14.44.ATL.ARM64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("arm64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // Spectre-mitigated libraries (should be filtered out)
                VsPackage {
                    id: "Microsoft.VC.14.44.CRT.x64.Desktop.Spectre".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Microsoft.VC.14.44.MFC.x64.Spectre".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // C++/CLI support packages (opt-in only)
                VsPackage {
                    id: "Microsoft.VC.14.44.CLI.x64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Microsoft.VC.14.44.CLI.ARM64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("arm64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // C++ Modules packages (opt-in only)
                VsPackage {
                    id: "Microsoft.VC.14.44.Modules.x64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // C++ Redistributable packages (opt-in only)
                VsPackage {
                    id: "Microsoft.VC.14.44.Redist.x64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Microsoft.VC.14.44.Redist.ARM64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("arm64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // Older version tools
                VsPackage {
                    id: "Microsoft.VC.14.43.Tools.HostX64.TargetX64.base".to_string(),
                    version: "14.43.34607".to_string(),
                    package_type: "Vsix".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // SDK packages with different architectures
                VsPackage {
                    id: "Win11SDK_10.0.26100".to_string(),
                    version: "26100.1742".to_string(),
                    package_type: "Msi".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Win11SDK_10.0.26100_arm64".to_string(),
                    version: "26100.1742".to_string(),
                    package_type: "Msi".to_string(),
                    chip: Some("arm64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                VsPackage {
                    id: "Win10SDK_10.0.22621".to_string(),
                    version: "22621.3233".to_string(),
                    package_type: "Msi".to_string(),
                    chip: Some("x64".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
                // SDK neutral package (should always be included)
                VsPackage {
                    id: "Win11SDK_10.0.26100_Headers".to_string(),
                    version: "26100.1742".to_string(),
                    package_type: "Msi".to_string(),
                    chip: Some("neutral".to_string()),
                    language: None,
                    payloads: vec![],
                    dependencies: HashMap::new(),
                    machine_arch: None,
                    product_arch: None,
                },
            ],
        }
    }

    #[test]
    fn test_get_latest_msvc_version() {
        let manifest = create_test_manifest();
        let latest = manifest.get_latest_msvc_version();

        // Should return the short version prefix (14.44), not the full version
        assert_eq!(latest, Some("14.44".to_string()));
    }

    #[test]
    fn test_list_msvc_versions() {
        let manifest = create_test_manifest();
        let versions = manifest.list_msvc_versions();

        // Should contain both version prefixes
        assert!(versions.contains(&"14.44".to_string()));
        assert!(versions.contains(&"14.43".to_string()));
        // Should be sorted
        assert_eq!(versions.last(), Some(&"14.44".to_string()));
    }

    #[test]
    fn test_get_latest_sdk_version() {
        let manifest = create_test_manifest();
        let latest = manifest.get_latest_sdk_version();

        // Should return the full SDK version
        assert_eq!(latest, Some("10.0.26100.0".to_string()));
    }

    #[test]
    fn test_list_sdk_versions() {
        let manifest = create_test_manifest();
        let versions = manifest.list_sdk_versions();

        assert!(versions.contains(&"10.0.26100.0".to_string()));
        assert!(versions.contains(&"10.0.22621.0".to_string()));
    }

    #[test]
    fn test_resolve_msvc_version() {
        let manifest = create_test_manifest();

        // Should resolve short version to full version
        let resolved = manifest.resolve_msvc_version("14.44");
        assert_eq!(resolved, Some("14.44.34823".to_string()));

        // Should resolve older version
        let resolved_old = manifest.resolve_msvc_version("14.43");
        assert_eq!(resolved_old, Some("14.43.34607".to_string()));

        // Non-existent version should return None
        let not_found = manifest.resolve_msvc_version("14.99");
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_resolve_sdk_version() {
        let manifest = create_test_manifest();

        // Exact match
        let exact = manifest.resolve_sdk_version("10.0.26100.0");
        assert_eq!(exact, Some("10.0.26100.0".to_string()));

        // Match by build number
        let by_build = manifest.resolve_sdk_version("26100");
        assert_eq!(by_build, Some("10.0.26100.0".to_string()));

        // Non-existent version
        let not_found = manifest.resolve_sdk_version("99999");
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_find_msvc_packages() {
        let manifest = create_test_manifest();
        let empty_components = HashSet::new();
        let empty_patterns: Vec<String> = vec![];

        // Find packages for 14.44 x64
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &empty_components, &empty_patterns);

        // Should find the tools package
        assert!(!packages.is_empty());
        assert!(packages.iter().any(|p| p.id.contains("Tools")));
        assert!(packages.iter().any(|p| p.id.contains("CRT")));
    }

    #[test]
    fn test_find_msvc_packages_architecture_filtering() {
        let manifest = create_test_manifest();
        let empty_components = HashSet::new();
        let empty_patterns: Vec<String> = vec![];

        // Find packages for x64 target
        let x64_packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &empty_components, &empty_patterns);

        // Should include x64 tools
        assert!(x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.Tools.HostX64.TargetX64.base"));

        // Should NOT include ARM64 or x86 tools
        assert!(!x64_packages.iter().any(|p| p.id.contains("TargetARM64")));
        assert!(!x64_packages.iter().any(|p| p.id.contains("TargetX86")));

        // Should include x64 CRT
        assert!(x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CRT.x64.Desktop"));

        // Should NOT include ARM64 or x86 CRT
        assert!(!x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CRT.ARM64.Desktop"));
        assert!(!x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CRT.x86.Desktop"));

        // Should include x64 MFC and ATL
        assert!(x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.MFC.x64"));
        assert!(x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.ATL.x64"));

        // Should NOT include ARM64 MFC and ATL
        assert!(!x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.MFC.ARM64"));
        assert!(!x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.ATL.ARM64"));

        // Should include architecture-neutral CRT.Headers
        assert!(x64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CRT.Headers"));
    }

    #[test]
    fn test_find_msvc_packages_spectre_filtering() {
        let manifest = create_test_manifest();
        let empty_components = HashSet::new();
        let empty_patterns: Vec<String> = vec![];

        // Find packages for x64 target (no Spectre component requested)
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &empty_components, &empty_patterns);

        // Should NOT include Spectre-mitigated libraries
        assert!(!packages.iter().any(|p| p.id.contains(".Spectre")));
        assert!(!packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CRT.x64.Desktop.Spectre"));
        assert!(!packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.MFC.x64.Spectre"));
    }

    #[test]
    fn test_find_msvc_packages_spectre_inclusion() {
        let manifest = create_test_manifest();
        let mut components = HashSet::new();
        components.insert(MsvcComponent::Spectre);
        let empty_patterns: Vec<String> = vec![];

        // Find packages for x64 target WITH Spectre component requested
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &components, &empty_patterns);

        // Should include Spectre-mitigated libraries
        assert!(packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CRT.x64.Desktop.Spectre"));
    }

    #[test]
    fn test_find_msvc_packages_exclude_patterns() {
        let manifest = create_test_manifest();
        let empty_components = HashSet::new();
        let exclude_patterns = vec![".mfc".to_string()];

        // Find packages for x64 target with MFC excluded
        let packages = manifest.find_msvc_packages(
            "14.44",
            "x64",
            "x64",
            &empty_components,
            &exclude_patterns,
        );

        // Should NOT include MFC packages
        assert!(!packages
            .iter()
            .any(|p| p.id.to_lowercase().contains(".mfc")));

        // Should still include ATL and CRT
        assert!(packages.iter().any(|p| p.id.contains("ATL")));
        assert!(packages.iter().any(|p| p.id.contains("CRT")));
    }

    #[test]
    fn test_find_msvc_packages_cli_inclusion() {
        let manifest = create_test_manifest();
        let empty_components = HashSet::new();
        let empty_patterns: Vec<String> = vec![];

        // Without CLI component, CLI packages should NOT be included
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &empty_components, &empty_patterns);
        assert!(!packages
            .iter()
            .any(|p| p.id.to_lowercase().contains(".cli")));

        // With CLI component, CLI packages SHOULD be included
        let mut components = HashSet::new();
        components.insert(MsvcComponent::Cli);
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &components, &empty_patterns);
        assert!(packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CLI.x64"));
        // Should NOT include ARM64 CLI when targeting x64
        assert!(!packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CLI.ARM64"));
    }

    #[test]
    fn test_find_msvc_packages_modules_inclusion() {
        let manifest = create_test_manifest();
        let empty_components = HashSet::new();
        let empty_patterns: Vec<String> = vec![];

        // Without Modules component, Modules packages should NOT be included
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &empty_components, &empty_patterns);
        assert!(!packages
            .iter()
            .any(|p| p.id.to_lowercase().contains(".modules")));

        // With Modules component, Modules packages SHOULD be included
        let mut components = HashSet::new();
        components.insert(MsvcComponent::Modules);
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &components, &empty_patterns);
        assert!(packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.Modules.x64"));
    }

    #[test]
    fn test_find_msvc_packages_redist_inclusion() {
        let manifest = create_test_manifest();
        let empty_components = HashSet::new();
        let empty_patterns: Vec<String> = vec![];

        // Without Redist component, Redist packages should NOT be included
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &empty_components, &empty_patterns);
        assert!(!packages
            .iter()
            .any(|p| p.id.to_lowercase().contains(".redist")));

        // With Redist component, Redist packages SHOULD be included
        let mut components = HashSet::new();
        components.insert(MsvcComponent::Redist);
        let packages =
            manifest.find_msvc_packages("14.44", "x64", "x64", &components, &empty_patterns);
        assert!(packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.Redist.x64"));
        // Should NOT include ARM64 Redist when targeting x64
        assert!(!packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.Redist.ARM64"));
    }

    #[test]
    fn test_find_msvc_packages_arm64_target() {
        let manifest = create_test_manifest();
        let empty_components = HashSet::new();
        let empty_patterns: Vec<String> = vec![];

        // Find packages for ARM64 target
        let arm64_packages = manifest.find_msvc_packages(
            "14.44",
            "x64",
            "arm64",
            &empty_components,
            &empty_patterns,
        );

        // Should include ARM64 tools (cross-compilation from x64 host)
        assert!(arm64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.Tools.HostX64.TargetARM64.base"));

        // Should NOT include x64 or x86 tools
        assert!(!arm64_packages.iter().any(|p| p.id.contains("TargetX64")));
        assert!(!arm64_packages.iter().any(|p| p.id.contains("TargetX86")));

        // Should include ARM64 CRT, MFC, ATL
        assert!(arm64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CRT.ARM64.Desktop"));
        assert!(arm64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.MFC.ARM64"));
        assert!(arm64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.ATL.ARM64"));

        // Should still include architecture-neutral headers
        assert!(arm64_packages
            .iter()
            .any(|p| p.id == "Microsoft.VC.14.44.CRT.Headers"));
    }

    #[test]
    fn test_find_sdk_packages() {
        let manifest = create_test_manifest();

        // Find SDK packages for 10.0.26100.0
        let packages = manifest.find_sdk_packages("10.0.26100.0", "x64");

        // Should find the SDK package
        assert!(!packages.is_empty());
        assert!(packages.iter().any(|p| p.id.contains("Win11SDK")));
    }

    #[test]
    fn test_find_sdk_packages_architecture_filtering() {
        let manifest = create_test_manifest();

        // Find SDK packages for x64 target
        let x64_packages = manifest.find_sdk_packages("10.0.26100.0", "x64");

        // Should include x64 SDK
        assert!(x64_packages.iter().any(|p| p.id == "Win11SDK_10.0.26100"));

        // Should NOT include ARM64 SDK
        assert!(!x64_packages
            .iter()
            .any(|p| p.id == "Win11SDK_10.0.26100_arm64"));

        // Should include neutral packages
        assert!(x64_packages
            .iter()
            .any(|p| p.id == "Win11SDK_10.0.26100_Headers"));
    }

    #[test]
    fn test_find_sdk_packages_arm64_target() {
        let manifest = create_test_manifest();

        // Find SDK packages for ARM64 target
        let arm64_packages = manifest.find_sdk_packages("10.0.26100.0", "arm64");

        // Should include ARM64 SDK
        assert!(arm64_packages
            .iter()
            .any(|p| p.id == "Win11SDK_10.0.26100_arm64"));

        // Should NOT include x64 SDK (no _x64 suffix, but chip is x64)
        // Note: Win11SDK_10.0.26100 has chip=x64, so it should be excluded
        assert!(!arm64_packages
            .iter()
            .any(|p| p.id == "Win11SDK_10.0.26100" && p.chip == Some("x64".to_string())));

        // Should include neutral packages
        assert!(arm64_packages
            .iter()
            .any(|p| p.id == "Win11SDK_10.0.26100_Headers"));
    }

    /// Build a minimal channel manifest body pointing at `vsman_url`
    fn channel_manifest_body(vsman_url: &str) -> String {
        format!(
            r#"{{
                "manifestVersion": "1.1",
                "info": {{ "productDisplayVersion": "18.0.0" }},
                "channelItems": [
                    {{
                        "id": "Microsoft.VisualStudio.Manifests.VisualStudio",
                        "version": "18.0.0",
                        "type": "Manifest",
                        "payloads": [
                            {{ "fileName": "VisualStudio.vsman", "url": "{}", "size": 64 }}
                        ]
                    }}
                ]
            }}"#,
            vsman_url
        )
    }

    /// Build a minimal VS package manifest body
    fn vs_manifest_body() -> String {
        r#"{
            "manifestVersion": "1.1",
            "packages": [
                {
                    "id": "Microsoft.VC.14.50.Tools.HostX64.TargetX64.base",
                    "version": "14.50.1000",
                    "type": "Vsix",
                    "chip": "x64",
                    "payloads": []
                }
            ]
        }"#
        .to_string()
    }

    #[test]
    fn html_response_is_reported_as_unpublished_channel() {
        let body = b"<!doctype html><html lang=\"en\"><body>Not found</body></html>";
        let err = parse_channel_manifest(body).unwrap_err();

        assert!(err.contains("HTML"), "{}", err);
        assert!(err.contains("not published"), "{}", err);
    }

    #[test]
    fn empty_response_is_rejected() {
        let err = parse_channel_manifest(b"   \n\t ").unwrap_err();
        assert!(err.contains("empty"), "{}", err);
    }

    #[test]
    fn malformed_json_is_rejected_with_context() {
        let err = parse_channel_manifest(b"{\"manifestVersion\": ").unwrap_err();
        assert!(err.contains("not a valid channel manifest"), "{}", err);
    }

    #[test]
    fn valid_channel_manifest_is_parsed() {
        let body = channel_manifest_body("https://example.com/VisualStudio.vsman");
        let manifest = parse_channel_manifest(body.as_bytes()).expect("valid channel manifest");

        assert_eq!(
            manifest.info.and_then(|info| info.product_display_version),
            Some("18.0.0".to_string())
        );
        assert_eq!(manifest.channel_items.len(), 1);
    }

    #[test]
    fn http_level_failures_are_reported_as_unavailable_channel() {
        let channel = VsChannelSpec::from_major(18);
        let err = classify_channel_error(
            &channel,
            MsvcKitError::Other(
                "Failed to fetch https://aka.ms/vs/18/release/channel: HTTP 404".to_string(),
            ),
        );

        match err {
            MsvcKitError::ChannelUnavailable {
                channel: name,
                url,
                reason,
            } => {
                assert_eq!(name, "Visual Studio 2026 (v18)");
                assert!(url.contains("/vs/18/"), "{}", url);
                assert!(reason.contains("404"), "{}", reason);
            }
            other => panic!("expected ChannelUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pinned_html_channel_fails_cleanly_without_poisoning_the_cache() {
        let mut server = mockito::Server::new_async().await;
        let html = server
            .mock("GET", "/vs/18/release/channel")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<!doctype html><html>not published yet</html>")
            .create_async()
            .await;

        let cache_dir = tempfile::tempdir().unwrap();
        let channel = VsChannelSpec::new(
            18,
            Some(2026),
            format!("{}/vs/18/release/channel", server.url()),
        );

        let err = VsManifest::fetch_with_channel(&channel, cache_dir.path())
            .await
            .expect_err("HTML channel must not be treated as a manifest");

        match err {
            MsvcKitError::ChannelUnavailable { reason, .. } => {
                assert!(reason.contains("HTML"), "{}", reason)
            }
            other => panic!("expected ChannelUnavailable, got {other:?}"),
        }

        html.assert_async().await;
        assert!(
            !cache_dir.path().join("channel-v18.json").exists(),
            "an unpublished channel body must not stay in the cache"
        );
    }

    #[tokio::test]
    async fn pinned_channel_serves_a_manifest() {
        let mut server = mockito::Server::new_async().await;
        let vsman_url = format!("{}/vsman/VisualStudio.vsman", server.url());

        let channel_mock = server
            .mock("GET", "/vs/18/release/channel")
            .with_status(200)
            .with_body(channel_manifest_body(&vsman_url))
            .create_async()
            .await;
        let vsman_mock = server
            .mock("GET", "/vsman/VisualStudio.vsman")
            .with_status(200)
            .with_body(vs_manifest_body())
            .create_async()
            .await;

        let cache_dir = tempfile::tempdir().unwrap();
        let channel = VsChannelSpec::new(
            18,
            Some(2026),
            format!("{}/vs/18/release/channel", server.url()),
        );

        let manifest = VsManifest::fetch_with_channel(&channel, cache_dir.path())
            .await
            .expect("manifest should load");

        assert_eq!(manifest.list_msvc_versions(), vec!["14.50".to_string()]);
        channel_mock.assert_async().await;
        vsman_mock.assert_async().await;
        assert!(cache_dir.path().join("channel-v18.json").exists());
    }

    #[tokio::test]
    async fn auto_selection_falls_back_to_the_next_channel() {
        let mut server = mockito::Server::new_async().await;
        let vsman_url = format!("{}/vsman/VisualStudio.vsman", server.url());

        // Newest channel: upstream has not published a JSON manifest yet.
        let unpublished = server
            .mock("GET", "/vs/19/release/channel")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<!doctype html><html>search results</html>")
            .create_async()
            .await;
        // Older channel: serves a real manifest.
        let published = server
            .mock("GET", "/vs/18/release/channel")
            .with_status(200)
            .with_body(channel_manifest_body(&vsman_url))
            .create_async()
            .await;
        let vsman = server
            .mock("GET", "/vsman/VisualStudio.vsman")
            .with_status(200)
            .with_body(vs_manifest_body())
            .create_async()
            .await;

        let cache_dir = tempfile::tempdir().unwrap();
        let candidates = vec![
            VsChannelSpec::new(19, None, format!("{}/vs/19/release/channel", server.url())),
            VsChannelSpec::new(
                18,
                Some(2026),
                format!("{}/vs/18/release/channel", server.url()),
            ),
        ];

        let (manifest, used) = VsManifest::fetch_auto_from(&candidates, cache_dir.path())
            .await
            .expect("should fall back to the published channel");

        assert_eq!(used.major, 18);
        assert_eq!(manifest.list_msvc_versions(), vec!["14.50".to_string()]);
        unpublished.assert_async().await;
        published.assert_async().await;
        vsman.assert_async().await;
        assert!(
            !cache_dir.path().join("channel-v19.json").exists(),
            "the unpublished channel must not be cached"
        );
    }

    #[tokio::test]
    async fn auto_selection_reports_when_no_channel_is_available() {
        let mut server = mockito::Server::new_async().await;
        let unavailable = server
            .mock("GET", "/vs/19/release/channel")
            .with_status(200)
            .with_body("<!doctype html><html>nope</html>")
            .create_async()
            .await;

        let cache_dir = tempfile::tempdir().unwrap();
        let candidates = vec![VsChannelSpec::new(
            19,
            None,
            format!("{}/vs/19/release/channel", server.url()),
        )];

        let err = VsManifest::fetch_auto_from(&candidates, cache_dir.path())
            .await
            .expect_err("no channel is usable");

        match err {
            MsvcKitError::ChannelUnavailable { reason, .. } => {
                assert!(
                    reason.contains("no channel serves a usable manifest"),
                    "{}",
                    reason
                )
            }
            other => panic!("expected ChannelUnavailable, got {other:?}"),
        }
        unavailable.assert_async().await;
    }
}
