//! Installation and extraction functionality

mod extractor;

use futures::{stream, StreamExt};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use crate::constants::{extraction as ext_const, progress as progress_const};
use crate::error::Result;
use crate::version::Architecture;

pub use extractor::{extract_cab, extract_msi, extract_vsix, get_extractor};
use extractor::{
    extract_cab_with_progress, extract_msi_with_progress, extract_vsix_with_progress,
    inner_progress_enabled,
};

const SDK_OPTIONAL_MSI_SKIP_PATTERNS: &[&str] = &["application verifier", "winrt intellisense"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SdkExtractionAction {
    Extract,
    KeepSourceMedia,
    SkipOptional,
    SkipUnsupported,
}

fn classify_sdk_payload_for_extraction(file: &Path) -> SdkExtractionAction {
    let extension = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "msi" => {
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if SDK_OPTIONAL_MSI_SKIP_PATTERNS
                .iter()
                .any(|pattern| name.contains(pattern))
            {
                SdkExtractionAction::SkipOptional
            } else {
                SdkExtractionAction::Extract
            }
        }
        "cab" => SdkExtractionAction::KeepSourceMedia,
        _ => SdkExtractionAction::SkipUnsupported,
    }
}

fn sdk_payloads_to_extract(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut extractable = Vec::new();
    let mut source_media = 0usize;
    let mut optional = 0usize;
    let mut unsupported = 0usize;

    for file in files {
        match classify_sdk_payload_for_extraction(file) {
            SdkExtractionAction::Extract => extractable.push(file.clone()),
            SdkExtractionAction::KeepSourceMedia => source_media += 1,
            SdkExtractionAction::SkipOptional => {
                optional += 1;
                tracing::info!(
                    "Skipping optional Windows SDK MSI that is not required for MSVC toolchains: {:?}",
                    file
                );
            }
            SdkExtractionAction::SkipUnsupported => unsupported += 1,
        }
    }

    tracing::info!(
        "Windows SDK extraction plan: {} MSI files, {} source CAB files retained, {} optional MSI files skipped, {} unsupported payloads skipped",
        extractable.len(),
        source_media,
        optional,
        unsupported
    );

    extractable
}

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

