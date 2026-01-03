//! Version management for MSVC and Windows SDK

use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

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

/// Marker trait for version types
pub trait VersionType: Clone + Default {
    /// Get the component name for display
    fn component_name() -> &'static str;
}

/// Marker for MSVC versions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Msvc;

impl VersionType for Msvc {
    fn component_name() -> &'static str {
        "MSVC"
    }
}

/// Marker for SDK versions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sdk;

impl VersionType for Sdk {
    fn component_name() -> &'static str {
        "Windows SDK"
    }
}

/// Generic version information for components
///
/// This struct provides a unified representation for both MSVC and SDK versions,
/// reducing code duplication while maintaining type safety through the marker trait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version<T: VersionType> {
    /// Full version string (e.g., "14.40.33807" for MSVC or "10.0.22621.0" for SDK)
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

    #[serde(skip)]
    _marker: PhantomData<T>,
}

impl<T: VersionType> Version<T> {
    /// Create a new version
    pub fn new(version: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            display_name: display_name.into(),
            is_latest: false,
            install_path: None,
            download_url: None,
            size: None,
            sha256: None,
            _marker: PhantomData,
        }
    }

    /// Check if this version is installed
    pub fn is_installed(&self) -> bool {
        self.install_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Get the component name
    pub fn component_name(&self) -> &'static str {
        T::component_name()
    }
}

impl<T: VersionType> fmt::Display for Version<T> {
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

/// MSVC version information (type alias for backward compatibility)
pub type MsvcVersion = Version<Msvc>;

/// Windows SDK version information (type alias for backward compatibility)
pub type SdkVersion = Version<Sdk>;

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

/// Check if MSVC is installed at the given path with the specified version
pub fn is_msvc_installed(install_dir: &Path, version: &str) -> bool {
    let msvc_dir = install_dir.join("VC").join("Tools").join("MSVC");
    if !msvc_dir.exists() {
        return false;
    }

    // Check if the specific version directory exists
    let version_dir = msvc_dir.join(version);
    if version_dir.exists() {
        return true;
    }

    // Check if any version matching the prefix exists
    if let Ok(entries) = std::fs::read_dir(&msvc_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(version) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Check if Windows SDK is installed at the given path with the specified version
pub fn is_sdk_installed(install_dir: &Path, version: &str) -> bool {
    let sdk_dir = install_dir.join("Windows Kits").join("10").join("Include");
    if !sdk_dir.exists() {
        return false;
    }

    // Check if the specific version directory exists
    let version_dir = sdk_dir.join(version);
    if version_dir.exists() {
        return true;
    }

    // Check if any version matching the prefix exists (e.g., "26100" matches "10.0.26100.0")
    if let Ok(entries) = std::fs::read_dir(&sdk_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.contains(version) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// List all installed MSVC versions
pub fn list_installed_msvc(install_dir: &Path) -> Vec<MsvcVersion> {
    let msvc_dir = install_dir.join("VC").join("Tools").join("MSVC");

    if !msvc_dir.exists() {
        return Vec::new();
    }

    let mut versions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&msvc_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    let mut version = MsvcVersion::new(name, format!("MSVC {}", name));
                    version.install_path = Some(entry.path());
                    versions.push(version);
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
pub fn list_installed_sdk(install_dir: &Path) -> Vec<SdkVersion> {
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
                        let mut version = SdkVersion::new(name, format!("Windows SDK {}", name));
                        version.install_path = Some(install_dir.join("Windows Kits").join("10"));
                        versions.push(version);
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
        assert_eq!(
            "arm64".parse::<Architecture>().unwrap(),
            Architecture::Arm64
        );
    }

    #[test]
    fn test_msvc_host_dir() {
        assert_eq!(Architecture::X64.msvc_host_dir(), "Hostx64");
        assert_eq!(Architecture::X86.msvc_host_dir(), "Hostx86");
    }

    #[test]
    fn test_version_generic() {
        let msvc = MsvcVersion::new("14.40.33807", "MSVC 14.40");
        assert_eq!(msvc.component_name(), "MSVC");
        assert!(!msvc.is_installed());

        let sdk = SdkVersion::new("10.0.22621.0", "Windows SDK 10.0.22621");
        assert_eq!(sdk.component_name(), "Windows SDK");
        assert!(!sdk.is_installed());
    }
}
