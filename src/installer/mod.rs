//! Installation and extraction functionality

mod extractor;

use futures::{stream, StreamExt};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::constants::{extraction as ext_const, progress as progress_const};
use crate::error::Result;
use crate::version::Architecture;

pub use extractor::{extract_cab, extract_msi, extract_vsix, get_extractor};
use extractor::{
    extract_cab_with_progress, extract_msi_with_progress, extract_vsix_with_progress,
    inner_progress_enabled,
};

/// Extract a package based on its file extension
pub async fn extract_package(file: &Path, target_dir: &Path) -> Result<()> {
    extract_package_with_progress(file, target_dir, inner_progress_enabled()).await
}

async fn extract_package_with_progress(
    file: &Path,
    target_dir: &Path,
    show_progress: bool,
) -> Result<()> {
    let extension = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "vsix" | "zip" => extract_vsix_with_progress(file, target_dir, show_progress).await,
        "msi" => extract_msi_with_progress(file, target_dir, show_progress).await,
        "cab" => extract_cab_with_progress(file, target_dir, show_progress).await,
        _ => {
            tracing::warn!("Unknown file type: {:?}, skipping extraction", file);
            Ok(())
        }
    }
}

/// Extract multiple packages with a unified progress bar (parallel extraction)
pub async fn extract_packages_with_progress(
    files: &[PathBuf],
    target_dir: &Path,
    label: &str,
) -> Result<()> {
    let total = files.len() as u64;
    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(4));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap()
            .tick_chars("⠁⠃⠇⠋⠙⠸⠴⠦"),
    );
    pb.enable_steady_tick(Duration::from_millis(progress_const::PROGRESS_TICK_MS));
    pb.set_message(format!("{} extracting 0/{} files", label, total));

    // cache marker dir
    let marker_dir = target_dir.join(".msvc-kit-extracted");
    tokio::fs::create_dir_all(&marker_dir).await.ok();

    // Determine parallel extraction count (use CPU cores, capped by constant)
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let parallel_count = num_cpus.min(ext_const::DEFAULT_PARALLEL_EXTRACTIONS);

    // Counters for progress tracking
    let extracted_count = Arc::new(AtomicUsize::new(0));
    let skipped_count = Arc::new(AtomicUsize::new(0));

    // Filter files that need extraction (not cached)
    let mut files_to_extract = Vec::new();
    let mut cached_files = Vec::new();

    for file in files.iter() {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let marker = marker_dir.join(format!("{}.done", name));

        if marker.exists() {
            cached_files.push(file.clone());
        } else {
            files_to_extract.push(file.clone());
        }
    }

    // Update progress for cached files
    let cached_count = cached_files.len();
    if cached_count > 0 {
        skipped_count.fetch_add(cached_count, Ordering::Relaxed);
        pb.set_message(format!(
            "{} extracting {}/{} (skipped {} cached)",
            label,
            0,
            files_to_extract.len(),
            cached_count
        ));
    }

    // Extract files in parallel
    let target_dir = target_dir.to_path_buf();
    let label = label.to_string();
    let pb = Arc::new(pb);

    let results: Vec<Result<PathBuf>> = stream::iter(files_to_extract.into_iter())
        .map(|file| {
            let target_dir = target_dir.clone();
            let marker_dir = marker_dir.clone();
            let extracted_count = extracted_count.clone();
            let skipped_count = skipped_count.clone();
            let pb = pb.clone();
            let label = label.clone();
            let total = total as usize;

            async move {
                let name = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Extract the package
                extract_package_with_progress(&file, &target_dir, false).await?;

                // Mark as extracted
                let marker = marker_dir.join(format!("{}.done", name));
                let _ = tokio::fs::write(&marker, b"ok").await;

                // Update progress
                let done = extracted_count.fetch_add(1, Ordering::Relaxed) + 1;
                let skip = skipped_count.load(Ordering::Relaxed);
                pb.set_message(format!(
                    "{} extracting {}/{} (done {}, cached {})",
                    label,
                    done + skip,
                    total,
                    done,
                    skip
                ));

                Ok(file)
            }
        })
        .buffer_unordered(parallel_count)
        .collect()
        .await;

    // Check for errors
    for result in results {
        result?;
    }

    let final_extracted = extracted_count.load(Ordering::Relaxed);
    let final_skipped = skipped_count.load(Ordering::Relaxed);
    pb.finish_with_message(format!(
        "{} extraction done ({} extracted, {} cached)",
        label, final_extracted, final_skipped
    ));
    Ok(())
}

/// Information about an installed component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallInfo {
    /// Component type (msvc, sdk)
    pub component_type: String,

    /// Installed version
    pub version: String,

    /// Installation path
    pub install_path: PathBuf,

    /// List of downloaded files
    pub downloaded_files: Vec<PathBuf>,

    /// Target architecture
    pub arch: Architecture,
}

impl InstallInfo {
    /// Check if the installation is valid
    pub fn is_valid(&self) -> bool {
        self.install_path.exists()
    }

