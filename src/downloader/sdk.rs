//! Windows SDK download functionality

use async_trait::async_trait;

use super::http::create_http_client;
use super::traits::{ComponentDownloader, ComponentType};
use super::{
    common::CommonDownloader, DownloadOptions, DownloadPreview, PackagePreview, VsManifest,
};
use crate::error::{MsvcKitError, Result};
use crate::installer::InstallInfo;

/// Windows SDK downloader
pub struct SdkDownloader {
    downloader: CommonDownloader,
}

impl SdkDownloader {
    /// Create a new SDK downloader
    pub fn new(options: DownloadOptions) -> Self {
        let client = options
            .http_client
            .clone()
            .unwrap_or_else(create_http_client);
        let progress_handler = options.progress_handler.clone();
        let cache_manager = options.cache_manager.clone();

        let mut downloader = CommonDownloader::with_client(options, client);
        if let Some(handler) = progress_handler {
            downloader = downloader.with_progress_handler(handler);
        }
        if let Some(cm) = cache_manager {
            downloader = downloader.with_cache_manager(cm);
        }

        Self { downloader }
    }

    /// Preview what would be downloaded (dry-run mode)
    pub async fn preview(&self) -> Result<DownloadPreview> {
        let manifest = VsManifest::fetch().await?;

        let available_versions = manifest.list_sdk_versions();
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

        let target_arch = self.downloader.options.arch.to_string();
        let packages = manifest.find_sdk_packages(&version, &target_arch);

        let file_count: usize = packages.iter().map(|p| p.payloads.len()).sum();
        let total_size: u64 = packages.iter().map(|p| p.total_size).sum();

        let package_previews: Vec<PackagePreview> = packages
            .iter()
            .map(|p| PackagePreview {
                id: p.id.clone(),
                version: p.version.clone(),
                file_count: p.payloads.len(),
                size: p.total_size,
            })
            .collect();

        Ok(DownloadPreview {
            component: "Windows SDK".to_string(),
            version,
            package_count: packages.len(),
            file_count,
            total_size,
            packages: package_previews,
        })
    }

    /// Internal download implementation
    async fn download_impl(&self) -> Result<InstallInfo> {
        // Check for dry-run mode
        if self.downloader.options.dry_run {
            let preview = self.preview().await?;
            tracing::info!("Dry-run mode: {}", preview.format());
            for pkg in &preview.packages {
                tracing::info!(
                    "  - {} v{} ({} files, {})",
                    pkg.id,
                    pkg.version,
                    pkg.file_count,
                    humansize::format_size(pkg.size, humansize::BINARY)
                );
            }
            return Ok(InstallInfo {
                component_type: "sdk".to_string(),
                version: preview.version,
                install_path: self.downloader.options.target_dir.clone(),
                downloaded_files: vec![],
                arch: self.downloader.options.arch,
            });
        }

        // Use custom cache dir if a cache_manager was injected
        let cache_dir = self.downloader.manifest_cache_dir();
        let manifest = VsManifest::fetch_with_cache_dir(&cache_dir).await?;

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
            download_dir,
            version,
            target_arch
        );

        // Download all packages
        let downloaded_files = self
            .downloader
            .download_packages(&packages, &download_dir, "Windows SDK")
            .await?;

        tracing::info!("Downloaded {} SDK packages", downloaded_files.len());

        // Return InstallInfo with target_dir as install_path (not extracted yet)
        Ok(InstallInfo {
            component_type: "sdk".to_string(),
            version,
            install_path: self.downloader.options.target_dir.clone(),
            downloaded_files,
            arch: self.downloader.options.arch,
        })
    }

    /// Download Windows SDK components
    pub async fn download(&self) -> Result<InstallInfo> {
        self.download_impl().await
    }
}

#[async_trait]
impl ComponentDownloader for SdkDownloader {
    async fn download(&self) -> Result<InstallInfo> {
        self.download_impl().await
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Sdk
    }
}
