//! Integration tests for msvc-kit

use msvc_kit::config::MsvcKitConfig;
use msvc_kit::version::Architecture;
use msvc_kit::DownloadOptions;

#[test]
fn test_architecture_parsing() {
    assert_eq!("x64".parse::<Architecture>().unwrap(), Architecture::X64);
    assert_eq!("amd64".parse::<Architecture>().unwrap(), Architecture::X64);
    assert_eq!("x86".parse::<Architecture>().unwrap(), Architecture::X86);
    assert_eq!(
        "arm64".parse::<Architecture>().unwrap(),
        Architecture::Arm64
    );
}

#[test]
fn test_architecture_display() {
    assert_eq!(Architecture::X64.to_string(), "x64");
    assert_eq!(Architecture::X86.to_string(), "x86");
    assert_eq!(Architecture::Arm64.to_string(), "arm64");
}

#[test]
fn test_default_config() {
    let config = MsvcKitConfig::default();
    assert!(config.verify_hashes);
    assert_eq!(config.parallel_downloads, 4);
    assert_eq!(config.default_arch, Architecture::X64);
}

#[test]
fn test_download_options_default() {
    let options = DownloadOptions::default();
    assert!(options.msvc_version.is_none());
    assert!(options.sdk_version.is_none());
    assert!(options.verify_hashes);
    assert_eq!(options.parallel_downloads, 4);
}

#[test]
fn test_architecture_msvc_dirs() {
    assert_eq!(Architecture::X64.msvc_host_dir(), "Hostx64");
    assert_eq!(Architecture::X64.msvc_target_dir(), "x64");
    assert_eq!(Architecture::X86.msvc_host_dir(), "Hostx86");
    assert_eq!(Architecture::X86.msvc_target_dir(), "x86");
}

#[cfg(windows)]
mod windows_tests {
    use super::*;
    use msvc_kit::env::{get_env_vars, MsvcEnvironment};
    use std::path::PathBuf;

    #[test]
    fn test_env_vars_generation() {
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
        assert!(vars.contains_key("VCToolsInstallDir"));
        assert!(vars.contains_key("VCToolsVersion"));
        assert!(vars.contains_key("WindowsSdkDir"));
        assert!(vars.contains_key("INCLUDE"));
        assert!(vars.contains_key("LIB"));
        assert!(vars.contains_key("PATH"));

        assert_eq!(vars.get("VCToolsVersion").unwrap(), "14.40.33807");
    }
}
