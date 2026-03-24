//! # msvc-kit
//!
//! A portable MSVC Build Tools installer and manager for Rust development.
//!
//! This crate provides both a CLI tool and a library for downloading and managing
//! MSVC compiler and Windows SDK components without requiring a full Visual Studio installation.
//!
//! ## Features
//!
//! - Download MSVC compiler components from Microsoft servers
//! - Download Windows SDK to specified directories
//! - Configure environment variables for cc-rs compatibility
//! - Support version selection (defaults to latest)
//! - Generate activation scripts for shell environments
//! - Create portable bundles with all dependencies
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use msvc_kit::{download_msvc, extract_and_finalize_msvc, DownloadOptions, Architecture};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Use builder pattern for configuration
//!     let options = DownloadOptions::builder()
//!         .target_dir("C:/msvc-kit")
//!         .arch(Architecture::X64)
//!         .parallel_downloads(8)
//!         .build();
//!
//!     // Download MSVC packages
//!     let mut msvc_info = download_msvc(&options).await?;
//!     
//!     // Extract and finalize (determines full version number)
//!     extract_and_finalize_msvc(&mut msvc_info).await?;
//!     
//!     println!("Installed MSVC {} to: {:?}", msvc_info.version, msvc_info.install_path);
//!     Ok(())
//! }
//! ```
//!
//! ## Bundle Creation
//!
//! ```rust,no_run
//! use msvc_kit::bundle::{create_bundle, BundleOptions, BundleLayout};
//! use msvc_kit::Architecture;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create a portable bundle
//!     let options = BundleOptions {
//!         output_dir: "./msvc-bundle".into(),
//!         arch: Architecture::X64,
//!         ..Default::default()
//!     };
//!     
//!     let result = create_bundle(options).await?;
//!     println!("Bundle created with MSVC {}", result.msvc_info.version);
//!     
//!     // Later, discover an existing bundle
//!     let layout = BundleLayout::from_root("./msvc-bundle")?;
//!     println!("cl.exe at: {:?}", layout.cl_exe_path());
//!     Ok(())
//! }
//! ```
//!
//! ## Advanced Configuration
//!
//! ```rust,no_run
//! use msvc_kit::{DownloadOptions, Architecture};
//!
//! let options = DownloadOptions::builder()
//!     .msvc_version("14.44")           // Specific MSVC version
//!     .sdk_version("10.0.26100.0")     // Specific SDK version
//!     .target_dir("C:/msvc-kit")
//!     .arch(Architecture::X64)
//!     .host_arch(Architecture::X64)    // For cross-compilation
//!     .verify_hashes(true)
//!     .parallel_downloads(8)
//!     .dry_run(false)                  // Set to true for preview mode
//!     .build();
//! ```

pub mod bundle;
pub mod config;
pub mod constants;
pub mod downloader;
pub mod env;
pub mod error;
pub mod installer;
pub mod query;
pub mod scripts;
pub mod version;

// Re-export main types and functions
pub use config::{load_config, save_config, MsvcKitConfig};
pub use downloader::{
    download_all, download_msvc, download_sdk, list_available_versions, AvailableVersions,
    BoxedCacheManager, BoxedProgressHandler, CacheManager, ComponentDownloader, ComponentType,
    DownloadOptions, DownloadOptionsBuilder, FileSystemCacheManager, MsvcComponent,
    ProgressHandler,
};
pub use env::{get_env_vars, setup_environment, MsvcEnvironment, ToolPaths};
pub use error::{MsvcKitError, Result};
pub use installer::{extract_and_finalize_msvc, extract_and_finalize_sdk, InstallInfo};
pub use query::{
    query_installation, ComponentInfo, QueryComponent, QueryOptions, QueryOptionsBuilder,
    QueryProperty, QueryResult,
};
pub use scripts::{
    generate_absolute_scripts, generate_portable_scripts, generate_script, save_scripts,
    GeneratedScripts, ScriptContext, ShellType,
};
pub use version::{Architecture, MsvcVersion, SdkVersion};

// Re-export bundle types
pub use bundle::{create_bundle, discover_bundle, BundleLayout, BundleOptions, BundleResult};
