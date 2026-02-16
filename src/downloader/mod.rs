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

use std::collections::HashSet;
use std::path::PathBuf;

use crate::error::Result;
use crate::installer::InstallInfo;
use crate::version::Architecture;

/// Optional MSVC component categories that can be included in downloads.
///
/// By default, only the core toolchain (Tools, CRT, MFC, ATL, ASAN) is downloaded.
/// Use this enum to opt-in to additional component categories like
/// Spectre-mitigated libraries, C++/CLI support, or UWP support.
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::{DownloadOptions, MsvcComponent};
///
/// let options = DownloadOptions::builder()
///     .include_component(MsvcComponent::Spectre)
///     .include_component(MsvcComponent::Cli)
///     .build();
/// ```
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum MsvcComponent {
    /// Spectre-mitigated CRT/ATL/MFC libraries
    /// Required for builds with `/Qspectre` flag (e.g., node-pty/winpty)
    Spectre,
    /// Microsoft Foundation Classes
    /// Required for legacy MFC applications
    Mfc,
    /// Active Template Library
    /// Required for COM/ATL components
    Atl,
    /// Address Sanitizer libraries
    /// Required for debugging with ASAN
    Asan,
    /// UWP/Store libraries
    /// Required for UWP app development
    Uwp,
    /// C++/CLI support libraries
    /// Required for mixed managed/native C++/CLI projects
    /// (VS Component: Microsoft.VisualStudio.Component.VC.CLI.Support)
    Cli,
    /// C++ Standard Library Modules (experimental)
    /// Required for C++20/23 `import std;` support
    /// (VS Component: Microsoft.VisualStudio.Component.VC.Modules.x86.x64)
    Modules,
    /// C++ Redistributable packages
    /// Required for distributing C++ applications
    /// (VS Component: Microsoft.VisualStudio.Component.VC.Redist.14.Latest)
    Redist,
    /// Custom package ID pattern for future extensibility
    /// Matches packages containing the specified string (case-insensitive)
    Custom(String),
}

impl std::fmt::Display for MsvcComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MsvcComponent::Spectre => write!(f, "spectre"),
            MsvcComponent::Mfc => write!(f, "mfc"),
            MsvcComponent::Atl => write!(f, "atl"),
            MsvcComponent::Asan => write!(f, "asan"),
            MsvcComponent::Uwp => write!(f, "uwp"),
            MsvcComponent::Cli => write!(f, "cli"),
            MsvcComponent::Modules => write!(f, "modules"),
            MsvcComponent::Redist => write!(f, "redist"),
            MsvcComponent::Custom(s) => write!(f, "custom:{}", s),
        }
    }
}

impl std::str::FromStr for MsvcComponent {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "spectre" => Ok(MsvcComponent::Spectre),
            "mfc" => Ok(MsvcComponent::Mfc),
            "atl" => Ok(MsvcComponent::Atl),
            "asan" => Ok(MsvcComponent::Asan),
            "uwp" | "store" => Ok(MsvcComponent::Uwp),
            "cli" | "c++/cli" => Ok(MsvcComponent::Cli),
            "modules" => Ok(MsvcComponent::Modules),
            "redist" | "redistributable" => Ok(MsvcComponent::Redist),
            other => {
                if let Some(pattern) = other.strip_prefix("custom:") {
                    Ok(MsvcComponent::Custom(pattern.to_string()))
                } else {
                    Err(format!(
                        "Unknown component '{}'. Valid: spectre, mfc, atl, asan, uwp, cli, modules, redist, custom:<pattern>",
                        s
                    ))
                }
            }
        }
    }
}

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

    /// Additional MSVC components to include (default: empty = standard install).
    ///
    /// By default, the standard toolchain (Tools, CRT, MFC, ATL) is downloaded.
    /// Use this to opt-in to additional components like Spectre-mitigated libraries.
    ///
    /// See [`MsvcComponent`] for available component categories.
    pub include_components: HashSet<MsvcComponent>,

    /// Package ID patterns to exclude (case-insensitive substring match).
    ///
    /// Any package whose ID contains one of these patterns will be excluded
    /// from the download, providing fine-grained control over package selection.
    pub exclude_patterns: Vec<String>,
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
            .field("include_components", &self.include_components)
            .field("exclude_patterns", &self.exclude_patterns)
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

        // Parse MSVC_KIT_INCLUDE_COMPONENTS env var (comma-separated)
        let include_components = std::env::var("MSVC_KIT_INCLUDE_COMPONENTS")
            .ok()
            .map(|s| {
                s.split(',')
                    .filter_map(|c| c.trim().parse::<MsvcComponent>().ok())
                    .collect()
            })
            .unwrap_or_default();

        // Parse MSVC_KIT_EXCLUDE_PATTERNS env var (comma-separated)
        let exclude_patterns = std::env::var("MSVC_KIT_EXCLUDE_PATTERNS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default();

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
            include_components,
            exclude_patterns,
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

    /// Include an optional MSVC component category.
    ///
    /// Components like Spectre-mitigated libraries are excluded by default.
    /// Use this to opt-in to additional component categories.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use msvc_kit::{DownloadOptions, MsvcComponent};
    ///
    /// let options = DownloadOptions::builder()
    ///     .include_component(MsvcComponent::Spectre)
    ///     .build();
    /// ```
    pub fn include_component(mut self, component: MsvcComponent) -> Self {
        self.options.include_components.insert(component);
        self
    }

    /// Include multiple optional MSVC component categories at once.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use msvc_kit::{DownloadOptions, MsvcComponent};
    ///
    /// let options = DownloadOptions::builder()
    ///     .include_components([MsvcComponent::Spectre, MsvcComponent::Asan])
    ///     .build();
    /// ```
    pub fn include_components(
        mut self,
        components: impl IntoIterator<Item = MsvcComponent>,
    ) -> Self {
        self.options.include_components.extend(components);
        self
    }

    /// Exclude packages matching a pattern (case-insensitive substring match).
    ///
    /// Any package whose ID contains the pattern will be excluded from download.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use msvc_kit::DownloadOptions;
    ///
    /// let options = DownloadOptions::builder()
    ///     .exclude_pattern(".uwp")
    ///     .exclude_pattern(".store")
    ///     .build();
    /// ```
    pub fn exclude_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.options.exclude_patterns.push(pattern.into());
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
