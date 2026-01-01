//! File extraction utilities for VSIX, MSI, and CAB files

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::error::{MsvcKitError, Result};

/// Extract a VSIX file (which is a ZIP archive)
pub async fn extract_vsix(vsix_path: &Path, target_dir: &Path) -> Result<()> {
    let vsix_path = vsix_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        extract_vsix_sync(&vsix_path, &target_dir)
    })
    .await
    .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

fn extract_vsix_sync(vsix_path: &Path, target_dir: &Path) -> Result<()> {
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
        let relative_path = name
            .strip_prefix("Contents/")
            .unwrap_or(&name);

        let out_path = target_dir.join(relative_path);

        if file.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut out_file = File::create(&out_path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            out_file.write_all(&buffer)?;
        }
    }

    Ok(())
}

/// Extract an MSI file
///
/// On Windows, uses msiexec. On other platforms, attempts to use msitools.
pub async fn extract_msi(msi_path: &Path, target_dir: &Path) -> Result<()> {
    let msi_path = msi_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        extract_msi_sync(&msi_path, &target_dir)
    })
    .await
    .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

fn extract_msi_sync(msi_path: &Path, target_dir: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::process::Command;

        // Use msiexec to extract MSI contents
        let status = Command::new("msiexec")
            .args([
                "/a",
                msi_path.to_str().ok_or_else(|| MsvcKitError::Other("Invalid MSI path".to_string()))?,
                "/qn",
                &format!("TARGETDIR={}", target_dir.display()),
            ])
            .status()?;

        if !status.success() {
            return Err(MsvcKitError::Other(format!(
                "msiexec failed with status: {}",
                status
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
                target_dir.to_str().ok_or_else(|| MsvcKitError::Other("Invalid target path".to_string()))?,
                msi_path.to_str().ok_or_else(|| MsvcKitError::Other("Invalid MSI path".to_string()))?,
            ])
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                return Err(MsvcKitError::Other(format!(
                    "msiextract failed with status: {}",
                    s
                )));
            }
            Err(e) => {
                return Err(MsvcKitError::Other(format!(
                    "Failed to run msiextract (is msitools installed?): {}",
                    e
                )));
            }
        }
    }

    Ok(())
}

/// Extract a CAB file
pub async fn extract_cab(cab_path: &Path, target_dir: &Path) -> Result<()> {
    let cab_path = cab_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        extract_cab_sync(&cab_path, &target_dir)
    })
    .await
    .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

fn extract_cab_sync(cab_path: &Path, target_dir: &Path) -> Result<()> {
    let file = File::open(cab_path)?;
    let cabinet = cab::Cabinet::new(file)
        .map_err(|e| MsvcKitError::Cab(format!("Failed to open CAB: {}", e)))?;

    // Collect file names first
    let file_names: Vec<String> = cabinet.folder_entries()
        .flat_map(|folder| folder.file_entries())
        .map(|entry| entry.name().to_string())
        .collect();

    // Extract each file
    for name in file_names {
        let out_path = target_dir.join(&name);
        
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Re-open cabinet to read the file
        let file = File::open(cab_path)?;
        let mut cabinet = cab::Cabinet::new(file)
            .map_err(|e| MsvcKitError::Cab(format!("Failed to open CAB: {}", e)))?;
        
        let mut reader = cabinet.read_file(&name)
            .map_err(|e| MsvcKitError::Cab(format!("Failed to read file {}: {}", name, e)))?;
        
        let mut out_file = File::create(&out_path)?;
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)
            .map_err(|e| MsvcKitError::Cab(format!("Failed to read file content: {}", e)))?;
        out_file.write_all(&buffer)?;
    }

    Ok(())
}

/// Determine the extraction method based on file extension
pub fn get_extractor(path: &Path) -> Option<fn(&Path, &Path) -> Result<()>> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    
    match extension.as_str() {
        "vsix" | "zip" => Some(|p, t| {
            tokio::runtime::Handle::current().block_on(extract_vsix(p, t))
        }),
        "msi" => Some(|p, t| {
            tokio::runtime::Handle::current().block_on(extract_msi(p, t))
        }),
        "cab" => Some(|p, t| {
            tokio::runtime::Handle::current().block_on(extract_cab(p, t))
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_get_extractor() {
        assert!(get_extractor(Path::new("test.vsix")).is_some());
        assert!(get_extractor(Path::new("test.msi")).is_some());
        assert!(get_extractor(Path::new("test.cab")).is_some());
        assert!(get_extractor(Path::new("test.unknown")).is_none());
    }
}