async fn extraction_marker_path(marker_dir: &Path, file: &Path) -> PathBuf {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let metadata = tokio::fs::metadata(file).await.ok();
    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
    let modified = metadata
        .and_then(|m| m.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    let mut hasher = Sha256::new();
    hasher.update(file.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(size.to_le_bytes());
    hasher.update(b"\0");
    hasher.update(modified.to_le_bytes());
    let fingerprint = hex::encode(hasher.finalize());

    marker_dir.join(format!("{}-{}.done", name, &fingerprint[..16]))
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
        let marker = extraction_marker_path(&marker_dir, file).await;

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

    let results: Vec<Result<PathBuf>> = stream::iter(files_to_extract)
        .map(|file| {
            let target_dir = target_dir.clone();
            let marker_dir = marker_dir.clone();
            let extracted_count = extracted_count.clone();
            let skipped_count = skipped_count.clone();
            let pb = pb.clone();
            let label = label.clone();
            let total = total as usize;

            async move {
                let marker = extraction_marker_path(&marker_dir, &file).await;

                // Extract the package
                extract_package_with_progress(&file, &target_dir, false).await?;

                // Mark as extracted
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

    // SDK payloads include bootstrap EXEs and source CAB media. The CAB files must
    // stay beside their MSI files for msiexec, but they are not standalone archives
    // to unpack. Some optional SDK MSIs also fail administrative extraction while
    // being unnecessary for MSVC/Rust toolchains.
    let sdk_files = sdk_payloads_to_extract(&info.downloaded_files);
    extract_packages_with_progress(&sdk_files, target_dir, "Windows SDK").await?;

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
    let sdk_files = sdk_payloads_to_extract(&info.downloaded_files);
    extract_packages_with_progress(&sdk_files, &info.install_path, "Windows SDK").await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_test_zip(path: &Path, entry_name: &str, contents: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file(entry_name, options).unwrap();
        std::io::Write::write_all(&mut zip, contents).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn sdk_extraction_keeps_cabs_and_skips_optional_msi_payloads() {
        let payloads = vec![
            PathBuf::from("downloads/sdk/26100_arm64/winsdksetup.exe"),
            PathBuf::from("downloads/sdk/26100_arm64/Installers/source.cab"),
            PathBuf::from("downloads/sdk/26100_arm64/Installers/Application Verifier arm64 External Package (DesktopEditions)-arm64_en-us.msi"),
            PathBuf::from("downloads/sdk/26100_arm64/Installers/WinRT Intellisense UAP - en-us-x86_en-us.msi"),
            PathBuf::from("downloads/sdk/26100_arm64/Installers/Windows SDK Desktop Headers arm64-x86_en-us.msi"),
        ];

        assert_eq!(
            classify_sdk_payload_for_extraction(&payloads[0]),
            SdkExtractionAction::SkipUnsupported
        );
        assert_eq!(
            classify_sdk_payload_for_extraction(&payloads[1]),
            SdkExtractionAction::KeepSourceMedia
        );
        assert_eq!(
            classify_sdk_payload_for_extraction(&payloads[2]),
            SdkExtractionAction::SkipOptional
        );
        assert_eq!(
            classify_sdk_payload_for_extraction(&payloads[3]),
            SdkExtractionAction::SkipOptional
        );
        assert_eq!(
            classify_sdk_payload_for_extraction(&payloads[4]),
            SdkExtractionAction::Extract
        );

        let extractable = sdk_payloads_to_extract(&payloads);
        assert_eq!(extractable, vec![payloads[4].clone()]);
    }

    #[tokio::test]
    async fn install_sdk_legacy_path_uses_sdk_payload_filter() {
        let temp = TempDir::new().unwrap();
        let install_path = temp.path().join("sdk");
        let info = InstallInfo {
            component_type: "sdk".to_string(),
            version: "10.0.26100.0".to_string(),
            install_path: install_path.clone(),
            downloaded_files: vec![
                temp.path().join("Installers").join("source.cab"),
                temp.path().join("Installers").join(
                    "Application Verifier x64 External Package (DesktopEditions)-x64_en-us.msi",
                ),
                temp.path()
                    .join("Installers")
                    .join("WinRT Intellisense UAP - en-us-x86_en-us.msi"),
            ],
            arch: Architecture::X64,
        };

        let installed = install_sdk(&info).await.unwrap();

        assert_eq!(installed, install_path);
        assert!(installed.exists());
    }

    #[tokio::test]
    async fn extraction_markers_include_source_fingerprint() {
        let temp = TempDir::new().unwrap();
        let first_dir = temp.path().join("downloads").join("sdk-1");
        let second_dir = temp.path().join("downloads").join("sdk-2");
        let target = temp.path().join("install");
        std::fs::create_dir_all(&first_dir).unwrap();
        std::fs::create_dir_all(&second_dir).unwrap();

        let first_zip = first_dir.join("shared-name.zip");
        let second_zip = second_dir.join("shared-name.zip");
        write_test_zip(&first_zip, "first.txt", b"first");
        write_test_zip(&second_zip, "second.txt", b"second");

        extract_packages_with_progress(&[first_zip], &target, "test")
            .await
            .unwrap();
        extract_packages_with_progress(&[second_zip], &target, "test")
            .await
            .unwrap();

        assert!(target.join("first.txt").exists());
        assert!(target.join("second.txt").exists());

        let markers = std::fs::read_dir(target.join(".msvc-kit-extracted"))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("shared-name.zip-")
            })
            .count();
        assert_eq!(markers, 2);
    }
}
