//! Environment variable configuration for MSVC toolchain
//!
//! This module handles setting up environment variables required for
//! the MSVC toolchain to work correctly, including compatibility with
//! Rust's cc-rs crate.

mod setup;

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::installer::InstallInfo;
use crate::version::Architecture;

pub use setup::{setup_environment, apply_environment, generate_activation_script, ShellType};

#[cfg(windows)]
pub use setup::write_to_registry;

/// MSVC environment configuration
///
/// Contains all the paths and environment variables needed for the
/// MSVC toolchain to function correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsvcEnvironment {
    /// Visual C++ installation directory (VCINSTALLDIR)
    pub vc_install_dir: PathBuf,

    /// VC Tools installation directory (VCToolsInstallDir)
    pub vc_tools_install_dir: PathBuf,

    /// VC Tools version (VCToolsVersion)
    pub vc_tools_version: String,

    /// Windows SDK directory (WindowsSdkDir)
    pub windows_sdk_dir: PathBuf,

    /// Windows SDK version (WindowsSDKVersion)
    pub windows_sdk_version: String,

    /// Include paths for compiler
    pub include_paths: Vec<PathBuf>,

    /// Library paths for linker
    pub lib_paths: Vec<PathBuf>,

    /// Binary paths (for cl.exe, link.exe, etc.)
    pub bin_paths: Vec<PathBuf>,

    /// Target architecture
    pub arch: Architecture,

    /// Host architecture
    pub host_arch: Architecture,
}

impl MsvcEnvironment {
    /// Create a new MSVC environment from install info
    pub fn from_install_info(
        msvc_info: &InstallInfo,
        sdk_info: Option<&InstallInfo>,
        host_arch: Architecture,
    ) -> Result<Self> {
        let base_dir = msvc_info.install_path.parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| msvc_info.install_path.clone());

        let vc_install_dir = base_dir.join("VC");
        let vc_tools_install_dir = msvc_info.install_path.clone();
        let vc_tools_version = msvc_info.version.clone();

        let (windows_sdk_dir, windows_sdk_version) = if let Some(sdk) = sdk_info {
            (sdk.install_path.clone(), sdk.version.clone())
        } else {
            // Default SDK paths
            (base_dir.join("Windows Kits").join("10"), "10.0.22621.0".to_string())
        };

        let arch = msvc_info.arch;

        // Build include paths
        let include_paths = Self::build_include_paths(
            &vc_tools_install_dir,
            &windows_sdk_dir,
            &windows_sdk_version,
        );

        // Build library paths
        let lib_paths = Self::build_lib_paths(
            &vc_tools_install_dir,
            &windows_sdk_dir,
            &windows_sdk_version,
            arch,
        );

        // Build binary paths
        let bin_paths = Self::build_bin_paths(
            &vc_tools_install_dir,
            &windows_sdk_dir,
            &windows_sdk_version,
            host_arch,
            arch,
        );

