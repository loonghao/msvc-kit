//! File extraction utilities for VSIX, MSI, and CAB files

use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::constants::{extraction as ext_const, progress as progress_const};
use crate::error::{MsvcKitError, Result};

/// Global mutex for MSI extraction.
/// Windows Installer (msiexec) can only run one instance at a time globally.
/// Error 1618 = "Another installation is already in progress"
static MSI_EXTRACT_LOCK: Mutex<()> = Mutex::new(());

/// Maximum retries for MSI extraction when encountering error 1618
const MSI_MAX_RETRIES: u32 = 5;
/// Delay between retries in milliseconds
const MSI_RETRY_DELAY_MS: u64 = 2000;

pub(crate) fn inner_progress_enabled() -> bool {
    matches!(
        env::var("MSVC_KIT_INNER_PROGRESS")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(crate) fn progress_style_bytes() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] {wide_bar:.cyan/blue} {bytes}/{total_bytes} @ {bytes_per_sec} ETA {eta} | {msg}")
        .unwrap()
        .progress_chars("##-")
}

pub(crate) fn progress_style_items() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] {wide_bar:.cyan/blue} {pos}/{len} files | {msg}",
        )
        .unwrap()
        .progress_chars("##-")
}

/// Extract a VSIX file (which is a ZIP archive) with optional progress bar
pub(crate) async fn extract_vsix_with_progress(
    vsix_path: &Path,
    target_dir: &Path,
    show_progress: bool,
) -> Result<()> {
    let vsix_path = vsix_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || extract_vsix_sync(&vsix_path, &target_dir, show_progress))
        .await
        .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

/// Extract a VSIX file (which is a ZIP archive) with progress bar
pub async fn extract_vsix(vsix_path: &Path, target_dir: &Path) -> Result<()> {
    extract_vsix_with_progress(vsix_path, target_dir, inner_progress_enabled()).await
}

fn extract_vsix_sync(vsix_path: &Path, target_dir: &Path, show_progress: bool) -> Result<()> {
    // Pre-compute total bytes for progress bar (skip metadata files)
    let total_bytes = {
        let file = File::open(vsix_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut total = 0u64;
        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name();
            if name.starts_with('[') || name == "extension.vsixmanifest" || file.is_dir() {
                continue;
            }
            total = total.saturating_add(file.size());
        }
        total
    };

    let pb = if show_progress {
        let pb = ProgressBar::new(total_bytes.max(1));
        pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(4));
        pb.set_style(progress_style_bytes());
        pb.set_message(
            vsix_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "extracting".to_string()),
        );
        Some(pb)
    } else {
        None
    };

    let file = File::open(vsix_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();

        // Skip metadata files
        if name.starts_with('[') || name == "extension.vsixmanifest" {
            continue;
        }

        // Remove "Contents/" prefix if present
        let relative_path = name.strip_prefix("Contents/").unwrap_or(&name);
        let out_path = target_dir.join(relative_path);

        if let Some(pb) = pb.as_ref() {
            pb.set_message(relative_path.to_string());
        }

        if file.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out_file = File::create(&out_path)?;
        let mut buffer = [0u8; ext_const::EXTRACT_BUFFER_SIZE];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            out_file.write_all(&buffer[..n])?;
            if let Some(pb) = pb.as_ref() {
                pb.inc(n as u64);
            }
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message("Extracted");
    }
    Ok(())
}

/// Extract an MSI file
///
/// On Windows, uses msiexec. On other platforms, attempts to use msitools.
pub(crate) async fn extract_msi_with_progress(
    msi_path: &Path,
    target_dir: &Path,
    show_progress: bool,
) -> Result<()> {
    let msi_path = msi_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || extract_msi_sync(&msi_path, &target_dir, show_progress))
        .await
        .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

pub async fn extract_msi(msi_path: &Path, target_dir: &Path) -> Result<()> {
    extract_msi_with_progress(msi_path, target_dir, inner_progress_enabled()).await
}