    /// Get the total size of downloaded files
    pub fn total_size(&self) -> u64 {
        self.downloaded_files
            .iter()
            .filter_map(|p| p.metadata().ok())
            .map(|m| m.len())
            .sum()
    }

    /// Get the bin directory for this component
    pub fn bin_dir(&self) -> PathBuf {
        match self.component_type.as_str() {
            "msvc" => {
                let host_dir = self.arch.msvc_host_dir();
                let target_dir = self.arch.msvc_target_dir();
                self.install_path
                    .join("bin")
                    .join(host_dir)
                    .join(target_dir)
            }
            "sdk" => self
                .install_path
                .join("bin")
                .join(&self.version)
                .join(self.arch.to_string()),
            _ => self.install_path.join("bin"),
        }
    }

    /// Get the include directory for this component
    pub fn include_dir(&self) -> PathBuf {
        match self.component_type.as_str() {
            "msvc" => self.install_path.join("include"),
            "sdk" => self.install_path.join("Include").join(&self.version),
            _ => self.install_path.join("include"),
        }
    }

    /// Get the lib directory for this component
    pub fn lib_dir(&self) -> PathBuf {
        match self.component_type.as_str() {
            "msvc" => self.install_path.join("lib").join(self.arch.to_string()),
            "sdk" => self
                .install_path
                .join("Lib")
                .join(&self.version)
                .join("um")
                .join(self.arch.to_string()),
            _ => self.install_path.join("lib"),
        }
    }

    /// Export install info to JSON
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "component_type": self.component_type,
            "version": self.version,
            "install_path": self.install_path,
            "bin_dir": self.bin_dir(),
            "include_dir": self.include_dir(),
            "lib_dir": self.lib_dir(),
            "arch": self.arch.to_string(),
            "is_valid": self.is_valid(),
            "total_size": self.total_size(),
        })
    }
}

/// Extract MSVC packages and finalize InstallInfo with actual version
///
/// This function:
/// 1. Extracts downloaded packages to the target directory
/// 2. Scans for the MSVC version directory to get the full version number
/// 3. Updates InstallInfo with the complete version and correct paths
pub async fn extract_and_finalize_msvc(info: &mut InstallInfo) -> Result<()> {
    let target_dir = &info.install_path;

    tracing::info!("Extracting MSVC packages to {:?}", target_dir);

    // Extract all packages
    extract_packages_with_progress(&info.downloaded_files, target_dir, "MSVC").await?;

    // Find the actual MSVC version directory and extract the full version number
    let vc_tools_path = target_dir.join("VC").join("Tools").join("MSVC");
    if vc_tools_path.exists() {
        // Find the version directory - this contains the full version number (e.g., 14.44.34823)
        let mut entries = tokio::fs::read_dir(&vc_tools_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let dir_name = entry.file_name();
                if let Some(name) = dir_name.to_str() {
                    // The directory name is the full version (e.g., "14.44.34823")
                    info.version = name.to_string();
                    tracing::info!(
                        "Found MSVC version directory: {} (full version: {})",
                        entry.path().display(),
                        info.version
                    );
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Extract SDK packages and finalize InstallInfo
///
/// This function:
/// 1. Extracts downloaded packages to the target directory
/// 2. Verifies the SDK installation path
pub async fn extract_and_finalize_sdk(info: &InstallInfo) -> Result<()> {
    let target_dir = &info.install_path;

    tracing::info!("Extracting Windows SDK packages to {:?}", target_dir);

    // Extract all packages
    extract_packages_with_progress(&info.downloaded_files, target_dir, "Windows SDK").await?;

    Ok(())
}

/// Install MSVC components from downloaded files
///
/// This is a legacy function that extracts packages to install_path.
/// For new code, use extract_and_finalize_msvc() instead.
pub async fn install_msvc(info: &InstallInfo) -> Result<PathBuf> {
    tracing::info!(
        "Installing MSVC {} to {:?}",
        info.version,
        info.install_path
    );

    tokio::fs::create_dir_all(&info.install_path).await?;
    extract_packages_with_progress(&info.downloaded_files, &info.install_path, "MSVC").await?;

    Ok(info.install_path.clone())
}

/// Install Windows SDK components from downloaded files
///
/// This is a legacy function that extracts packages to install_path.
/// For new code, use extract_and_finalize_sdk() instead.
pub async fn install_sdk(info: &InstallInfo) -> Result<PathBuf> {
    tracing::info!(
        "Installing Windows SDK {} to {:?}",
        info.version,
        info.install_path
    );

    tokio::fs::create_dir_all(&info.install_path).await?;
    extract_packages_with_progress(&info.downloaded_files, &info.install_path, "SDK").await?;

    Ok(info.install_path.clone())
}

/// Clean up downloaded files after installation
pub async fn cleanup_downloads(info: &InstallInfo) -> Result<()> {
    for file in &info.downloaded_files {
        if file.exists() {
            tokio::fs::remove_file(file).await?;
        }
    }
    Ok(())
}
