//! Bundle creation and management for portable MSVC toolchain
//!
//! This module provides APIs to create self-contained MSVC toolchain bundles
//! that can be used anywhere without installation.
//!
//! # Directory Structure
//!
//! The bundle follows a structure similar to msvc-wine for compatibility:
//!
//! ```text
//! {bundle_root}/
//! ├── VC/
//! │   └── Tools/
//! │       └── MSVC/
//! │           └── {version}/          # e.g., 14.44.34823
//! │               ├── bin/
//! │               │   └── Host{arch}/
//! │               │       └── {arch}/ # cl.exe, link.exe, etc.
//! │               ├── include/
//! │               └── lib/
//! │                   └── {arch}/
//! └── Windows Kits/
//!     └── 10/
//!         ├── Include/
//!         │   └── {sdk_version}/      # e.g., 10.0.26100.0
//!         │       ├── ucrt/
//!         │       ├── shared/
//!         │       ├── um/
//!         │       ├── winrt/
//!         │       └── cppwinrt/
//!         ├── Lib/
//!         │   └── {sdk_version}/
//!         │       ├── ucrt/{arch}/
//!         │       └── um/{arch}/
//!         └── bin/
//!             └── {sdk_version}/
//!                 └── {arch}/         # rc.exe, etc.
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use msvc_kit::bundle::{BundleLayout, BundleOptions, create_bundle};
//! use msvc_kit::Architecture;
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create a bundle
//!     let options = BundleOptions {
//!         output_dir: PathBuf::from("./msvc-bundle"),
//!         arch: Architecture::X64,
//!         host_arch: Architecture::X64,
//!         msvc_version: None,  // Use latest
//!         sdk_version: None,   // Use latest
//!         parallel_downloads: 8,
//!     };
//!     
//!     let result = create_bundle(options).await?;
//!     println!("Bundle created: {:?}", result.layout.root);
//!     
//!     // Get paths from existing bundle
//!     let layout = BundleLayout::from_root("./msvc-bundle")?;
//!     println!("cl.exe: {:?}", layout.cl_exe_path());
//!     
//!     Ok(())
//! }
//! ```

mod layout;
pub mod scripts;

pub use layout::BundleLayout;
pub use scripts::{generate_bundle_scripts, save_bundle_scripts, BundleScripts};

use crate::downloader::{download_msvc, download_sdk, DownloadOptions};
use crate::error::{MsvcKitError, Result};
use crate::installer::{install_msvc, install_sdk, InstallInfo};
use crate::version::Architecture;
use std::path::{Path, PathBuf};

/// Options for creating a bundle
#[derive(Debug, Clone)]
pub struct BundleOptions {
    /// Output directory for the bundle
    pub output_dir: PathBuf,
    /// Target architecture
    pub arch: Architecture,
    /// Host architecture (defaults to current system)
    pub host_arch: Architecture,
    /// MSVC version (None = latest)
    pub msvc_version: Option<String>,
    /// SDK version (None = latest)
    pub sdk_version: Option<String>,
    /// Number of parallel downloads
    pub parallel_downloads: usize,
}

impl Default for BundleOptions {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("./msvc-bundle"),
            arch: Architecture::X64,
            host_arch: Architecture::host(),
            msvc_version: None,
            sdk_version: None,
            parallel_downloads: 8,
        }
    }
}

/// Result of bundle creation
#[derive(Debug, Clone)]
pub struct BundleResult {
    /// Bundle layout with all paths
    pub layout: BundleLayout,
    /// MSVC installation info
    pub msvc_info: InstallInfo,
    /// SDK installation info
    pub sdk_info: InstallInfo,
    /// Generated scripts
    pub scripts: BundleScripts,
}

/// Create a portable MSVC toolchain bundle
///
/// Downloads MSVC and Windows SDK components and organizes them into
/// a portable bundle structure.
///
/// # Arguments
///
/// * `options` - Bundle creation options
///
/// # Returns
///
/// Returns `BundleResult` containing the layout and installation info.
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::bundle::{create_bundle, BundleOptions};
/// use msvc_kit::Architecture;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let options = BundleOptions {
///         output_dir: "./my-bundle".into(),
///         arch: Architecture::X64,
///         ..Default::default()
///     };
///     
///     let result = create_bundle(options).await?;
///     println!("MSVC version: {}", result.msvc_info.version);
///     Ok(())
/// }
/// ```
pub async fn create_bundle(options: BundleOptions) -> Result<BundleResult> {
    // Create output directory
    tokio::fs::create_dir_all(&options.output_dir)
        .await
        .map_err(|e| MsvcKitError::Io(e))?;

    // Download options - download directly to bundle root
    let download_opts = DownloadOptions {
        msvc_version: options.msvc_version.clone(),
        sdk_version: options.sdk_version.clone(),
        target_dir: options.output_dir.clone(),
        arch: options.arch,
        host_arch: Some(options.host_arch),
        verify_hashes: true,
        parallel_downloads: options.parallel_downloads,
        http_client: None,
        progress_handler: None,
        dry_run: false,
    };

    // Download and install MSVC
    let msvc_info = download_msvc(&download_opts).await?;
    install_msvc(&msvc_info).await?;

    // Download and install SDK
    let sdk_info = download_sdk(&download_opts).await?;
    install_sdk(&sdk_info).await?;

    // Create bundle layout from the installed files
    let layout = BundleLayout::from_root_with_versions(
        &options.output_dir,
        &msvc_info.version,
        &sdk_info.version,
        options.arch,
        options.host_arch,
    )?;

    // Generate activation scripts
    let scripts = generate_bundle_scripts(&layout)?;

    Ok(BundleResult {
        layout,
        msvc_info,
        sdk_info,
        scripts,
    })
}

/// Discover an existing bundle from a root directory
///
/// Scans the directory to find MSVC and SDK versions automatically.
///
/// # Arguments
///
/// * `root` - Bundle root directory
///
/// # Returns
///
/// Returns `BundleLayout` if a valid bundle is found.
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::bundle::discover_bundle;
///
/// let layout = discover_bundle("./msvc-bundle")?;
/// println!("Found MSVC {}", layout.msvc_version);
/// # Ok::<(), msvc_kit::MsvcKitError>(())
/// ```
pub fn discover_bundle<P: AsRef<Path>>(root: P) -> Result<BundleLayout> {
    BundleLayout::from_root(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_options_default() {
        let opts = BundleOptions::default();
        assert_eq!(opts.arch, Architecture::X64);
        assert_eq!(opts.parallel_downloads, 8);
    }
}
