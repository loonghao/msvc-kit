//! Bundle directory layout and path resolution
//!
//! Provides `BundleLayout` for discovering and accessing paths within a bundle.

use crate::env::{get_env_vars, MsvcEnvironment};
use crate::error::{MsvcKitError, Result};
use crate::version::Architecture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Bundle directory layout
///
/// Represents the structure of a portable MSVC toolchain bundle and provides
/// methods to access various paths within it.
///
/// # Directory Structure
///
/// ```text
/// {root}/
/// ├── VC/
/// │   └── Tools/
/// │       └── MSVC/
/// │           └── {msvc_version}/
/// │               ├── bin/Host{host_arch}/{target_arch}/
/// │               ├── include/
/// │               └── lib/{target_arch}/
/// └── Windows Kits/
///     └── 10/
///         ├── Include/{sdk_version}/
///         ├── Lib/{sdk_version}/
///         └── bin/{sdk_version}/
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleLayout {
    /// Root directory of the bundle
    pub root: PathBuf,
    /// MSVC version (e.g., "14.44.34823")
    pub msvc_version: String,
    /// Windows SDK version (e.g., "10.0.26100.0")
    pub sdk_version: String,
    /// Target architecture
    pub arch: Architecture,
    /// Host architecture
    pub host_arch: Architecture,
}

impl BundleLayout {
    /// Create a bundle layout from root directory by auto-discovering versions
    ///
    /// Scans the directory structure to find installed MSVC and SDK versions.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use msvc_kit::bundle::BundleLayout;
    ///
    /// let layout = BundleLayout::from_root("./msvc-bundle")?;
    /// println!("MSVC: {}, SDK: {}", layout.msvc_version, layout.sdk_version);
    /// # Ok::<(), msvc_kit::MsvcKitError>(())
    /// ```
    pub fn from_root<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root.as_ref().to_path_buf();

        // Discover MSVC version
        let msvc_tools_dir = root.join("VC").join("Tools").join("MSVC");
        let msvc_version = Self::discover_version(&msvc_tools_dir)?;

        // Discover SDK version
        let sdk_include_dir = root.join("Windows Kits").join("10").join("Include");
        let sdk_version = Self::discover_version(&sdk_include_dir)?;

        // Default to host architecture
        let arch = Architecture::host();
        let host_arch = Architecture::host();