        Ok(Self {
            vc_install_dir,
            vc_tools_install_dir,
            vc_tools_version,
            windows_sdk_dir,
            windows_sdk_version,
            include_paths,
            lib_paths,
            bin_paths,
            arch,
            host_arch,
        })
    }

    /// Build include paths
    fn build_include_paths(
        vc_tools_dir: &PathBuf,
        sdk_dir: &PathBuf,
        sdk_version: &str,
    ) -> Vec<PathBuf> {
        vec![
            // MSVC includes
            vc_tools_dir.join("include"),
            // Windows SDK includes
            sdk_dir.join("Include").join(sdk_version).join("ucrt"),
            sdk_dir.join("Include").join(sdk_version).join("shared"),
            sdk_dir.join("Include").join(sdk_version).join("um"),
            sdk_dir.join("Include").join(sdk_version).join("winrt"),
            sdk_dir.join("Include").join(sdk_version).join("cppwinrt"),
        ]
    }

    /// Build library paths
    fn build_lib_paths(
        vc_tools_dir: &PathBuf,
        sdk_dir: &PathBuf,
        sdk_version: &str,
        arch: Architecture,
    ) -> Vec<PathBuf> {
        let arch_str = arch.to_string();
        vec![
            // MSVC libs
            vc_tools_dir.join("lib").join(&arch_str),
            // Windows SDK libs
            sdk_dir.join("Lib").join(sdk_version).join("ucrt").join(&arch_str),
            sdk_dir.join("Lib").join(sdk_version).join("um").join(&arch_str),
        ]
    }

    /// Build binary paths
    fn build_bin_paths(
        vc_tools_dir: &PathBuf,
        sdk_dir: &PathBuf,
        sdk_version: &str,
        host_arch: Architecture,
        target_arch: Architecture,
    ) -> Vec<PathBuf> {
        let host_dir = host_arch.msvc_host_dir();
        let target_dir = target_arch.msvc_target_dir();

        vec![
            // MSVC binaries
            vc_tools_dir.join("bin").join(host_dir).join(target_dir),
            // Windows SDK binaries
            sdk_dir.join("bin").join(sdk_version).join(target_arch.to_string()),
        ]
    }

    /// Check if cl.exe is available in the configured paths
    pub fn has_cl_exe(&self) -> bool {
        self.bin_paths.iter().any(|p| p.join("cl.exe").exists())
    }

    /// Get the path to cl.exe
    pub fn cl_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("cl.exe"))
            .find(|p| p.exists())
    }

    /// Get the path to link.exe
    pub fn link_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("link.exe"))
            .find(|p| p.exists())
    }
}

/// Get environment variables as a HashMap
///
/// Returns all environment variables needed for MSVC toolchain,
/// formatted for use with cc-rs and other build tools.
pub fn get_env_vars(env: &MsvcEnvironment) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    // Visual Studio environment variables
    vars.insert("VCINSTALLDIR".to_string(), env.vc_install_dir.display().to_string());
    vars.insert("VCToolsInstallDir".to_string(), env.vc_tools_install_dir.display().to_string());
    vars.insert("VCToolsVersion".to_string(), env.vc_tools_version.clone());

    // Windows SDK environment variables
    vars.insert("WindowsSdkDir".to_string(), env.windows_sdk_dir.display().to_string());
    vars.insert("WindowsSDKVersion".to_string(), format!("{}\\", env.windows_sdk_version));
    vars.insert("WindowsSdkBinPath".to_string(), 
        env.windows_sdk_dir.join("bin").join(&env.windows_sdk_version).display().to_string());

    // INCLUDE path
    let include = env.include_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(";");
    vars.insert("INCLUDE".to_string(), include);

    // LIB path
    let lib = env.lib_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(";");
    vars.insert("LIB".to_string(), lib);

    // PATH additions
    let path = env.bin_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(";");
    vars.insert("PATH".to_string(), path);

    // Platform information
    vars.insert("Platform".to_string(), env.arch.to_string());
    vars.insert("VSCMD_ARG_HOST_ARCH".to_string(), env.host_arch.to_string());
    vars.insert("VSCMD_ARG_TGT_ARCH".to_string(), env.arch.to_string());

    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_env_vars() {
        let env = MsvcEnvironment {
            vc_install_dir: PathBuf::from("C:\\VC"),
            vc_tools_install_dir: PathBuf::from("C:\\VC\\Tools\\MSVC\\14.40"),
            vc_tools_version: "14.40.33807".to_string(),
            windows_sdk_dir: PathBuf::from("C:\\Windows Kits\\10"),
            windows_sdk_version: "10.0.22621.0".to_string(),
            include_paths: vec![PathBuf::from("C:\\include")],
            lib_paths: vec![PathBuf::from("C:\\lib")],
            bin_paths: vec![PathBuf::from("C:\\bin")],
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        };

        let vars = get_env_vars(&env);
        assert!(vars.contains_key("VCINSTALLDIR"));
        assert!(vars.contains_key("INCLUDE"));
        assert!(vars.contains_key("LIB"));
        assert!(vars.contains_key("PATH"));
    }
}
