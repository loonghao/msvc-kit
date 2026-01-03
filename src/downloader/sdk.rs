//! Windows SDK download functionality

use super::{common::CommonDownloader, DownloadOptions, VsManifest};
use crate::error::{MsvcKitError, Result};
use crate::installer::{extract_packages_with_progress, InstallInfo};

/// Windows SDK downloader
pub struct SdkDownloader {
    downloader: CommonDownloader,
}

impl SdkDownloader {
    /// Create a new SDK downloader
    pub fn new(options: DownloadOptions) -> Self {
        Self {
            downloader: CommonDownloader::new(options),
        }
    }

    /// Download Windows SDK components
    pub async fn download(&self) -> Result<InstallInfo> {
        let manifest = VsManifest::fetch().await?;

        // List available versions for debugging
        let available_versions = manifest.list_sdk_versions();
        tracing::debug!("Available SDK versions: {:?}", available_versions);

        // Determine version to download
        let version = self
            .downloader
            .options
            .sdk_version
            .clone()
            .or_else(|| manifest.get_latest_sdk_version())
            .ok_or_else(|| {
                MsvcKitError::VersionNotFound(format!(
                    "No Windows SDK version found. Available: {:?}",
                    available_versions
                ))
            })?;

        tracing::info!("Selected Windows SDK version: {}", version);

        // Determine target architecture
        let target_arch = self.downloader.options.arch.to_string();

        tracing::info!("Target architecture: {}", target_arch);

        // Find packages to download
        let packages = manifest.find_sdk_packages(&version, &target_arch);

        if packages.is_empty() {
            return Err(MsvcKitError::ComponentNotFound(format!(
                "No Windows SDK packages found for version {} (target: {})",
                version, target_arch
            )));
        }

        tracing::info!("Found {} SDK packages to download", packages.len());
        for pkg in &packages {
            tracing::debug!(
                "  - {} ({})",
                pkg.id,
                humansize::format_size(pkg.total_size, humansize::BINARY)
            );
        }

        // Create download directory with version and architecture info
        // Structure: downloads/sdk/{build_number}_{target}/
        // Extract build number from version (e.g., "10.0.26100.0" -> "26100")
        let build_number = version.split('.').nth(2).unwrap_or(&version);
        let download_subdir = format!("{}_{}", build_number, target_arch.to_lowercase());
        let download_dir = self
            .downloader
            .options
            .target_dir
            .join("downloads")
            .join("sdk")
            .join(&download_subdir);
        tokio::fs::create_dir_all(&download_dir).await?;

        tracing::info!(
            "Download directory: {:?} (version={}, target={})",
            download_dir, version, target_arch
        );

        // Download all packages
        let downloaded_files = self
            .downloader
            .download_packages(&packages, &download_dir, "Windows SDK")
            .await?;

        // Extract packages
        let install_path = self.downloader.options.target_dir.clone();
        tracing::info!("Extracting SDK packages to {:?}", install_path);

        extract_packages_with_progress(&downloaded_files, &install_path, "Windows SDK").await?;

        // Determine the actual SDK install path
        let sdk_path = install_path.join("Windows Kits").join("10");

        Ok(InstallInfo {
            component_type: "sdk".to_string(),
            version,
            install_path: sdk_path,
            downloaded_files,
            arch: self.downloader.options.arch,
        })
    }
}