        Ok(Self {
            root,
            msvc_version,
            sdk_version,
            arch,
            host_arch,
        })
    }

    /// Create a bundle layout with explicit versions
    pub fn from_root_with_versions<P: AsRef<Path>>(
        root: P,
        msvc_version: &str,
        sdk_version: &str,
        arch: Architecture,
        host_arch: Architecture,
    ) -> Result<Self> {
        Ok(Self {
            root: root.as_ref().to_path_buf(),
            msvc_version: msvc_version.to_string(),
            sdk_version: sdk_version.to_string(),
            arch,
            host_arch,
        })
    }

    /// Discover version from a directory containing version subdirectories
    fn discover_version(dir: &Path) -> Result<String> {
        if !dir.exists() {
            return Err(MsvcKitError::ComponentNotFound(format!(
                "Directory not found: {}",
                dir.display()
            )));
        }

        let mut versions: Vec<String> = std::fs::read_dir(dir)
            .map_err(MsvcKitError::Io)?
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    let path = e.path();
                    if path.is_dir() {
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
            })
            .filter(|name| {
                // Filter to version-like directories (start with digit)
                name.chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            })
            .collect();

        versions.sort();
        versions.pop().ok_or_else(|| {
            MsvcKitError::ComponentNotFound(format!("No version found in: {}", dir.display()))
        })
    }

    // ==================== VC Paths ====================

    /// Get VC installation directory
    ///
    /// Returns: `{root}/VC`
    pub fn vc_dir(&self) -> PathBuf {
        self.root.join("VC")
    }

    /// Get VC Tools installation directory
    ///
    /// Returns: `{root}/VC/Tools/MSVC/{version}`
    pub fn vc_tools_dir(&self) -> PathBuf {
        self.root
            .join("VC")
            .join("Tools")
            .join("MSVC")
            .join(&self.msvc_version)
    }

    /// Get VC include directory
    ///
    /// Returns: `{root}/VC/Tools/MSVC/{version}/include`
    pub fn vc_include_dir(&self) -> PathBuf {
        self.vc_tools_dir().join("include")
    }

    /// Get VC library directory for target architecture
    ///
    /// Returns: `{root}/VC/Tools/MSVC/{version}/lib/{arch}`
    pub fn vc_lib_dir(&self) -> PathBuf {
        self.vc_tools_dir().join("lib").join(self.arch.to_string())
    }

    /// Get VC binary directory
    ///
    /// Returns: `{root}/VC/Tools/MSVC/{version}/bin/Host{host}/{target}`
    pub fn vc_bin_dir(&self) -> PathBuf {
        self.vc_tools_dir()
            .join("bin")
            .join(self.host_arch.msvc_host_dir())
            .join(self.arch.msvc_target_dir())
    }

    // ==================== SDK Paths ====================

    /// Get Windows SDK root directory
    ///
    /// Returns: `{root}/Windows Kits/10`
    pub fn sdk_dir(&self) -> PathBuf {
        self.root.join("Windows Kits").join("10")
    }

    /// Get SDK include directory for a specific component
    ///
    /// Returns: `{root}/Windows Kits/10/Include/{version}/{component}`
    pub fn sdk_include_dir(&self, component: &str) -> PathBuf {
        self.sdk_dir()
            .join("Include")
            .join(&self.sdk_version)
            .join(component)
    }

    /// Get all SDK include directories
    pub fn sdk_include_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.sdk_include_dir("ucrt"),
            self.sdk_include_dir("shared"),
            self.sdk_include_dir("um"),
            self.sdk_include_dir("winrt"),
            self.sdk_include_dir("cppwinrt"),
        ]
    }

    /// Get SDK library directory for a specific component
    ///
    /// Returns: `{root}/Windows Kits/10/Lib/{version}/{component}/{arch}`
    pub fn sdk_lib_dir(&self, component: &str) -> PathBuf {
        self.sdk_dir()
            .join("Lib")
            .join(&self.sdk_version)
            .join(component)
            .join(self.arch.to_string())
    }

    /// Get all SDK library directories
    pub fn sdk_lib_dirs(&self) -> Vec<PathBuf> {
        vec![self.sdk_lib_dir("ucrt"), self.sdk_lib_dir("um")]
    }

    /// Get SDK binary directory
    ///
    /// Returns: `{root}/Windows Kits/10/bin/{version}/{arch}`
    pub fn sdk_bin_dir(&self) -> PathBuf {
        self.sdk_dir()
            .join("bin")
            .join(&self.sdk_version)
            .join(self.arch.to_string())
    }

    // ==================== Tool Paths ====================

    /// Get path to cl.exe (C/C++ compiler)
    pub fn cl_exe_path(&self) -> PathBuf {
        self.vc_bin_dir().join("cl.exe")
    }

    /// Get path to link.exe (linker)
    pub fn link_exe_path(&self) -> PathBuf {
        self.vc_bin_dir().join("link.exe")
    }

    /// Get path to lib.exe (static library manager)
    pub fn lib_exe_path(&self) -> PathBuf {
        self.vc_bin_dir().join("lib.exe")
    }

    /// Get path to nmake.exe
    pub fn nmake_exe_path(&self) -> PathBuf {
        self.vc_bin_dir().join("nmake.exe")
    }

    /// Get path to ml64.exe (MASM assembler for x64)
    pub fn ml64_exe_path(&self) -> PathBuf {
        self.vc_bin_dir().join("ml64.exe")
    }

    /// Get path to rc.exe (resource compiler)
    pub fn rc_exe_path(&self) -> PathBuf {
        self.sdk_bin_dir().join("rc.exe")
    }

    // ==================== Environment ====================

    /// Get all include paths
    pub fn include_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.vc_include_dir()];
        paths.extend(self.sdk_include_dirs());
        paths
    }

    /// Get all library paths
    pub fn lib_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.vc_lib_dir()];
        paths.extend(self.sdk_lib_dirs());
        paths
    }

    /// Get all binary paths
    pub fn bin_paths(&self) -> Vec<PathBuf> {
        vec![self.vc_bin_dir(), self.sdk_bin_dir()]
    }

    /// Get INCLUDE environment variable value
    pub fn include_env(&self) -> String {
        self.include_paths()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Get LIB environment variable value
    pub fn lib_env(&self) -> String {
        self.lib_paths()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Get PATH additions
    pub fn path_env(&self) -> String {
        self.bin_paths()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Convert to MsvcEnvironment for compatibility
    pub fn to_msvc_environment(&self) -> MsvcEnvironment {
        MsvcEnvironment {
            vc_install_dir: self.vc_dir(),
            vc_tools_install_dir: self.vc_tools_dir(),
            vc_tools_version: self.msvc_version.clone(),
            windows_sdk_dir: self.sdk_dir(),
            windows_sdk_version: self.sdk_version.clone(),
            include_paths: self.include_paths(),
            lib_paths: self.lib_paths(),
            bin_paths: self.bin_paths(),
            arch: self.arch,
            host_arch: self.host_arch,
        }
    }

    /// Get all environment variables as a HashMap
    pub fn env_vars(&self) -> HashMap<String, String> {
        get_env_vars(&self.to_msvc_environment())
    }

    /// Verify that the bundle is valid (all required paths exist)
    pub fn verify(&self) -> Result<()> {
        let required_paths = [
            ("VC Tools directory", self.vc_tools_dir()),
            ("VC include directory", self.vc_include_dir()),
            ("VC lib directory", self.vc_lib_dir()),
            ("VC bin directory", self.vc_bin_dir()),
            ("SDK directory", self.sdk_dir()),
        ];

        for (name, path) in required_paths {
            if !path.exists() {
                return Err(MsvcKitError::ComponentNotFound(format!(
                    "{} not found: {}",
                    name,
                    path.display()
                )));
            }
        }

        // Check for cl.exe
        let cl_path = self.cl_exe_path();
        if !cl_path.exists() {
            return Err(MsvcKitError::ComponentNotFound(format!(
                "cl.exe not found: {}",
                cl_path.display()
            )));
        }

        Ok(())
    }

    /// Export layout to JSON
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "root": self.root,
            "msvc_version": self.msvc_version,
            "sdk_version": self.sdk_version,
            "arch": self.arch.to_string(),
            "host_arch": self.host_arch.to_string(),
            "paths": {
                "vc_dir": self.vc_dir(),
                "vc_tools_dir": self.vc_tools_dir(),
                "vc_include_dir": self.vc_include_dir(),
                "vc_lib_dir": self.vc_lib_dir(),
                "vc_bin_dir": self.vc_bin_dir(),
                "sdk_dir": self.sdk_dir(),
                "sdk_bin_dir": self.sdk_bin_dir(),
            },
            "tools": {
                "cl": self.cl_exe_path(),
                "link": self.link_exe_path(),
                "lib": self.lib_exe_path(),
                "nmake": self.nmake_exe_path(),
                "rc": self.rc_exe_path(),
            },
            "env": {
                "INCLUDE": self.include_env(),
                "LIB": self.lib_env(),
                "PATH": self.path_env(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_layout_paths() {
        let layout = BundleLayout {
            root: PathBuf::from("C:/msvc-bundle"),
            msvc_version: "14.44.34823".to_string(),
            sdk_version: "10.0.26100.0".to_string(),
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        };

        assert_eq!(
            layout.vc_tools_dir(),
            PathBuf::from("C:/msvc-bundle/VC/Tools/MSVC/14.44.34823")
        );
        assert_eq!(
            layout.vc_bin_dir(),
            PathBuf::from("C:/msvc-bundle/VC/Tools/MSVC/14.44.34823/bin/Hostx64/x64")
        );
        assert_eq!(
            layout.sdk_bin_dir(),
            PathBuf::from("C:/msvc-bundle/Windows Kits/10/bin/10.0.26100.0/x64")
        );
    }

    #[test]
    fn test_bundle_layout_env() {
        let layout = BundleLayout {
            root: PathBuf::from("C:/msvc-bundle"),
            msvc_version: "14.44.34823".to_string(),
            sdk_version: "10.0.26100.0".to_string(),
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        };

        let include = layout.include_env();
        assert!(include.contains("VC"));
        assert!(include.contains("ucrt"));

        let lib = layout.lib_env();
        assert!(lib.contains("lib"));
    }
}
