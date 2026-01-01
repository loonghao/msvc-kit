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
//! ## Library Usage
//!
//! ```rust,no_run
//! use msvc_kit::{download_msvc, DownloadOptions, Architecture};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let options = DownloadOptions {
//!         msvc_version: None, // Use latest
//!         sdk_version: None,  // Use latest
//!         target_dir: std::path::PathBuf::from("C:\\msvc-kit"),
//!         arch: Architecture::X64,
//!         ..Default::default()
//!     };
//!
//!     let install_info = download_msvc(&options).await?;
//!     println!("Installed MSVC to: {:?}", install_info.install_path);
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod downloader;
pub mod env;
pub mod error;
pub mod installer;
pub mod version;

// Re-export main types and functions
pub use config::{MsvcKitConfig, load_config, save_config};
pub use downloader::{DownloadOptions, download_msvc, download_sdk};
pub use env::{MsvcEnvironment, setup_environment, get_env_vars, generate_activation_script};
pub use error::{MsvcKitError, Result};
pub use installer::InstallInfo;
pub use version::{Architecture, MsvcVersion, SdkVersion};
