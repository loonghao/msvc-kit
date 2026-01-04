//! Bundle module tests

use msvc_kit::bundle::{discover_bundle, generate_bundle_scripts, BundleLayout, BundleOptions};
use msvc_kit::version::Architecture;
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

// ============================================================================
// BundleLayout Tests
// ============================================================================

#[test]
fn test_bundle_layout_paths() {
    let layout = sample_layout();

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
    let layout = sample_layout();

    let include = layout.include_env();
    assert!(include.contains("VC"));
    assert!(include.contains("ucrt"));

    let lib = layout.lib_env();
    assert!(lib.contains("lib"));
}

#[test]
fn test_bundle_layout_to_json() {
    let layout = sample_layout();
    let json = layout.to_json();

    assert_eq!(json["msvc_version"], "14.44.34823");
    assert_eq!(json["sdk_version"], "10.0.26100.0");
    assert_eq!(json["arch"], "x64");
    assert_eq!(json["host_arch"], "x64");
    assert!(json["paths"]["vc_dir"].is_string());
    assert!(json["paths"]["vc_tools_dir"].is_string());
    assert!(json["paths"]["sdk_dir"].is_string());
    assert!(json["tools"]["cl"].is_string());
    assert!(json["tools"]["link"].is_string());
    assert!(json["env"]["INCLUDE"].is_string());
    assert!(json["env"]["LIB"].is_string());
    assert!(json["env"]["PATH"].is_string());
}

#[test]
fn test_bundle_layout_to_msvc_environment() {
    let layout = sample_layout();
    let env = layout.to_msvc_environment();

    assert_eq!(env.vc_tools_version, "14.44.34823");
    assert_eq!(env.windows_sdk_version, "10.0.26100.0");
    assert_eq!(env.arch, Architecture::X64);
    assert_eq!(env.host_arch, Architecture::X64);
    assert!(!env.include_paths.is_empty());
    assert!(!env.lib_paths.is_empty());
    assert!(!env.bin_paths.is_empty());
}

#[test]
fn test_bundle_layout_env_vars() {
    let layout = sample_layout();
    let vars = layout.env_vars();

    assert!(vars.contains_key("INCLUDE"));
    assert!(vars.contains_key("LIB"));
    assert!(vars.contains_key("PATH"));
    assert!(vars.contains_key("VCToolsVersion"));
    assert!(vars.contains_key("WindowsSDKVersion"));
}

#[test]
fn test_bundle_layout_tool_paths() {
    let layout = sample_layout();

    let cl_path = layout.cl_exe_path();
    assert!(cl_path.to_string_lossy().ends_with("cl.exe"));
    assert!(cl_path.to_string_lossy().contains("Hostx64"));

    let link_path = layout.link_exe_path();
    assert!(link_path.to_string_lossy().ends_with("link.exe"));

    let lib_path = layout.lib_exe_path();
    assert!(lib_path.to_string_lossy().ends_with("lib.exe"));

    let nmake_path = layout.nmake_exe_path();
    assert!(nmake_path.to_string_lossy().ends_with("nmake.exe"));

    let ml64_path = layout.ml64_exe_path();
    assert!(ml64_path.to_string_lossy().ends_with("ml64.exe"));

    let rc_path = layout.rc_exe_path();
    assert!(rc_path.to_string_lossy().ends_with("rc.exe"));
}

#[test]
fn test_bundle_layout_sdk_include_dirs() {
    let layout = sample_layout();
    let dirs = layout.sdk_include_dirs();

    assert_eq!(dirs.len(), 5);
    assert!(dirs.iter().any(|p| p.to_string_lossy().contains("ucrt")));
    assert!(dirs.iter().any(|p| p.to_string_lossy().contains("shared")));
    assert!(dirs.iter().any(|p| p.to_string_lossy().contains("um")));
}

#[test]
fn test_bundle_layout_sdk_lib_dirs() {
    let layout = sample_layout();
    let dirs = layout.sdk_lib_dirs();

    assert_eq!(dirs.len(), 2);
    assert!(dirs.iter().any(|p| p.to_string_lossy().contains("ucrt")));
    assert!(dirs.iter().any(|p| p.to_string_lossy().contains("um")));
}

#[test]
fn test_bundle_layout_include_paths() {
    let layout = sample_layout();
    let paths = layout.include_paths();

    assert!(paths.len() >= 6);
    assert!(paths.iter().any(|p| {
        p.to_string_lossy().contains("VC") && p.to_string_lossy().contains("include")
    }));
}

#[test]
fn test_bundle_layout_lib_paths() {
    let layout = sample_layout();
    let paths = layout.lib_paths();

    assert!(paths.len() >= 3);
}

#[test]
fn test_bundle_layout_bin_paths() {
    let layout = sample_layout();
    let paths = layout.bin_paths();

    assert_eq!(paths.len(), 2);
}

