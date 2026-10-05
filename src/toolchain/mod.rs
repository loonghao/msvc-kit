//! Shared installed-toolchain selection for activation, queries and diagnostics.

use std::path::PathBuf;

use crate::env::MsvcEnvironment;
use crate::error::{MsvcKitError, Result};
use crate::installer::InstallInfo;
use crate::version::{
    list_installed_msvc, list_installed_sdk, select_installed_version, Architecture,
};

/// Requested installed versions. Omitted selectors mean latest installed.
#[derive(Debug, Clone)]
pub struct ToolchainRequest {
    /// Portable installation root.
    pub install_dir: PathBuf,
    /// Target output architecture.
    pub arch: Architecture,
    /// Architecture of compiler and SDK executables.
    pub host_arch: Architecture,
    /// Exact MSVC version or dotted prefix.
    pub msvc_version: Option<String>,
    /// Exact SDK version, dotted prefix or SDK build-number shorthand.
    pub sdk_version: Option<String>,
}

impl Default for ToolchainRequest {
    fn default() -> Self {
        Self {
            install_dir: "msvc-kit".into(),
            arch: Architecture::host(),
            host_arch: Architecture::host(),
            msvc_version: None,
            sdk_version: None,
        }
    }
}

/// Resolve versions once and construct an environment for those exact versions.
/// An absent SDK is allowed only when no SDK selector was explicitly requested.
pub fn resolve_toolchain(request: &ToolchainRequest) -> Result<MsvcEnvironment> {
    if !request.install_dir.is_dir() {
        return Err(MsvcKitError::InstallPath(format!(
            "Installation directory not found: {}",
            request.install_dir.display()
        )));
    }
    // Keep compiler-facing paths absolute without Windows verbatim prefixes,
    // which some downstream build tools do not accept in environment values.
    let install_dir = crate::env::compiler_path(&std::path::absolute(&request.install_dir)?);
    let msvc_versions = list_installed_msvc(&install_dir);
    let sdk_versions = list_installed_sdk(&install_dir);
    let msvc = select_installed_version(&msvc_versions, request.msvc_version.as_deref())?
        .ok_or_else(|| {
            MsvcKitError::ComponentNotFound("No installed MSVC compiler found".into())
        })?;
    let sdk = select_installed_version(&sdk_versions, request.sdk_version.as_deref())?;
    let msvc_info = InstallInfo {
        component_type: "msvc".into(),
        version: msvc.version.clone(),
        install_path: msvc
            .install_path
            .clone()
            .ok_or_else(|| MsvcKitError::InstallPath("MSVC installation path is missing".into()))?,
        downloaded_files: vec![],
        arch: request.arch,
    };
    let sdk_info = sdk.map(|version| InstallInfo {
        component_type: "sdk".into(),
        version: version.version.clone(),
        install_path: install_dir.join("Windows Kits").join("10"),
        downloaded_files: vec![],
        arch: request.arch,
    });
    MsvcEnvironment::from_install_info(&msvc_info, sdk_info.as_ref(), request.host_arch)
}
