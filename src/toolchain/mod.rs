//! Shared installed-toolchain selection for activation, queries and diagnostics.

pub(crate) mod layout;

use std::path::PathBuf;

use crate::env::MsvcEnvironment;
use crate::error::{MsvcKitError, Result};
use crate::installer::InstallInfo;
use crate::version::{
    list_installed_msvc, list_installed_sdk_at, select_installed_version, Architecture,
};

/// Requested installed versions. Omitted selectors mean latest installed.
#[derive(Debug, Clone)]
pub struct ToolchainRequest {
    /// Portable installation root or existing Visual Studio installation root.
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
    resolve_toolchain_with_sdk_root(request, None)
}

/// Select an existing, separate Windows Kits/10 root without copying or registering it.
/// `None` preserves the portable-root behavior of `resolve_toolchain`.
pub fn resolve_toolchain_with_sdk_root(
    request: &ToolchainRequest,
    sdk_dir: Option<&std::path::Path>,
) -> Result<MsvcEnvironment> {
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
    let explicit_sdk = sdk_dir.is_some();
    let sdk_dir = sdk_root(&install_dir, sdk_dir)?;
    let sdk_versions = list_installed_sdk_at(&sdk_dir);
    if explicit_sdk && sdk_versions.is_empty() {
        return Err(MsvcKitError::ComponentNotFound(format!(
            "No Windows SDK versions found in explicit SDK root: {}",
            sdk_dir.display()
        )));
    }
    let msvc = select_installed_version(&msvc_versions, request.msvc_version.as_deref())?
        .ok_or_else(|| {
            MsvcKitError::ComponentNotFound("No installed MSVC compiler found; VS2015 requires VC/vcvarsall.bat and VC/include/crtversion.h identifying v140 (14.0)".into())
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
        install_path: sdk_dir,
        downloaded_files: vec![],
        arch: request.arch,
    });
    MsvcEnvironment::from_install_info(&msvc_info, sdk_info.as_ref(), request.host_arch)
}

/// Resolve only the requested root; never consult ambient SDK variables or registry.
pub(crate) fn sdk_root(
    install_dir: &std::path::Path,
    explicit: Option<&std::path::Path>,
) -> Result<PathBuf> {
    match explicit {
        Some(root) if !root.is_dir() => Err(MsvcKitError::InstallPath(format!(
            "Explicit Windows SDK directory not found: {}",
            root.display()
        ))),
        Some(root) => Ok(crate::env::compiler_path(&std::path::absolute(root)?)),
        None => Ok(install_dir.join("Windows Kits").join("10")),
    }
}
