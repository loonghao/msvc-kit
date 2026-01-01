//! Version management for MSVC and Windows SDK

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Target architecture for MSVC tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    /// 64-bit x86 (AMD64)
    #[default]
    X64,
    /// 32-bit x86
    X86,
    /// ARM64
    Arm64,
    /// ARM (32-bit)
    Arm,
}

impl fmt::Display for Architecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Architecture::X64 => write!(f, "x64"),
            Architecture::X86 => write!(f, "x86"),
            Architecture::Arm64 => write!(f, "arm64"),
            Architecture::Arm => write!(f, "arm"),
        }
    }
}

impl std::str::FromStr for Architecture {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "x64" | "amd64" | "x86_64" => Ok(Architecture::X64),
            "x86" | "i686" | "i386" => Ok(Architecture::X86),
            "arm64" | "aarch64" => Ok(Architecture::Arm64),
            "arm" => Ok(Architecture::Arm),
            _ => Err(format!("Unknown architecture: {}", s)),
        }
    }
}

impl Architecture {
    /// Get the host architecture for the current system
    pub fn host() -> Self {
        #[cfg(target_arch = "x86_64")]
        return Architecture::X64;
        #[cfg(target_arch = "x86")]
        return Architecture::X86;
        #[cfg(target_arch = "aarch64")]
        return Architecture::Arm64;
        #[cfg(target_arch = "arm")]
        return Architecture::Arm;
        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "x86",
            target_arch = "aarch64",
            target_arch = "arm"
        )))]
        return Architecture::X64; // Default fallback
    }

    /// Get the MSVC host directory name
    pub fn msvc_host_dir(&self) -> &'static str {
        match self {
            Architecture::X64 => "Hostx64",
            Architecture::X86 => "Hostx86",
            Architecture::Arm64 => "Hostarm64",
            Architecture::Arm => "Hostarm",
        }
    }

    /// Get the MSVC target directory name
    pub fn msvc_target_dir(&self) -> &'static str {
        match self {
            Architecture::X64 => "x64",
            Architecture::X86 => "x86",
            Architecture::Arm64 => "arm64",
            Architecture::Arm => "arm",
        }
    }
}

/// MSVC version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsvcVersion {
    /// Full version string (e.g., "14.40.33807")
    pub version: String,

    /// Display name
    pub display_name: String,

    /// Whether this is the latest version
    pub is_latest: bool,

    /// Installation path (if installed)
    pub install_path: Option<PathBuf>,

    /// Download URL
    pub download_url: Option<String>,

    /// Package size in bytes
    pub size: Option<u64>,

    /// SHA256 hash for verification
    pub sha256: Option<String>,
}

impl MsvcVersion {
    /// Check if this version is installed
    pub fn is_installed(&self) -> bool {
        self.install_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
    }
}

impl fmt::Display for MsvcVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.version)?;
        if self.is_latest {
            write!(f, " (latest)")?;
        }
        if self.is_installed() {
            write!(f, " [installed]")?;
        }
        Ok(())
    }
}

/// Windows SDK version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkVersion {
    /// Full version string (e.g., "10.0.22621.0")
    pub version: String,

    /// Display name
    pub display_name: String,

    /// Whether this is the latest version
    pub is_latest: bool,

    /// Installation path (if installed)
    pub install_path: Option<PathBuf>,

    /// Download URL
    pub download_url: Option<String>,

    /// Package size in bytes
    pub size: Option<u64>,

    /// SHA256 hash for verification
    pub sha256: Option<String>,
}

impl SdkVersion {
    /// Check if this version is installed
    pub fn is_installed(&self) -> bool {
        self.install_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
    }
}

impl fmt::Display for SdkVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.version)?;
        if self.is_latest {
            write!(f, " (latest)")?;
        }
        if self.is_installed() {
            write!(f, " [installed]")?;
        }
        Ok(())
    }
}

/// Installed version record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledVersion {
    /// MSVC version
    pub msvc: Option<MsvcVersion>,

    /// Windows SDK version
    pub sdk: Option<SdkVersion>,

    /// Installation timestamp
    pub installed_at: chrono::DateTime<chrono::Utc>,

    /// Architecture
    pub arch: Architecture,
}

/// List all installed MSVC versions
pub fn list_installed_msvc(install_dir: &PathBuf) -> Vec<MsvcVersion> {
    let msvc_dir = install_dir.join("VC").join("Tools").join("MSVC");

    if !msvc_dir.exists() {
        return Vec::new();
    }

    let mut versions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&msvc_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    versions.push(MsvcVersion {
                        version: name.to_string(),
                        display_name: format!("MSVC {}", name),
                        is_latest: false,
                        install_path: Some(entry.path()),
                        download_url: None,
                        size: None,
                        sha256: None,
                    });
                }
            }
        }
    }

    // Sort by version descending
    versions.sort_by(|a, b| b.version.cmp(&a.version));

    // Mark the first one as latest
    if let Some(first) = versions.first_mut() {
        first.is_latest = true;
    }

    versions
}

/// List all installed Windows SDK versions
pub fn list_installed_sdk(install_dir: &PathBuf) -> Vec<SdkVersion> {
    let sdk_dir = install_dir.join("Windows Kits").join("10").join("Include");

    if !sdk_dir.exists() {
        return Vec::new();
    }

    let mut versions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&sdk_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("10.0.") {
                        versions.push(SdkVersion {
                            version: name.to_string(),
                            display_name: format!("Windows SDK {}", name),
                            is_latest: false,
                            install_path: Some(install_dir.join("Windows Kits").join("10")),
                            download_url: None,
                            size: None,
                            sha256: None,
                        });
                    }
                }
            }
        }
    }

    // Sort by version descending
    versions.sort_by(|a, b| b.version.cmp(&a.version));

    // Mark the first one as latest
    if let Some(first) = versions.first_mut() {
        first.is_latest = true;
    }

    versions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_display() {
        assert_eq!(Architecture::X64.to_string(), "x64");
        assert_eq!(Architecture::X86.to_string(), "x86");
        assert_eq!(Architecture::Arm64.to_string(), "arm64");
    }

    #[test]
    fn test_architecture_parse() {
        assert_eq!("x64".parse::<Architecture>().unwrap(), Architecture::X64);
        assert_eq!("amd64".parse::<Architecture>().unwrap(), Architecture::X64);
        assert_eq!("x86".parse::<Architecture>().unwrap(), Architecture::X86);
        assert_eq!("arm64".parse::<Architecture>().unwrap(), Architecture::Arm64);
    }

    #[test]
    fn test_msvc_host_dir() {
        assert_eq!(Architecture::X64.msvc_host_dir(), "Hostx64");
        assert_eq!(Architecture::X86.msvc_host_dir(), "Hostx86");
    }
}
