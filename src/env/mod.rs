//! Environment variable configuration for MSVC toolchain
//!
//! This module handles setting up environment variables required for
//! the MSVC toolchain to work correctly, including compatibility with
//! Rust's cc-rs crate.

mod setup;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::installer::InstallInfo;
use crate::version::Architecture;

pub use setup::{
    apply_environment, generate_activation_script, generate_all_activation_scripts,
    save_activation_script, setup_environment,
};

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

/// Translate Windows verbatim paths to the ordinary absolute spelling accepted
/// by MSVC's INCLUDE/LIB parser and downstream build tools.
pub(crate) fn compiler_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{}", unc));
        }
        if let Some(drive) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(drive);
        }
    }
    path.to_path_buf()
}

impl MsvcEnvironment {
    /// Create a new MSVC environment from install info
    pub fn from_install_info(
        msvc_info: &InstallInfo,
        sdk_info: Option<&InstallInfo>,
        host_arch: Architecture,
    ) -> Result<Self> {
        let msvc_path = compiler_path(&msvc_info.install_path);
        let base_dir = msvc_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| msvc_path.clone());

        let vc_install_dir = base_dir.join("VC");
        let vc_tools_install_dir = msvc_path;
        let vc_tools_version = msvc_info.version.clone();

        let (windows_sdk_dir, windows_sdk_version) = if let Some(sdk) = sdk_info {
            (compiler_path(&sdk.install_path), sdk.version.clone())
        } else {
            (base_dir.join("Windows Kits").join("10"), String::new())
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
    fn build_include_paths(vc_tools_dir: &Path, sdk_dir: &Path, sdk_version: &str) -> Vec<PathBuf> {
        if sdk_version.is_empty() {
            return vec![vc_tools_dir.join("include")];
        }
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
        vc_tools_dir: &Path,
        sdk_dir: &Path,
        sdk_version: &str,
        arch: Architecture,
    ) -> Vec<PathBuf> {
        let arch_str = arch.to_string();
        if sdk_version.is_empty() {
            return vec![vc_tools_dir.join("lib").join(&arch_str)];
        }
        vec![
            // MSVC libs
            vc_tools_dir.join("lib").join(&arch_str),
            // Windows SDK libs
            sdk_dir
                .join("Lib")
                .join(sdk_version)
                .join("ucrt")
                .join(&arch_str),
            sdk_dir
                .join("Lib")
                .join(sdk_version)
                .join("um")
                .join(&arch_str),
        ]
    }

    /// Build binary paths
    fn build_bin_paths(
        vc_tools_dir: &Path,
        sdk_dir: &Path,
        sdk_version: &str,
        host_arch: Architecture,
        target_arch: Architecture,
    ) -> Vec<PathBuf> {
        let host_dir = host_arch.msvc_host_dir();
        let target_dir = target_arch.msvc_target_dir();

        let mut paths = vec![
            // MSVC binaries
            vc_tools_dir.join("bin").join(host_dir).join(target_dir),
        ];
        if !sdk_version.is_empty() {
            // SDK executables run on the host; their libraries target the output architecture.
            paths.push(
                sdk_dir
                    .join("bin")
                    .join(sdk_version)
                    .join(host_arch.to_string()),
            );
        }
        paths
    }

    /// Check if cl.exe is available in the configured paths
    pub fn has_cl_exe(&self) -> bool {
        self.cl_exe_path().is_some()
    }

    /// Get the path to cl.exe
    pub fn cl_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("cl.exe"))
            .find(|p| p.is_file())
    }

    /// Get the path to link.exe
    pub fn link_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("link.exe"))
            .find(|p| p.is_file())
    }

    /// Get the path to lib.exe (static library manager)
    pub fn lib_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("lib.exe"))
            .find(|p| p.is_file())
    }

    /// Get the path to ml64.exe (MASM assembler)
    pub fn ml64_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("ml64.exe"))
            .find(|p| p.is_file())
    }

    /// Get the path to nmake.exe
    pub fn nmake_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("nmake.exe"))
            .find(|p| p.is_file())
    }

    /// Get the path to rc.exe (resource compiler)
    pub fn rc_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("rc.exe"))
            .find(|p| p.is_file())
    }

    /// Get the path to the Windows manifest tool.
    pub fn mt_exe_path(&self) -> Option<PathBuf> {
        self.bin_paths
            .iter()
            .map(|p| p.join("mt.exe"))
            .find(|p| p.is_file())
    }

    /// Stable toolchain identity independent of its installation directory.
    /// This identifies selected versions and architectures, not file integrity.
    pub fn fingerprint(&self) -> String {
        toolchain_fingerprint(
            Some(&self.vc_tools_version),
            (!self.windows_sdk_version.is_empty()).then_some(self.windows_sdk_version.as_str()),
            self.host_arch,
            self.arch,
        )
    }

    /// Get all tool paths as a struct for easy access
    pub fn tool_paths(&self) -> ToolPaths {
        ToolPaths {
            cl: self.cl_exe_path(),
            link: self.link_exe_path(),
            lib: self.lib_exe_path(),
            ml64: self.ml64_exe_path(),
            nmake: self.nmake_exe_path(),
            rc: self.rc_exe_path(),
            mt: self.mt_exe_path(),
        }
    }

    /// Get the INCLUDE environment variable value
    pub fn include_path_string(&self) -> String {
        self.include_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Get the LIB environment variable value
    pub fn lib_path_string(&self) -> String {
        self.lib_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Get the PATH additions
    pub fn bin_path_string(&self) -> String {
        self.bin_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Export environment to JSON for external tools
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "vc_install_dir": self.vc_install_dir,
            "vc_tools_install_dir": self.vc_tools_install_dir,
            "vc_tools_version": self.vc_tools_version,
            "windows_sdk_dir": self.windows_sdk_dir,
            "windows_sdk_version": self.windows_sdk_version,
            "include_paths": self.include_paths,
            "lib_paths": self.lib_paths,
            "bin_paths": self.bin_paths,
            "arch": self.arch.to_string(),
            "host_arch": self.host_arch.to_string(),
            "fingerprint": self.fingerprint(),
            "tools": {
                "cl": self.cl_exe_path(),
                "link": self.link_exe_path(),
                "lib": self.lib_exe_path(),
                "ml64": self.ml64_exe_path(),
                "nmake": self.nmake_exe_path(),
                "rc": self.rc_exe_path(),
                "mt": self.mt_exe_path(),
            }
        })
    }
}

