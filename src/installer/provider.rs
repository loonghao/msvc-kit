//! Provider API layering (spec 07 §6, roadmap P10).
//!
//! Defines the `ToolchainProvider` and `BuildToolProvider` traits that a
//! provider implements to participate in the shared detect → resolve → plan →
//! acquire → validate lifecycle. The existing MSVC/SDK downloaders adapt via
//! [`MsvcToolchainProvider`]; build tools (CMake/Ninja/NASM) adapt via
//! [`SimpleBuildToolProvider`].
//!
//! The shared infrastructure (Plan, Cache, Transaction, Receipt, Diagnostics)
//! is owned by the caller (the plan/apply pipeline), not by the provider -
//! providers supply the provider-specific resolution and validation.

use std::path::Path;

use async_trait::async_trait;

use crate::downloader::MsvcComponent;
use crate::error::Result;
use crate::installer::detection::BuildToolId;
use crate::installer::detection::DetectedToolchain;
use crate::version::Architecture;

/// A toolchain provider (spec 07 §6). Implementors supply the provider-
/// specific detect, resolve and validate; the caller owns the plan/cache/
/// transaction pipeline.
#[async_trait]
pub trait ToolchainProvider: Send + Sync {
    /// The provider's identity (e.g. `"msvc"`).
    fn provider_id(&self) -> &'static str;

    /// Detect installed toolchains of this provider kind at the given roots.
    /// Read-only.
    fn detect(&self, root: &Path, arch: Architecture) -> Result<DetectedToolchain>;

    /// Resolve a version request to an exact version. `None` selects latest.
    fn resolve_version(
        &self,
        manifest: &crate::downloader::VsManifest,
        requested: Option<&str>,
    ) -> Result<String>;

    /// The component opt-ins this provider requires for the given build
    /// profile (delegates to the resolver's profile mapping).
    fn include_components(
        &self,
        profile: &crate::installer::resolver::BuildProfile,
    ) -> Vec<MsvcComponent> {
        profile.include_components()
    }
}

/// A build-tool provider (spec 07 §6 `BuildToolProvider`). Build tools are
/// not compiler toolchains; they have their own detect/validate lifecycle.
#[async_trait]
pub trait BuildToolProvider: Send + Sync {
    fn tool_id(&self) -> BuildToolId;

    /// Detect the build tool at the given path. Read-only.
    fn detect(&self, exe_path: &Path) -> Result<DetectedToolchain>;
}

/// The MSVC provider implementation: adapts the existing detection and
/// resolver to the `ToolchainProvider` trait.
#[derive(Debug, Default)]
pub struct MsvcToolchainProvider;

#[async_trait]
impl ToolchainProvider for MsvcToolchainProvider {
    fn provider_id(&self) -> &'static str {
        "msvc"
    }

    fn detect(&self, root: &Path, arch: Architecture) -> Result<DetectedToolchain> {
        crate::installer::detection::detect_msvc(root, arch)
    }

    fn resolve_version(
        &self,
        manifest: &crate::downloader::VsManifest,
        requested: Option<&str>,
    ) -> Result<String> {
        match requested {
            Some(requested) => manifest.resolve_msvc_version(requested).ok_or_else(|| {
                crate::error::MsvcKitError::VersionNotFound(format!(
                    "MSVC selector {requested:?} not found in the manifest"
                ))
            }),
            None => manifest.get_latest_msvc_version().ok_or_else(|| {
                crate::error::MsvcKitError::VersionNotFound(
                    "No MSVC version in the manifest".to_string(),
                )
            }),
        }
    }
}

/// A generic build-tool provider implementation for CMake/Ninja/NASM.
#[derive(Debug)]
pub struct SimpleBuildToolProvider {
    tool: BuildToolId,
}

impl SimpleBuildToolProvider {
    pub fn new(tool: BuildToolId) -> Self {
        Self { tool }
    }
}

#[async_trait]
impl BuildToolProvider for SimpleBuildToolProvider {
    fn tool_id(&self) -> BuildToolId {
        self.tool
    }

    fn detect(&self, exe_path: &Path) -> Result<DetectedToolchain> {
        crate::installer::detection::detect_build_tool(exe_path, self.tool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installer::detection::ToolchainKind;
    use std::fs;
    use std::path::PathBuf;

    fn msvc_fixture(dir: &Path, version: &str, arch: Architecture) -> PathBuf {
        let host = Architecture::host();
        let bin = dir
            .join("VC/Tools/MSVC")
            .join(version)
            .join("bin")
            .join(host.msvc_host_dir())
            .join(arch.msvc_target_dir());
        fs::create_dir_all(&bin).unwrap();
        for tool in ["cl.exe", "link.exe", "lib.exe"] {
            fs::write(bin.join(tool), b"fake").unwrap();
        }
        let include = dir.join("VC/Tools/MSVC").join(version).join("include");
        fs::create_dir_all(&include).unwrap();
        fs::write(include.join("stdio.h"), b"fake").unwrap();
        let lib = dir
            .join("VC/Tools/MSVC")
            .join(version)
            .join("lib")
            .join(arch.to_string());
        fs::create_dir_all(&lib).unwrap();
        fs::write(lib.join("libcmt.lib"), b"fake").unwrap();
        dir.to_path_buf()
    }

    #[test]
    fn msvc_provider_detects_complete_installation() {
        let dir = tempfile::tempdir().unwrap();
        let root = msvc_fixture(dir.path(), "14.44.34823", Architecture::X64);
        let provider = MsvcToolchainProvider;
        let detected = provider.detect(&root, Architecture::X64).unwrap();
        assert_eq!(detected.kind, ToolchainKind::Msvc);
        assert!(detected.validation.all_present);
    }

    #[test]
    fn msvc_provider_resolves_version() {
        let provider = MsvcToolchainProvider;
        let manifest = {
            use crate::downloader::VsPackage;
            crate::downloader::VsManifest {
                manifest_version: "1.0".to_string(),
                engine_version: None,
                packages: vec![VsPackage {
                    id: "Microsoft.VC.14.44.Tools.HostX64.TargetX64".to_string(),
                    version: "14.44.34823".to_string(),
                    package_type: "VSIX".to_string(),
                    chip: None,
                    language: None,
                    payloads: Vec::new(),
                    dependencies: Default::default(),
                    machine_arch: None,
                    product_arch: None,
                }],
            }
        };
        let resolved = provider.resolve_version(&manifest, Some("14.44")).unwrap();
        assert_eq!(resolved, "14.44.34823");
    }

    #[test]
    fn build_tool_provider_detects_existing_exe() {
        let dir = tempfile::tempdir().unwrap();
        let cmake = dir.path().join("cmake.exe");
        fs::write(&cmake, b"fake cmake").unwrap();
        let provider = SimpleBuildToolProvider::new(BuildToolId::CMake);
        let detected = provider.detect(&cmake).unwrap();
        assert_eq!(detected.kind, ToolchainKind::BuildTool(BuildToolId::CMake));
        assert!(detected.validation.all_present);
    }

    #[test]
    fn provider_id_is_stable() {
        assert_eq!(MsvcToolchainProvider.provider_id(), "msvc");
    }
}
