//! Configuration management for msvc-kit

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::Result;
use crate::version::Architecture;

/// Main configuration structure for msvc-kit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsvcKitConfig {
    /// Base installation directory for all MSVC components
    pub install_dir: PathBuf,

    /// Default MSVC version to use (None = latest)
    pub default_msvc_version: Option<String>,

    /// Default Windows SDK version to use (None = latest)
    pub default_sdk_version: Option<String>,

    /// Default architecture
    pub default_arch: Architecture,

    /// Whether to verify file hashes after download
    pub verify_hashes: bool,

    /// Number of parallel downloads
    pub parallel_downloads: usize,

    /// Cache directory for downloaded packages
    pub cache_dir: Option<PathBuf>,
}

impl Default for MsvcKitConfig {
    fn default() -> Self {
        let base_dir = get_default_install_dir();
        Self {
            install_dir: base_dir.clone(),
            default_msvc_version: None,
            default_sdk_version: None,
            default_arch: Architecture::X64,
            verify_hashes: true,
            parallel_downloads: 4,
            cache_dir: Some(base_dir.join("cache")),
        }
    }
}

/// Get the default installation directory
fn get_default_install_dir() -> PathBuf {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "loonghao", "msvc-kit") {
        proj_dirs.data_local_dir().to_path_buf()
    } else {
        // Fallback to user's home directory
        dirs_fallback()
    }
}

fn dirs_fallback() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\msvc-kit"))
            .join("msvc-kit")
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
            .join(".msvc-kit")
    }
}

/// Get the configuration file path (TOML format)
fn get_config_path() -> PathBuf {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "loonghao", "msvc-kit") {
        proj_dirs.config_dir().join("config.toml")
    } else {
        get_default_install_dir().join("config.toml")
    }
}

/// Get the legacy JSON configuration file path (for migration)
fn get_legacy_config_path() -> PathBuf {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "loonghao", "msvc-kit") {
        proj_dirs.config_dir().join("config.json")
    } else {
        get_default_install_dir().join("config.json")
    }
}

/// Load configuration from disk
///
/// If the configuration file doesn't exist, returns default configuration.
/// Supports automatic migration from legacy JSON format to TOML.
pub fn load_config() -> Result<MsvcKitConfig> {
    let config_path = get_config_path();

    // Try to load TOML config first
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let config: MsvcKitConfig = toml::from_str(&content)?;
        return Ok(config);
    }

    // Try to migrate from legacy JSON config
    let legacy_path = get_legacy_config_path();
    if legacy_path.exists() {
        let content = std::fs::read_to_string(&legacy_path)?;
        let config: MsvcKitConfig = serde_json::from_str(&content)?;

        // Save as TOML for future use
        let _ = save_config(&config);

        // Remove legacy JSON file after successful migration
        let _ = std::fs::remove_file(&legacy_path);

        tracing::info!(
            "Migrated configuration from {} to {}",
            legacy_path.display(),
            config_path.display()
        );

        return Ok(config);
    }

    Ok(MsvcKitConfig::default())
}

/// Save configuration to disk (TOML format)
pub fn save_config(config: &MsvcKitConfig) -> Result<()> {
    let config_path = get_config_path();

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(config)?;
    std::fs::write(&config_path, content)?;
    Ok(())
}

/// Get the installation directory for a specific MSVC version
pub fn get_msvc_install_dir(config: &MsvcKitConfig, version: &str) -> PathBuf {
    config
        .install_dir
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join(version)
}

/// Get the installation directory for a specific Windows SDK version
pub fn get_sdk_install_dir(config: &MsvcKitConfig, version: &str) -> PathBuf {
    config
        .install_dir
        .join("Windows Kits")
        .join("10")
        .join(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MsvcKitConfig::default();
        assert!(config.verify_hashes);
        assert_eq!(config.parallel_downloads, 4);
        assert_eq!(config.default_arch, Architecture::X64);
    }

    #[test]
    fn test_config_toml_serialization() {
        let config = MsvcKitConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();

        // Verify it contains expected TOML keys
        assert!(toml_str.contains("install_dir"));
        assert!(toml_str.contains("verify_hashes"));
        assert!(toml_str.contains("parallel_downloads"));

        // Verify round-trip
        let parsed: MsvcKitConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.verify_hashes, config.verify_hashes);
        assert_eq!(parsed.parallel_downloads, config.parallel_downloads);
    }
}