/// Collection of tool executable paths
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPaths {
    /// Path to cl.exe (C/C++ compiler)
    pub cl: Option<PathBuf>,
    /// Path to link.exe (linker)
    pub link: Option<PathBuf>,
    /// Path to lib.exe (static library manager)
    pub lib: Option<PathBuf>,
    /// Path to ml64.exe (MASM assembler)
    pub ml64: Option<PathBuf>,
    /// Path to nmake.exe (make utility)
    pub nmake: Option<PathBuf>,
    /// Path to rc.exe (resource compiler)
    pub rc: Option<PathBuf>,
    /// Path to mt.exe (manifest tool)
    pub mt: Option<PathBuf>,
}

/// Hash selected toolchain metadata, including host and target independently.
pub fn toolchain_fingerprint(
    msvc: Option<&str>,
    sdk: Option<&str>,
    host: Architecture,
    target: Architecture,
) -> String {
    let identity = format!(
        "msvc-kit.toolchain.v1\nmsvc={}\nsdk={}\nhost={}\ntarget={}\n",
        msvc.unwrap_or_default(),
        sdk.unwrap_or_default(),
        host,
        target
    );
    hex::encode(Sha256::digest(identity.as_bytes()))
}

/// Export an SDK-only environment without implying a compiler installation.
pub fn get_sdk_env_vars(
    sdk_dir: &Path,
    version: &str,
    arch: Architecture,
    host_arch: Architecture,
) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    let root = sdk_dir.display().to_string();
    let bin_root = sdk_dir.join("bin").join(version);
    vars.insert("WindowsSdkDir".into(), root.clone());
    vars.insert("WindowsSDKVersion".into(), format!("{}\\", version));
    vars.insert("WindowsSDKLibVersion".into(), format!("{}\\", version));
    vars.insert("WindowsSdkBinPath".into(), bin_root.display().to_string());
    vars.insert(
        "WindowsSdkVerBinPath".into(),
        bin_root.display().to_string(),
    );
    vars.insert("UniversalCRTSdkDir".into(), root);
    vars.insert("UCRTVersion".into(), version.into());
    vars.insert(
        "INCLUDE".into(),
        ["ucrt", "shared", "um", "winrt", "cppwinrt"]
            .iter()
            .map(|dir| {
                sdk_dir
                    .join("Include")
                    .join(version)
                    .join(dir)
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(";"),
    );
    vars.insert(
        "LIB".into(),
        ["ucrt", "um"]
            .iter()
            .map(|dir| {
                sdk_dir
                    .join("Lib")
                    .join(version)
                    .join(dir)
                    .join(arch.to_string())
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(";"),
    );
    vars.insert(
        "PATH".into(),
        bin_root.join(host_arch.to_string()).display().to_string(),
    );
    vars.insert("Platform".into(), arch.to_string());
    vars.insert("VSCMD_ARG_HOST_ARCH".into(), host_arch.to_string());
    vars.insert("VSCMD_ARG_TGT_ARCH".into(), arch.to_string());
    vars
}

/// Get environment variables as a HashMap
///
/// Returns all environment variables needed for MSVC toolchain,
/// formatted for use with cc-rs and other build tools.
pub fn get_env_vars(env: &MsvcEnvironment) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    // Visual Studio environment variables
    vars.insert(
        "VCINSTALLDIR".to_string(),
        env.vc_install_dir.display().to_string(),
    );
    vars.insert(
        "VCToolsInstallDir".to_string(),
        env.vc_tools_install_dir.display().to_string(),
    );
    vars.insert("VCToolsVersion".to_string(), env.vc_tools_version.clone());

    // SDK metadata is exported only for a resolved, installed SDK.
    if !env.windows_sdk_version.is_empty() {
        vars.extend(get_sdk_env_vars(
            &env.windows_sdk_dir,
            &env.windows_sdk_version,
            env.arch,
            env.host_arch,
        ));
    }

    // INCLUDE path
    let include = env
        .include_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(";");
    vars.insert("INCLUDE".to_string(), include);

    // LIB path
    let lib = env
        .lib_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(";");
    vars.insert("LIB".to_string(), lib);

    // PATH additions
    let path = env
        .bin_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(";");
    vars.insert("PATH".to_string(), path);

    // Platform information
    vars.insert("Platform".to_string(), env.arch.to_string());
    vars.insert("VSCMD_ARG_HOST_ARCH".to_string(), env.host_arch.to_string());
    vars.insert("VSCMD_ARG_TGT_ARCH".to_string(), env.arch.to_string());

    let target = env.arch.rust_target_triple();
    if let Some(compiler) = env.cl_exe_path() {
        for key in ["CC", "CXX"] {
            let compiler = compiler.display().to_string();
            vars.insert(key.to_string(), compiler.clone());
            vars.insert(format!("{}_{}", key, target), compiler.clone());
            vars.insert(format!("{}_{}", key, target.replace('-', "_")), compiler);
        }
    }
    if let Some(linker) = env.link_exe_path() {
        vars.insert(
            format!(
                "CARGO_TARGET_{}_LINKER",
                target.replace('-', "_").to_uppercase()
            ),
            linker.display().to_string(),
        );
    }
    if let Some(librarian) = env.lib_exe_path() {
        vars.insert(format!("AR_{}", target), librarian.display().to_string());
        vars.insert(
            format!("AR_{}", target.replace('-', "_")),
            librarian.display().to_string(),
        );
    }

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
