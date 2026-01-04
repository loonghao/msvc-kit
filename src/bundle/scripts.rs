//! Activation script generation for bundles
//!
//! This module provides bundle-specific script generation by delegating
//! to the unified `scripts` module.

use super::BundleLayout;
use crate::error::Result;
use crate::scripts::{self, GeneratedScripts, ScriptContext};

/// Generated bundle scripts (re-export for backward compatibility)
pub type BundleScripts = GeneratedScripts;

/// Generate activation scripts for a bundle
///
/// Creates portable scripts that use relative paths so the bundle
/// can be moved to any location.
pub fn generate_bundle_scripts(layout: &BundleLayout) -> Result<BundleScripts> {
    let ctx = ScriptContext::portable(
        &layout.msvc_version,
        &layout.sdk_version,
        layout.arch,
        layout.host_arch,
    );

    scripts::generate_portable_scripts(&ctx)
}

/// Save bundle scripts to the bundle directory
pub async fn save_bundle_scripts(layout: &BundleLayout, scripts: &BundleScripts) -> Result<()> {
    scripts::save_scripts(scripts, &layout.root, "setup").await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Architecture;
    use std::path::PathBuf;

    fn sample_layout() -> BundleLayout {
        BundleLayout {
            root: PathBuf::from("C:/msvc-bundle"),
            msvc_version: "14.44.34823".to_string(),
            sdk_version: "10.0.26100.0".to_string(),
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        }
    }

    #[test]
    fn test_generate_bundle_scripts() {
        let layout = sample_layout();
        let scripts = generate_bundle_scripts(&layout).unwrap();

        assert!(scripts.cmd.contains("BUNDLE_ROOT"));
        assert!(scripts.cmd.contains("14.44.34823"));
        assert!(scripts.cmd.contains("10.0.26100.0"));
        assert!(scripts.cmd.contains("Hostx64"));

        assert!(scripts.powershell.contains("$PSScriptRoot"));
        assert!(scripts.bash.contains("BASH_SOURCE"));
        assert!(scripts.readme.is_some());
    }

    #[test]
    fn test_generate_bundle_scripts_arm64() {
        let layout = BundleLayout {
            root: PathBuf::from("C:/msvc-bundle"),
            msvc_version: "14.44.34823".to_string(),
            sdk_version: "10.0.26100.0".to_string(),
            arch: Architecture::Arm64,
            host_arch: Architecture::X64,
        };

        let scripts = generate_bundle_scripts(&layout).unwrap();

        assert!(scripts.cmd.contains("Hostx64"));
        assert!(scripts.cmd.contains("arm64"));
    }

    #[test]
    fn test_generate_bundle_scripts_x86() {
        let layout = BundleLayout {
            root: PathBuf::from("C:/msvc-bundle"),
            msvc_version: "14.44.34823".to_string(),
            sdk_version: "10.0.26100.0".to_string(),
            arch: Architecture::X86,
            host_arch: Architecture::X86,
        };

        let scripts = generate_bundle_scripts(&layout).unwrap();

        assert!(scripts.cmd.contains("Hostx86"));
        assert!(scripts.cmd.contains("x86"));
    }

    #[test]
    fn test_bundle_scripts_readme() {
        let layout = sample_layout();
        let scripts = generate_bundle_scripts(&layout).unwrap();

        let readme = scripts.readme.as_ref().unwrap();
        assert!(readme.contains("14.44.34823"));
        assert!(readme.contains("10.0.26100.0"));
    }

    #[tokio::test]
    async fn test_save_bundle_scripts() {
        let temp_dir = tempfile::tempdir().unwrap();
        let layout = BundleLayout {
            root: temp_dir.path().to_path_buf(),
            msvc_version: "14.44.34823".to_string(),
            sdk_version: "10.0.26100.0".to_string(),
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        };

        let scripts = generate_bundle_scripts(&layout).unwrap();
        save_bundle_scripts(&layout, &scripts).await.unwrap();

        // Verify files were created
        assert!(temp_dir.path().join("setup.bat").exists());
        assert!(temp_dir.path().join("setup.ps1").exists());
        assert!(temp_dir.path().join("setup.sh").exists());
        assert!(temp_dir.path().join("README.txt").exists());

        // Verify content
        let cmd_content = std::fs::read_to_string(temp_dir.path().join("setup.bat")).unwrap();
        assert!(cmd_content.contains("14.44.34823"));
        assert!(cmd_content.contains("BUNDLE_ROOT"));
    }
}
