//! Download functionality for MSVC and Windows SDK components

pub mod cache;
mod common;
pub mod hash;
pub mod http;
mod index;
mod manifest;
mod msvc;
pub mod progress;
mod sdk;
mod traits;

#[cfg(test)]
mod common_tests;

use std::path::PathBuf;

use crate::error::Result;
use crate::installer::InstallInfo;
use crate::version::Architecture;

pub use common::CommonDownloader;
pub use hash::{compute_file_hash, compute_hash, hashes_match};
pub use http::{
    create_http_client, create_http_client_with_config, tls_backend_name, HttpClientConfig,
};
pub use index::{DownloadIndex, DownloadStatus, IndexEntry};
pub use manifest::{ChannelManifest, Package, PackagePayload, VsManifest};
pub use msvc::MsvcDownloader;
pub use progress::{
    BoxedProgressHandler, IndicatifProgressHandler, NoopProgressHandler, ProgressHandler,
};
pub use sdk::SdkDownloader;
pub use traits::{
    BoxedCacheManager, CacheManager, ComponentDownloader, ComponentType, FileSystemCacheManager,
};

/// Options for downloading MSVC/SDK components
#[derive(Clone)]
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

    /// Custom HTTP client (None = create default)
    pub http_client: Option<reqwest::Client>,

    /// Custom progress handler (None = use default indicatif)
    pub progress_handler: Option<BoxedProgressHandler>,

    /// Custom cache manager (None = use default file system cache)
    pub cache_manager: Option<BoxedCacheManager>,

    /// Dry-run mode: preview what would be downloaded without actually downloading
    pub dry_run: bool,
}

impl std::fmt::Debug for DownloadOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadOptions")
            .field("msvc_version", &self.msvc_version)
            .field("sdk_version", &self.sdk_version)
            .field("target_dir", &self.target_dir)
            .field("arch", &self.arch)
            .field("host_arch", &self.host_arch)
            .field("verify_hashes", &self.verify_hashes)
            .field("parallel_downloads", &self.parallel_downloads)
            .field("http_client", &self.http_client.is_some())
            .field("progress_handler", &self.progress_handler.is_some())
            .field("cache_manager", &self.cache_manager.is_some())
            .field("dry_run", &self.dry_run)
            .finish()
    }
}

