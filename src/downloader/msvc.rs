//! MSVC compiler download functionality

use async_trait::async_trait;

use super::http::create_http_client;
use super::traits::{ComponentDownloader, ComponentType};
use super::{
    common::CommonDownloader, DownloadOptions, DownloadPreview, PackagePreview, VsManifest,
};
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
        let client = options
            .http_client
            .clone()
            .unwrap_or_else(create_http_client);
        let progress_handler = options.progress_handler.clone();

        let mut downloader = CommonDownloader::with_client(options, client);
        if let Some(handler) = progress_handler {
            downloader = downloader.with_progress_handler(handler);
        }

        Self { downloader }
    }

    /// Preview what would be downloaded (dry-run mode)
    pub async fn preview(&self) -> Result<DownloadPreview> {
        let manifest = VsManifest::fetch().await?;

        let available_versions = manifest.list_msvc_versions();
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

        let host_arch = self
            .downloader
            .options
            .host_arch
            .unwrap_or(Architecture::host())
            .to_string();
        let target_arch = self.downloader.options.arch.to_string();

        let packages = manifest.find_msvc_packages(&version, &host_arch, &target_arch);

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
            component: "MSVC".to_string(),
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
                component_type: "msvc".to_string(),
                version: preview.version,
                install_path: self.downloader.options.target_dir.clone(),
                downloaded_files: vec![],
                arch: self.downloader.options.arch,
            });
        }

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
        let download_subdir = format!(
            "{}_{}_{}",
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
            download_dir,
            version,
            host_arch,
            target_arch
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

        // Find the actual MSVC version directory and extract the full version number
        let vc_tools_path = install_path.join("VC").join("Tools").join("MSVC");
        let (msvc_version_dir, actual_version) = if vc_tools_path.exists() {
            // Find the version directory - this contains the full version number (e.g., 14.44.34823)
            let mut entries = tokio::fs::read_dir(&vc_tools_path).await?;
            let mut version_dir = None;
            let mut full_version = version.clone();
            while let Some(entry) = entries.next_entry().await? {
                if entry.file_type().await?.is_dir() {
                    let dir_name = entry.file_name();
                    if let Some(name) = dir_name.to_str() {
                        // The directory name is the full version (e.g., "14.44.34823")
                        full_version = name.to_string();
                        version_dir = Some(entry.path());
                        tracing::info!(
                            "Found MSVC version directory: {} (full version: {})",
                            entry.path().display(),
                            full_version
                        );
                        break;
                    }
                }
            }
            (version_dir.unwrap_or(vc_tools_path), full_version)
        } else {
            (install_path.clone(), version.clone())
        };

        Ok(InstallInfo {
            component_type: "msvc".to_string(),
            version: actual_version,
            install_path: msvc_version_dir,
            downloaded_files,
            arch: self.downloader.options.arch,
        })
    }

    /// Download MSVC components
    pub async fn download(&self) -> Result<InstallInfo> {
        self.download_impl().await
    }
}

#[async_trait]
impl ComponentDownloader for MsvcDownloader {
    async fn download(&self) -> Result<InstallInfo> {
        self.download_impl().await
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Msvc
    }
}
