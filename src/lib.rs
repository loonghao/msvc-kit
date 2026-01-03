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
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use msvc_kit::{download_msvc, DownloadOptions, Architecture};
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
//!     let install_info = download_msvc(&options).await?;
//!     println!("Installed MSVC to: {:?}", install_info.install_path);
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

pub mod config;
pub mod constants;
pub mod downloader;
pub mod env;
pub mod error;
pub mod installer;
pub mod version;

// Re-export main types and functions
pub use config::{load_config, save_config, MsvcKitConfig};
pub use downloader::{
    download_msvc, download_sdk, BoxedCacheManager, BoxedProgressHandler, CacheManager,
    ComponentDownloader, ComponentType, DownloadOptions, DownloadOptionsBuilder,
    FileSystemCacheManager, ProgressHandler,
};
pub use env::{
    generate_activation_script, get_env_vars, setup_environment, MsvcEnvironment, ShellType,
    ToolPaths,
};
pub use error::{MsvcKitError, Result};
pub use installer::InstallInfo;
pub use version::{Architecture, MsvcVersion, SdkVersion};