impl Default for DownloadOptions {
    fn default() -> Self {
        use crate::constants::download::DEFAULT_PARALLEL_DOWNLOADS;

        // Support environment variable overrides
        let target_dir = std::env::var("MSVC_KIT_INSTALL_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("msvc-kit"));

        let parallel_downloads = std::env::var("MSVC_KIT_PARALLEL_DOWNLOADS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PARALLEL_DOWNLOADS);

        let verify_hashes = std::env::var("MSVC_KIT_VERIFY_HASHES")
            .ok()
            .map(|s| !matches!(s.to_lowercase().as_str(), "0" | "false" | "no"))
            .unwrap_or(true);

        let dry_run = std::env::var("MSVC_KIT_DRY_RUN")
            .ok()
            .map(|s| matches!(s.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        Self {
            msvc_version: std::env::var("MSVC_KIT_MSVC_VERSION").ok(),
            sdk_version: std::env::var("MSVC_KIT_SDK_VERSION").ok(),
            target_dir,
            arch: Architecture::host(),
            host_arch: None,
            verify_hashes,
            parallel_downloads,
            http_client: None,
            progress_handler: None,
            cache_manager: None,
            dry_run,
        }
    }
}

impl DownloadOptions {
    /// Create a builder for download options
    pub fn builder() -> DownloadOptionsBuilder {
        DownloadOptionsBuilder::default()
    }
}

/// Builder for DownloadOptions
#[derive(Default)]
pub struct DownloadOptionsBuilder {
    options: DownloadOptions,
}

impl DownloadOptionsBuilder {
    /// Set MSVC version
    pub fn msvc_version(mut self, version: impl Into<String>) -> Self {
        self.options.msvc_version = Some(version.into());
        self
    }

    /// Set SDK version
    pub fn sdk_version(mut self, version: impl Into<String>) -> Self {
        self.options.sdk_version = Some(version.into());
        self
    }

    /// Set target directory
    pub fn target_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.options.target_dir = dir.into();
        self
    }

    /// Set target architecture
    pub fn arch(mut self, arch: Architecture) -> Self {
        self.options.arch = arch;
        self
    }

    /// Set host architecture
    pub fn host_arch(mut self, arch: Architecture) -> Self {
        self.options.host_arch = Some(arch);
        self
    }

    /// Set hash verification
    pub fn verify_hashes(mut self, verify: bool) -> Self {
        self.options.verify_hashes = verify;
        self
    }

    /// Set parallel downloads count
    pub fn parallel_downloads(mut self, count: usize) -> Self {
        self.options.parallel_downloads = count;
        self
    }

    /// Set custom HTTP client
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.options.http_client = Some(client);
        self
    }

    /// Set custom progress handler
    pub fn progress_handler(mut self, handler: BoxedProgressHandler) -> Self {
        self.options.progress_handler = Some(handler);
        self
    }

    /// Set custom cache manager for manifest and payload caching
    pub fn cache_manager(mut self, manager: BoxedCacheManager) -> Self {
        self.options.cache_manager = Some(manager);
        self
    }

    /// Enable dry-run mode (preview without downloading)
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.options.dry_run = dry_run;
        self
    }

    /// Build the options
    pub fn build(self) -> DownloadOptions {
        self.options
    }
}

/// Preview information for dry-run mode
#[derive(Debug, Clone)]
pub struct DownloadPreview {
    /// Component type (MSVC or SDK)
    pub component: String,
    /// Version to be downloaded
    pub version: String,
    /// Total number of packages
    pub package_count: usize,
    /// Total number of files
    pub file_count: usize,
    /// Total size in bytes
    pub total_size: u64,
    /// List of packages with their sizes
    pub packages: Vec<PackagePreview>,
}

/// Preview information for a single package
#[derive(Debug, Clone)]
pub struct PackagePreview {
    /// Package ID
    pub id: String,
    /// Package version
    pub version: String,
    /// Number of files in package
    pub file_count: usize,
    /// Total size of package in bytes
    pub size: u64,
}

impl DownloadPreview {
    /// Format the preview as a human-readable string
    pub fn format(&self) -> String {
        let size_str = humansize::format_size(self.total_size, humansize::BINARY);
        format!(
            "{} v{}: {} packages, {} files, {}",
            self.component, self.version, self.package_count, self.file_count, size_str
        )
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
/// Downloads are performed in parallel for better performance.
pub async fn download_all(options: &DownloadOptions) -> Result<(InstallInfo, InstallInfo)> {
    // Run MSVC and SDK downloads in parallel for better performance
    let (msvc_result, sdk_result) = tokio::join!(download_msvc(options), download_sdk(options));

    let msvc_info = msvc_result?;
    let sdk_info = sdk_result?;
    Ok((msvc_info, sdk_info))
}

/// Information about available versions from Microsoft servers
#[derive(Debug, Clone)]
pub struct AvailableVersions {
    /// Available MSVC toolset versions (short format, e.g., "14.44")
    pub msvc_versions: Vec<String>,
    /// Available Windows SDK versions (e.g., "10.0.26100.0")
    pub sdk_versions: Vec<String>,
    /// Latest MSVC version
    pub latest_msvc: Option<String>,
    /// Latest SDK version
    pub latest_sdk: Option<String>,
}

/// Fetch available MSVC and Windows SDK versions from Microsoft servers
///
/// This function queries the Visual Studio manifest to get all available
/// versions that can be downloaded.
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::list_available_versions;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let versions = list_available_versions().await?;
///     
///     println!("Latest MSVC: {:?}", versions.latest_msvc);
///     println!("Latest SDK: {:?}", versions.latest_sdk);
///     
///     println!("\nAvailable MSVC versions:");
///     for v in &versions.msvc_versions {
///         println!("  {}", v);
///     }
///     
///     println!("\nAvailable SDK versions:");
///     for v in &versions.sdk_versions {
///         println!("  {}", v);
///     }
///     Ok(())
/// }
/// ```
pub async fn list_available_versions() -> Result<AvailableVersions> {
    let manifest = VsManifest::fetch().await?;

    Ok(AvailableVersions {
        msvc_versions: manifest.list_msvc_versions(),
        sdk_versions: manifest.list_sdk_versions(),
        latest_msvc: manifest.get_latest_msvc_version(),
        latest_sdk: manifest.get_latest_sdk_version(),
    })
}
