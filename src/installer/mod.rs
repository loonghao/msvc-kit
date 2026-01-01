//! Installation and extraction functionality

mod extractor;

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::version::Architecture;

pub use extractor::{extract_vsix, extract_msi, extract_cab};

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
}

/// Install MSVC components from downloaded files
pub async fn install_msvc(info: &InstallInfo) -> Result<PathBuf> {
    tracing::info!("Installing MSVC {} to {:?}", info.version, info.install_path);

    // Create installation directory
    tokio::fs::create_dir_all(&info.install_path).await?;

    // Extract each downloaded file
    for file in &info.downloaded_files {
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        match extension.to_lowercase().as_str() {
            "vsix" => {
                extract_vsix(file, &info.install_path).await?;
            }
            "msi" => {
                extract_msi(file, &info.install_path).await?;
            }
            "cab" => {
                extract_cab(file, &info.install_path).await?;
            }
            _ => {
                tracing::warn!("Unknown file type: {:?}", file);
            }
        }
    }

    Ok(info.install_path.clone())
}

/// Install Windows SDK components from downloaded files
pub async fn install_sdk(info: &InstallInfo) -> Result<PathBuf> {
    tracing::info!("Installing Windows SDK {} to {:?}", info.version, info.install_path);

    // Create installation directory
    tokio::fs::create_dir_all(&info.install_path).await?;

    // Extract each downloaded file
    for file in &info.downloaded_files {
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        match extension.to_lowercase().as_str() {
            "vsix" => {
                extract_vsix(file, &info.install_path).await?;
            }
            "msi" => {
                extract_msi(file, &info.install_path).await?;
            }
            "cab" => {
                extract_cab(file, &info.install_path).await?;
            }
            _ => {
                tracing::warn!("Unknown file type: {:?}", file);
            }
        }
    }

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
