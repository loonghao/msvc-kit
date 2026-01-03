//! MSVC compiler download functionality

use super::{common::CommonDownloader, DownloadOptions, VsManifest};
use crate::error::{MsvcKitError, Result};
use crate::installer::{extract_packages_with_progress, InstallInfo};
use crate::version::Architecture;

/// MSVC downloader
pub struct MsvcDownloader {
    downloader: CommonDownloader,
}

impl MsvcDownloader {
    /// Create a new MSVC downloader
    pub fn new(options: DownloadOptions) -> Self {
        Self {
            downloader: CommonDownloader::new(options),
        }
    }

    /// Download MSVC components
    pub async fn download(&self) -> Result<InstallInfo> {
        let manifest = VsManifest::fetch().await?;

        // List available versions for debugging
        let available_versions = manifest.list_msvc_versions();
        tracing::debug!("Available MSVC versions: {:?}", available_versions);

        // Determine version to download
        let version = self
            .downloader
            .options
            .msvc_version
            .clone()
            .or_else(|| manifest.get_latest_msvc_version())
            .ok_or_else(|| {
                MsvcKitError::VersionNotFound(format!(
                    "No MSVC version found. Available: {:?}",
                    available_versions
                ))
            })?;

        tracing::info!("Selected MSVC version: {}", version);

        // Determine architectures
        let host_arch = self
            .downloader
            .options
            .host_arch
            .unwrap_or(Architecture::host())
            .to_string();
        let target_arch = self.downloader.options.arch.to_string();

        tracing::info!(
            "Host architecture: {}, Target architecture: {}",
            host_arch,
            target_arch
        );

        // Find packages to download
        let packages = manifest.find_msvc_packages(&version, &host_arch, &target_arch);

        if packages.is_empty() {
            return Err(MsvcKitError::ComponentNotFound(format!(
                "No MSVC packages found for version {} (host: {}, target: {})",
                version, host_arch, target_arch
            )));
        }

        tracing::info!("Found {} MSVC packages to download", packages.len());
        for pkg in &packages {
            tracing::debug!(
                "  - {} ({})",
                pkg.id,
                humansize::format_size(pkg.total_size, humansize::BINARY)
            );
        }

        // Create download directory with version and architecture info
        // Structure: downloads/msvc/{version}_{host}_{target}/
        let download_subdir = format!("{}_{}_{}",
            version.replace('.', "_"),
            host_arch.to_lowercase(),
            target_arch.to_lowercase()
        );
        let download_dir = self
            .downloader
            .options
            .target_dir
            .join("downloads")
            .join("msvc")
            .join(&download_subdir);
        tokio::fs::create_dir_all(&download_dir).await?;

        tracing::info!(
            "Download directory: {:?} (version={}, host={}, target={})",
            download_dir, version, host_arch, target_arch
        );

        // Download all packages
        let downloaded_files = self
            .downloader
            .download_packages(&packages, &download_dir, "MSVC")
            .await?;

        // Extract packages
        let install_path = self.downloader.options.target_dir.clone();
        tracing::info!("Extracting MSVC packages to {:?}", install_path);

        extract_packages_with_progress(&downloaded_files, &install_path, "MSVC").await?;

        // Find the actual MSVC version directory
        let vc_tools_path = install_path.join("VC").join("Tools").join("MSVC");
        let msvc_version_dir = if vc_tools_path.exists() {
            // Find the version directory
            let mut entries = tokio::fs::read_dir(&vc_tools_path).await?;
            let mut version_dir = None;
            while let Some(entry) = entries.next_entry().await? {
                if entry.file_type().await?.is_dir() {
                    version_dir = Some(entry.path());
                    break;
                }
            }
            version_dir.unwrap_or(vc_tools_path)
        } else {
            install_path.clone()
        };

        Ok(InstallInfo {
            component_type: "msvc".to_string(),
            version: version.clone(),
            install_path: msvc_version_dir,
            downloaded_files,
            arch: self.downloader.options.arch,
        })
    }
}