#[test]
fn test_bundle_layout_from_root_with_versions() {
    let layout = BundleLayout::from_root_with_versions(
        "C:/test-bundle",
        "14.43.0",
        "10.0.22621.0",
        Architecture::Arm64,
        Architecture::X64,
    )
    .unwrap();

    assert_eq!(layout.root, PathBuf::from("C:/test-bundle"));
    assert_eq!(layout.msvc_version, "14.43.0");
    assert_eq!(layout.sdk_version, "10.0.22621.0");
    assert_eq!(layout.arch, Architecture::Arm64);
    assert_eq!(layout.host_arch, Architecture::X64);
}

#[test]
fn test_bundle_layout_verify_nonexistent() {
    let layout = sample_layout();
    let result = layout.verify();
    assert!(result.is_err());
}

#[test]
fn test_bundle_layout_arm64_paths() {
    let layout = BundleLayout {
        root: PathBuf::from("C:/msvc-bundle"),
        msvc_version: "14.44.34823".to_string(),
        sdk_version: "10.0.26100.0".to_string(),
        arch: Architecture::Arm64,
        host_arch: Architecture::X64,
    };

    let bin_dir = layout.vc_bin_dir();
    assert!(bin_dir.to_string_lossy().contains("Hostx64"));
    assert!(bin_dir.to_string_lossy().contains("arm64"));

    let lib_dir = layout.vc_lib_dir();
    assert!(lib_dir.to_string_lossy().contains("arm64"));
}

#[test]
fn test_bundle_layout_serde() {
    let layout = sample_layout();
    let json = serde_json::to_string(&layout).unwrap();
    let parsed: BundleLayout = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.root, layout.root);
    assert_eq!(parsed.msvc_version, layout.msvc_version);
    assert_eq!(parsed.sdk_version, layout.sdk_version);
    assert_eq!(parsed.arch, layout.arch);
    assert_eq!(parsed.host_arch, layout.host_arch);
}

#[test]
fn test_bundle_layout_clone() {
    let layout = sample_layout();
    let cloned = layout.clone();

    assert_eq!(cloned.root, layout.root);
    assert_eq!(cloned.msvc_version, layout.msvc_version);
}

#[test]
fn test_bundle_layout_debug() {
    let layout = sample_layout();
    let debug_str = format!("{:?}", layout);

    assert!(debug_str.contains("BundleLayout"));
    assert!(debug_str.contains("14.44.34823"));
}

// ============================================================================
// BundleOptions Tests
// ============================================================================

#[test]
fn test_bundle_options_default() {
    let opts = BundleOptions::default();

    assert_eq!(opts.output_dir, PathBuf::from("./msvc-bundle"));
    assert_eq!(opts.arch, Architecture::X64);
    assert_eq!(opts.parallel_downloads, 8);
    assert!(opts.msvc_version.is_none());
    assert!(opts.sdk_version.is_none());
}

#[test]
fn test_bundle_options_custom() {
    let opts = BundleOptions {
        output_dir: PathBuf::from("C:/custom-bundle"),
        arch: Architecture::Arm64,
        host_arch: Architecture::X64,
        msvc_version: Some("14.44".to_string()),
        sdk_version: Some("10.0.26100.0".to_string()),
        parallel_downloads: 16,
    };

    assert_eq!(opts.output_dir, PathBuf::from("C:/custom-bundle"));
    assert_eq!(opts.arch, Architecture::Arm64);
    assert_eq!(opts.host_arch, Architecture::X64);
    assert_eq!(opts.msvc_version, Some("14.44".to_string()));
    assert_eq!(opts.sdk_version, Some("10.0.26100.0".to_string()));
    assert_eq!(opts.parallel_downloads, 16);
}

#[test]
fn test_bundle_options_debug() {
    let opts = BundleOptions::default();
    let debug_str = format!("{:?}", opts);

    assert!(debug_str.contains("BundleOptions"));
    assert!(debug_str.contains("msvc-bundle"));
}

#[test]
fn test_bundle_options_clone() {
    let opts = BundleOptions {
        output_dir: PathBuf::from("C:/test"),
        arch: Architecture::X86,
        host_arch: Architecture::X64,
        msvc_version: Some("14.43".to_string()),
        sdk_version: None,
        parallel_downloads: 4,
    };

    let cloned = opts.clone();
    assert_eq!(cloned.output_dir, opts.output_dir);
    assert_eq!(cloned.arch, opts.arch);
    assert_eq!(cloned.msvc_version, opts.msvc_version);
}

// ============================================================================
// discover_bundle Tests
// ============================================================================

#[test]
fn test_discover_bundle_nonexistent() {
    let result = discover_bundle("C:/nonexistent/path");
    assert!(result.is_err());
}

#[test]
fn test_discover_bundle_empty_dir() {
    let temp_dir = tempfile::tempdir().unwrap();
    let result = discover_bundle(temp_dir.path());
    assert!(result.is_err());
}

// ============================================================================
// Bundle Scripts Tests
// ============================================================================

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
