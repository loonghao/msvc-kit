//! Download functionality for MSVC and Windows SDK components

mod common;
mod index;
mod manifest;
mod msvc;
mod sdk;

#[cfg(test)]
mod common_tests;

use std::path::PathBuf;

use crate::error::Result;
use crate::installer::InstallInfo;
use crate::version::Architecture;

pub use index::{DownloadIndex, DownloadStatus, IndexEntry};
pub use manifest::{ChannelManifest, Package, PackagePayload, VsManifest};
pub use msvc::MsvcDownloader;
pub use sdk::SdkDownloader;

/// Options for downloading MSVC/SDK components
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// MSVC version to download (None = latest)
    pub msvc_version: Option<String>,

    /// Windows SDK version to download (None = latest)
    pub sdk_version: Option<String>,

    /// Target directory for installation
    pub target_dir: PathBuf,

    /// Target architecture
    pub arch: Architecture,

    /// Host architecture (for cross-compilation)
    pub host_arch: Option<Architecture>,

    /// Whether to verify file hashes
    pub verify_hashes: bool,

    /// Number of parallel downloads
    pub parallel_downloads: usize,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            msvc_version: None,
            sdk_version: None,
            target_dir: PathBuf::from("msvc-kit"),
            arch: Architecture::host(),
            host_arch: None,
            verify_hashes: true,
            parallel_downloads: 4,
        }
    }
}

/// Download MSVC compiler components
///
/// This function downloads the MSVC compiler toolchain from Microsoft servers
/// and extracts it to the specified directory.
///
/// # Arguments
///
/// * `options` - Download options including version and target directory
///
/// # Returns
///
/// Returns `InstallInfo` containing paths to installed components
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::{download_msvc, DownloadOptions};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let options = DownloadOptions::default();
///     let info = download_msvc(&options).await?;
///     println!("Installed to: {:?}", info.install_path);
///     Ok(())
/// }
/// ```
pub async fn download_msvc(options: &DownloadOptions) -> Result<InstallInfo> {
    let downloader = MsvcDownloader::new(options.clone());
    downloader.download().await
}

/// Download Windows SDK components
///
/// This function downloads the Windows SDK from Microsoft servers
/// and extracts it to the specified directory.
///
/// # Arguments
///
/// * `options` - Download options including version and target directory
///
/// # Returns
///
/// Returns `InstallInfo` containing paths to installed components
pub async fn download_sdk(options: &DownloadOptions) -> Result<InstallInfo> {
    let downloader = SdkDownloader::new(options.clone());
    downloader.download().await
}

/// Download both MSVC and Windows SDK
///
/// Convenience function to download both components in one call.
pub async fn download_all(options: &DownloadOptions) -> Result<(InstallInfo, InstallInfo)> {
    let msvc_info = download_msvc(options).await?;
    let sdk_info = download_sdk(options).await?;
    Ok((msvc_info, sdk_info))
}