fn extract_msi_sync(msi_path: &Path, target_dir: &Path, show_progress: bool) -> Result<()> {
    let file_name = msi_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.msi")
        .to_string();

    let pb = if show_progress {
        let pb = ProgressBar::new_spinner();
        pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(4));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap()
                .tick_chars("⠁⠃⠇⠋⠙⠸⠴⠦"),
        );
        pb.set_message(format!("msiexec extracting {}", file_name));
        pb.enable_steady_tick(Duration::from_millis(progress_const::PROGRESS_TICK_MS));
        Some(pb)
    } else {
        None
    };

    // Acquire global MSI lock to prevent concurrent msiexec invocations.
    // Windows Installer can only run one instance at a time (error 1618).
    let _lock = MSI_EXTRACT_LOCK
        .lock()
        .map_err(|e| MsvcKitError::Other(format!("Failed to acquire MSI lock: {}", e)))?;

    #[cfg(windows)]
    {
        use std::process::Command;

        let msi_path_str = msi_path
            .to_str()
            .ok_or_else(|| MsvcKitError::Other("Invalid MSI path".to_string()))?;
        let target_dir_str = format!("TARGETDIR={}", target_dir.display());

        // Retry loop for handling error 1618 (another installation in progress)
        // This can happen if system Windows Installer is busy with other operations
        let mut last_error = None;
        for attempt in 1..=MSI_MAX_RETRIES {
            let status = Command::new("msiexec")
                .args(["/a", msi_path_str, "/qn", &target_dir_str])
                .status()?;

            if status.success() {
                if let Some(pb) = pb {
                    pb.finish_with_message(format!("MSI extracted: {}", file_name));
                }
                return Ok(());
            }

            // Check for error 1618 (another installation in progress)
            // This can still happen if system-level installers are running
            if let Some(code) = status.code() {
                if code == 1618 && attempt < MSI_MAX_RETRIES {
                    tracing::warn!(
                        "msiexec returned 1618 (another installation in progress) for {}, retry {}/{}",
                        file_name,
                        attempt,
                        MSI_MAX_RETRIES
                    );
                    if let Some(pb) = pb.as_ref() {
                        pb.set_message(format!(
                            "msiexec waiting (retry {}/{}) {}",
                            attempt, MSI_MAX_RETRIES, file_name
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(MSI_RETRY_DELAY_MS));
                    continue;
                }
            }

            last_error = Some(status);
            break;
        }

        if let Some(status) = last_error {
            if let Some(pb) = pb.as_ref() {
                pb.abandon_with_message(format!("msiexec failed: {}", file_name));
            }
            return Err(MsvcKitError::Other(format!(
                "msiexec failed with status: {} for {}",
                status, file_name
            )));
        }
    }

    #[cfg(not(windows))]
    {
        // On non-Windows, try using msitools (msiextract)
        use std::process::Command;

        let status = Command::new("msiextract")
            .args([
                "-C",
                target_dir
                    .to_str()
                    .ok_or_else(|| MsvcKitError::Other("Invalid target path".to_string()))?,
                msi_path
                    .to_str()
                    .ok_or_else(|| MsvcKitError::Other("Invalid MSI path".to_string()))?,
            ])
            .status();

        match status {
            Ok(s) if s.success() => {
                if let Some(pb) = pb {
                    pb.finish_with_message(format!("MSI extracted: {}", file_name));
                }
                return Ok(());
            }
            Ok(s) => {
                if let Some(pb) = pb.as_ref() {
                    pb.abandon_with_message("msiextract failed");
                }
                return Err(MsvcKitError::Other(format!(
                    "msiextract failed with status: {}",
                    s
                )));
            }
            Err(e) => {
                if let Some(pb) = pb.as_ref() {
                    pb.abandon_with_message("msiextract failed");
                }
                return Err(MsvcKitError::Other(format!(
                    "Failed to run msiextract (is msitools installed?): {}",
                    e
                )));
            }
        }
    }

    #[cfg(windows)]
    {
        if let Some(pb) = pb {
            pb.finish_with_message(format!("MSI extracted: {}", file_name));
        }
        Ok(())
    }
}

/// Extract a CAB file with a simple file-count progress bar
pub(crate) async fn extract_cab_with_progress(
    cab_path: &Path,
    target_dir: &Path,
    show_progress: bool,
) -> Result<()> {
    let cab_path = cab_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || extract_cab_sync(&cab_path, &target_dir, show_progress))
        .await
        .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

pub async fn extract_cab(cab_path: &Path, target_dir: &Path) -> Result<()> {
    extract_cab_with_progress(cab_path, target_dir, inner_progress_enabled()).await
}

fn extract_cab_sync(cab_path: &Path, target_dir: &Path, show_progress: bool) -> Result<()> {
    let file = File::open(cab_path)?;
    let cabinet = cab::Cabinet::new(file)
        .map_err(|e| MsvcKitError::Cab(format!("Failed to open CAB: {}", e)))?;

    // Collect file names first by iterating through folders
    let file_names: Vec<String> = cabinet
        .folder_entries()
        .flat_map(|folder| folder.file_entries())
        .map(|entry| entry.name().to_string())
        .collect();

    let total_files = file_names.len() as u64;
    let pb = if show_progress {
        let pb = ProgressBar::new(total_files.max(1));
        pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(4));
        pb.set_style(progress_style_items());
        pb.set_message(
            cab_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Extracting CAB".to_string()),
        );
        Some(pb)
    } else {
        None
    };

    // Re-open cabinet for extraction (cab crate requires this pattern)
    // Note: The cab crate's API requires re-opening for each file read.
    // This is a limitation of the crate, not an efficiency issue we can fix here.
    // A future optimization would be to use a different CAB library or implement
    // streaming extraction.
    for (idx, name) in file_names.iter().enumerate() {
        let out_path = target_dir.join(name);

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if let Some(pb) = pb.as_ref() {
            pb.set_message(format!("{} ({}/{})", name, idx + 1, total_files));
        }

        // Re-open cabinet to read the file (cab crate limitation)
        let file = File::open(cab_path)?;
        let mut cabinet = cab::Cabinet::new(file)
            .map_err(|e| MsvcKitError::Cab(format!("Failed to open CAB: {}", e)))?;

        let mut reader = cabinet
            .read_file(name)
            .map_err(|e| MsvcKitError::Cab(format!("Failed to read file {}: {}", name, e)))?;

        let mut out_file = File::create(&out_path)?;
        let mut buffer = [0u8; ext_const::EXTRACT_BUFFER_SIZE];
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|e| MsvcKitError::Cab(format!("Failed to read file content: {}", e)))?;
            if n == 0 {
                break;
            }
            out_file.write_all(&buffer[..n])?;
        }

        if let Some(pb) = pb.as_ref() {
            pb.inc(1);
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message("CAB extracted");
    }
    Ok(())
}

/// Determine the extraction method based on file extension
pub fn get_extractor(path: &Path) -> Option<fn(&Path, &Path) -> Result<()>> {
    let extension = path.extension()?.to_str()?.to_lowercase();

    match extension.as_str() {
        "vsix" | "zip" => {
            Some(|p, t| tokio::runtime::Handle::current().block_on(extract_vsix(p, t)))
        }
        "msi" => Some(|p, t| tokio::runtime::Handle::current().block_on(extract_msi(p, t))),
        "cab" => Some(|p, t| tokio::runtime::Handle::current().block_on(extract_cab(p, t))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(unused_imports)]
    use tempfile::TempDir;

    #[test]
    fn test_get_extractor() {
        assert!(get_extractor(Path::new("test.vsix")).is_some());
        assert!(get_extractor(Path::new("test.msi")).is_some());
        assert!(get_extractor(Path::new("test.cab")).is_some());
        assert!(get_extractor(Path::new("test.unknown")).is_none());
    }
}
