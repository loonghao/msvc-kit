//! Unit tests for msvc-kit

use msvc_kit::config::MsvcKitConfig;
use msvc_kit::downloader::{
    compute_hash, hashes_match, ComponentType, DownloadOptions, DownloadPreview,
    FileSystemCacheManager, HttpClientConfig, NoopProgressHandler, PackagePreview, ProgressHandler,
};
use msvc_kit::env::{generate_activation_script, get_env_vars, MsvcEnvironment, ShellType};
use msvc_kit::error::MsvcKitError;
use msvc_kit::installer::InstallInfo;
use msvc_kit::version::{
    is_msvc_installed, is_sdk_installed, Architecture, InstalledVersion, MsvcVersion, SdkVersion,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Architecture Tests
// ============================================================================

mod architecture_tests {
    use super::*;

    #[test]
    fn test_architecture_from_str_x64_variants() {
        assert_eq!("x64".parse::<Architecture>().unwrap(), Architecture::X64);
        assert_eq!("amd64".parse::<Architecture>().unwrap(), Architecture::X64);
        assert_eq!("x86_64".parse::<Architecture>().unwrap(), Architecture::X64);
        assert_eq!("X64".parse::<Architecture>().unwrap(), Architecture::X64);
        assert_eq!("AMD64".parse::<Architecture>().unwrap(), Architecture::X64);
    }

    #[test]
    fn test_architecture_from_str_x86_variants() {
        assert_eq!("x86".parse::<Architecture>().unwrap(), Architecture::X86);
        assert_eq!("i686".parse::<Architecture>().unwrap(), Architecture::X86);
        assert_eq!("i386".parse::<Architecture>().unwrap(), Architecture::X86);
        assert_eq!("X86".parse::<Architecture>().unwrap(), Architecture::X86);
    }

    #[test]
    fn test_architecture_from_str_arm64_variants() {
        assert_eq!(
            "arm64".parse::<Architecture>().unwrap(),
            Architecture::Arm64
        );
        assert_eq!(
            "aarch64".parse::<Architecture>().unwrap(),
            Architecture::Arm64
        );
        assert_eq!(
            "ARM64".parse::<Architecture>().unwrap(),
            Architecture::Arm64
        );
    }

    #[test]
    fn test_architecture_from_str_arm() {
        assert_eq!("arm".parse::<Architecture>().unwrap(), Architecture::Arm);
        assert_eq!("ARM".parse::<Architecture>().unwrap(), Architecture::Arm);
    }

    #[test]
    fn test_architecture_from_str_invalid() {
        assert!("invalid".parse::<Architecture>().is_err());
        assert!("".parse::<Architecture>().is_err());
        assert!("x128".parse::<Architecture>().is_err());
    }

    #[test]
    fn test_architecture_display() {
        assert_eq!(Architecture::X64.to_string(), "x64");
        assert_eq!(Architecture::X86.to_string(), "x86");
        assert_eq!(Architecture::Arm64.to_string(), "arm64");
        assert_eq!(Architecture::Arm.to_string(), "arm");
    }

    #[test]
    fn test_architecture_msvc_host_dir() {
        assert_eq!(Architecture::X64.msvc_host_dir(), "Hostx64");
        assert_eq!(Architecture::X86.msvc_host_dir(), "Hostx86");
        assert_eq!(Architecture::Arm64.msvc_host_dir(), "Hostarm64");
        assert_eq!(Architecture::Arm.msvc_host_dir(), "Hostarm");
    }

    #[test]
    fn test_architecture_msvc_target_dir() {
        assert_eq!(Architecture::X64.msvc_target_dir(), "x64");
        assert_eq!(Architecture::X86.msvc_target_dir(), "x86");
        assert_eq!(Architecture::Arm64.msvc_target_dir(), "arm64");
        assert_eq!(Architecture::Arm.msvc_target_dir(), "arm");
    }

    #[test]
    fn test_architecture_host() {
        // Just ensure it returns a valid architecture
        let host = Architecture::host();
        assert!(matches!(
            host,
            Architecture::X64 | Architecture::X86 | Architecture::Arm64 | Architecture::Arm
        ));
    }

    #[test]
    fn test_architecture_default() {
        assert_eq!(Architecture::default(), Architecture::X64);
    }

    #[test]
    fn test_architecture_clone_eq() {
        let arch = Architecture::X64;
        let cloned = arch;
        assert_eq!(arch, cloned);
    }

    #[test]
    fn test_architecture_serde() {
        let arch = Architecture::X64;
        let json = serde_json::to_string(&arch).unwrap();
        assert_eq!(json, "\"x64\"");

        let parsed: Architecture = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, arch);
    }
}

// ============================================================================
// Config Tests
// ============================================================================

mod config_tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MsvcKitConfig::default();
        assert!(config.verify_hashes);
        assert_eq!(config.parallel_downloads, 4);
        assert_eq!(config.default_arch, Architecture::X64);
        assert!(config.default_msvc_version.is_none());
        assert!(config.default_sdk_version.is_none());
    }

    #[test]
    fn test_config_serde() {
        let config = MsvcKitConfig {
            install_dir: PathBuf::from("C:/test"),
            default_msvc_version: Some("14.44".to_string()),
            default_sdk_version: Some("10.0.26100.0".to_string()),
            default_arch: Architecture::X86,
            verify_hashes: false,
            parallel_downloads: 8,
            cache_dir: Some(PathBuf::from("C:/cache")),
        };

        // Test TOML serialization (production format)
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: MsvcKitConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.install_dir, config.install_dir);
        assert_eq!(parsed.default_msvc_version, config.default_msvc_version);
        assert_eq!(parsed.default_sdk_version, config.default_sdk_version);
        assert_eq!(parsed.default_arch, config.default_arch);
        assert_eq!(parsed.verify_hashes, config.verify_hashes);
        assert_eq!(parsed.parallel_downloads, config.parallel_downloads);
    }

    #[test]
    fn test_get_msvc_install_dir() {
        let config = MsvcKitConfig {
            install_dir: PathBuf::from("C:/msvc-kit"),
            ..Default::default()
        };

        let dir = msvc_kit::config::get_msvc_install_dir(&config, "14.44.33807");
        assert!(dir.to_string_lossy().contains("MSVC"));
        assert!(dir.to_string_lossy().contains("14.44.33807"));
    }

    #[test]
    fn test_get_sdk_install_dir() {
        let config = MsvcKitConfig {
            install_dir: PathBuf::from("C:/msvc-kit"),
            ..Default::default()
        };

        let dir = msvc_kit::config::get_sdk_install_dir(&config, "10.0.26100.0");
        assert!(dir.to_string_lossy().contains("Windows Kits"));
        assert!(dir.to_string_lossy().contains("10.0.26100.0"));
    }
}

// ============================================================================
// DownloadOptions Tests
// ============================================================================

mod download_options_tests {
    use super::*;

    #[test]
    fn test_download_options_default() {
        let options = DownloadOptions::default();
        assert!(options.msvc_version.is_none());
        assert!(options.sdk_version.is_none());
        assert!(options.verify_hashes);
        assert_eq!(options.parallel_downloads, 4);
        assert_eq!(options.arch, Architecture::X64);
    }

    #[test]
    fn test_download_options_custom() {
        let options = DownloadOptions::builder()
            .msvc_version("14.44")
            .sdk_version("10.0.26100.0")
            .target_dir("C:/custom")
            .arch(Architecture::Arm64)
            .host_arch(Architecture::X64)
            .verify_hashes(false)
            .parallel_downloads(16)
            .build();

        assert_eq!(options.msvc_version, Some("14.44".to_string()));
        assert_eq!(options.sdk_version, Some("10.0.26100.0".to_string()));
        assert_eq!(options.target_dir, PathBuf::from("C:/custom"));
        assert_eq!(options.arch, Architecture::Arm64);
        assert_eq!(options.host_arch, Some(Architecture::X64));
        assert!(!options.verify_hashes);
        assert_eq!(options.parallel_downloads, 16);
    }
}

// ============================================================================
// Version Tests
// ============================================================================

mod version_tests {
    use super::*;

    #[test]
    fn test_msvc_version_display() {
        let mut version = MsvcVersion::new("14.44.33807", "MSVC 14.44");
        version.is_latest = true;

        let display = format!("{}", version);
        assert!(display.contains("14.44.33807"));
        assert!(display.contains("latest"));
    }

    #[test]
    fn test_msvc_version_is_installed() {
        let version = MsvcVersion::new("14.44.33807", "MSVC 14.44");

        assert!(!version.is_installed());
    }

    #[test]
    fn test_sdk_version_display() {
        let mut version = SdkVersion::new("10.0.26100.0", "Windows SDK 10.0.26100.0");
        version.is_latest = true;

        let display = format!("{}", version);
        assert!(display.contains("10.0.26100.0"));
        assert!(display.contains("latest"));
    }

    #[test]
    fn test_sdk_version_is_installed() {
        let version = SdkVersion::new("10.0.26100.0", "Windows SDK");

        assert!(!version.is_installed());
    }

    #[test]
    fn test_list_installed_msvc_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let versions = msvc_kit::version::list_installed_msvc(temp_dir.path());
        assert!(versions.is_empty());
    }

    #[test]
    fn test_list_installed_sdk_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let versions = msvc_kit::version::list_installed_sdk(temp_dir.path());
        assert!(versions.is_empty());
    }

    #[test]
    fn test_list_installed_msvc_with_versions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let msvc_dir = temp_dir.path().join("VC").join("Tools").join("MSVC");

        // Create fake version directories
        std::fs::create_dir_all(msvc_dir.join("14.44.33807")).unwrap();
        std::fs::create_dir_all(msvc_dir.join("14.43.34808")).unwrap();

        let versions = msvc_kit::version::list_installed_msvc(temp_dir.path());
        assert_eq!(versions.len(), 2);

        // Should be sorted descending, so 14.44 should be first and marked as latest
        assert!(versions[0].version.starts_with("14.44"));
        assert!(versions[0].is_latest);
        assert!(!versions[1].is_latest);
    }

    #[test]
    fn test_list_installed_sdk_with_versions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sdk_dir = temp_dir
            .path()
            .join("Windows Kits")
            .join("10")
            .join("Include");

        // Create fake version directories
        std::fs::create_dir_all(sdk_dir.join("10.0.26100.0")).unwrap();
        std::fs::create_dir_all(sdk_dir.join("10.0.22621.0")).unwrap();

        let versions = msvc_kit::version::list_installed_sdk(temp_dir.path());
        assert_eq!(versions.len(), 2);

        // Should be sorted descending
        assert!(versions[0].version.starts_with("10.0.26100"));
        assert!(versions[0].is_latest);
    }
}

// ============================================================================
// Error Tests
// ============================================================================

mod error_tests {
    use super::*;

    #[test]
    fn test_error_from_string() {
        let error: MsvcKitError = "test error".into();
        assert!(matches!(error, MsvcKitError::Other(_)));
        assert!(error.to_string().contains("test error"));
    }

    #[test]
    fn test_error_from_owned_string() {
        let error: MsvcKitError = String::from("owned error").into();
        assert!(matches!(error, MsvcKitError::Other(_)));
        assert!(error.to_string().contains("owned error"));
    }

    #[test]
    fn test_error_version_not_found() {
        let error = MsvcKitError::VersionNotFound("14.44".to_string());
        assert!(error.to_string().contains("14.44"));
        assert!(error.to_string().contains("not found"));
    }

    #[test]
    fn test_error_component_not_found() {
        let error = MsvcKitError::ComponentNotFound("cl.exe".to_string());
        assert!(error.to_string().contains("cl.exe"));
    }

    #[test]
    fn test_error_hash_mismatch() {
        let error = MsvcKitError::HashMismatch {
            file: "test.vsix".to_string(),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        assert!(error.to_string().contains("test.vsix"));
        assert!(error.to_string().contains("abc123"));
        assert!(error.to_string().contains("def456"));
    }

    #[test]
    fn test_error_unsupported_platform() {
        let error = MsvcKitError::UnsupportedPlatform("Linux".to_string());
        assert!(error.to_string().contains("Linux"));
    }

    #[test]
    fn test_error_cancelled() {
        let error = MsvcKitError::Cancelled;
        assert!(error.to_string().contains("cancelled"));
    }

    #[test]
    fn test_error_config() {
        let error = MsvcKitError::Config("invalid config".to_string());
        assert!(error.to_string().contains("invalid config"));
    }

    #[test]
    fn test_error_env_setup() {
        let error = MsvcKitError::EnvSetup("failed to set PATH".to_string());
        assert!(error.to_string().contains("failed to set PATH"));
    }

    #[test]
    fn test_error_database() {
        let error = MsvcKitError::Database("connection failed".to_string());
        assert!(error.to_string().contains("connection failed"));
    }

    #[test]
    fn test_error_cab() {
        let error = MsvcKitError::Cab("invalid cab file".to_string());
        assert!(error.to_string().contains("invalid cab file"));
    }

    #[test]
    fn test_error_install_path() {
        let error = MsvcKitError::InstallPath("path not found".to_string());
        assert!(error.to_string().contains("path not found"));
    }

    #[test]
    fn test_error_debug_impl() {
        let error = MsvcKitError::Other("debug test".to_string());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("Other"));
    }
}

// ============================================================================
// Environment Tests (Windows-specific)
// ============================================================================

#[cfg(windows)]
mod windows_env_tests {
    use super::*;
    use msvc_kit::env::{get_env_vars, MsvcEnvironment};

    fn create_test_environment() -> MsvcEnvironment {
        MsvcEnvironment {
            vc_install_dir: PathBuf::from("C:\\VC"),
            vc_tools_install_dir: PathBuf::from("C:\\VC\\Tools\\MSVC\\14.44.33807"),
            vc_tools_version: "14.44.33807".to_string(),
            windows_sdk_dir: PathBuf::from("C:\\Windows Kits\\10"),
            windows_sdk_version: "10.0.26100.0".to_string(),
            include_paths: vec![
                PathBuf::from("C:\\VC\\include"),
                PathBuf::from("C:\\Windows Kits\\10\\Include\\10.0.26100.0\\ucrt"),
            ],
            lib_paths: vec![
                PathBuf::from("C:\\VC\\lib\\x64"),
                PathBuf::from("C:\\Windows Kits\\10\\Lib\\10.0.26100.0\\ucrt\\x64"),
            ],
            bin_paths: vec![
                PathBuf::from("C:\\VC\\bin\\Hostx64\\x64"),
                PathBuf::from("C:\\Windows Kits\\10\\bin\\10.0.26100.0\\x64"),
            ],
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        }
    }

    #[test]
    fn test_env_vars_contains_required_keys() {
        let env = create_test_environment();
        let vars = get_env_vars(&env);

        assert!(vars.contains_key("VCINSTALLDIR"));
        assert!(vars.contains_key("VCToolsInstallDir"));
        assert!(vars.contains_key("VCToolsVersion"));
        assert!(vars.contains_key("WindowsSdkDir"));
        assert!(vars.contains_key("WindowsSDKVersion"));
        assert!(vars.contains_key("INCLUDE"));
        assert!(vars.contains_key("LIB"));
        assert!(vars.contains_key("PATH"));
    }

    #[test]
    fn test_env_vars_values() {
        let env = create_test_environment();
        let vars = get_env_vars(&env);

        assert_eq!(vars.get("VCToolsVersion").unwrap(), "14.44.33807");
        // WindowsSDKVersion includes trailing backslash per Windows SDK standard
        assert_eq!(vars.get("WindowsSDKVersion").unwrap(), "10.0.26100.0\\");
    }

    #[test]
    fn test_env_vars_include_path() {
        let env = create_test_environment();
        let vars = get_env_vars(&env);

        let include = vars.get("INCLUDE").unwrap();
        assert!(include.contains("VC"));
        assert!(include.contains("ucrt"));
    }

    #[test]
    fn test_env_vars_lib_path() {
        let env = create_test_environment();
        let vars = get_env_vars(&env);

        let lib = vars.get("LIB").unwrap();
        assert!(lib.contains("x64"));
    }

    #[test]
    fn test_env_vars_bin_path() {
        let env = create_test_environment();
        let vars = get_env_vars(&env);

        let path = vars.get("PATH").unwrap();
        assert!(path.contains("Hostx64"));
    }
}

// ============================================================================
// Shell Script Generation Tests
// ============================================================================

mod shell_script_tests {
    use msvc_kit::env::ShellType;

    #[test]
    fn test_shell_type_script_extension() {
        assert_eq!(ShellType::Cmd.script_extension(), "bat");
        assert_eq!(ShellType::PowerShell.script_extension(), "ps1");
        assert_eq!(ShellType::Bash.script_extension(), "sh");
    }

    #[test]
    fn test_shell_type_detect() {
        // Just ensure it doesn't panic and returns a valid type
        let shell = ShellType::detect();
        assert!(matches!(
            shell,
            ShellType::Cmd | ShellType::PowerShell | ShellType::Bash
        ));
    }

    #[test]
    fn test_shell_type_clone_eq() {
        let shell = ShellType::PowerShell;
        let cloned = shell;
        assert_eq!(shell, cloned);
    }
}

// ============================================================================
// Library Re-exports Tests (for issue #21)
// ============================================================================

mod library_reexports_tests {
    //! Tests to verify that all required types are properly re-exported
    //! from the crate root for ergonomic imports.
    //!
    //! This addresses the requirement from issue #21:
    //! ```rust
    //! use msvc_kit::{Architecture, DownloadOptions, download_msvc, download_sdk, setup_environment, MsvcEnvironment};
    //! ```

    #[test]
    fn test_architecture_reexport() {
        // Verify Architecture is accessible from crate root
        let _arch: msvc_kit::Architecture = msvc_kit::Architecture::X64;
        assert_eq!(_arch, msvc_kit::version::Architecture::X64);
    }

    #[test]
    fn test_download_options_reexport() {
        // Verify DownloadOptions is accessible from crate root
        let options = msvc_kit::DownloadOptions::default();
        assert_eq!(options.arch, msvc_kit::Architecture::X64);
    }

    #[test]
    fn test_download_options_builder_reexport() {
        // Verify DownloadOptionsBuilder is accessible from crate root
        let options = msvc_kit::DownloadOptions::builder()
            .arch(msvc_kit::Architecture::X64)
            .build();
        assert_eq!(options.arch, msvc_kit::Architecture::X64);
    }

    #[test]
    fn test_msvc_environment_reexport() {
        // Verify MsvcEnvironment is accessible from crate root
        use std::path::PathBuf;
        let _env = msvc_kit::MsvcEnvironment {
            vc_install_dir: PathBuf::new(),
            vc_tools_install_dir: PathBuf::new(),
            vc_tools_version: String::new(),
            windows_sdk_dir: PathBuf::new(),
            windows_sdk_version: String::new(),
            include_paths: vec![],
            lib_paths: vec![],
            bin_paths: vec![],
            arch: msvc_kit::Architecture::X64,
            host_arch: msvc_kit::Architecture::X64,
        };
    }

    #[test]
    fn test_install_info_reexport() {
        // Verify InstallInfo is accessible from crate root
        use std::path::PathBuf;
        let _info = msvc_kit::InstallInfo {
            component_type: "msvc".to_string(),
            version: "14.44".to_string(),
            install_path: PathBuf::new(),
            downloaded_files: vec![],
            arch: msvc_kit::Architecture::X64,
        };
    }

    #[test]
    fn test_tool_paths_reexport() {
        // Verify ToolPaths is accessible from crate root
        let _paths = msvc_kit::ToolPaths {
            cl: None,
            link: None,
            lib: None,
            ml64: None,
            nmake: None,
            rc: None,
        };
    }

    #[test]
    fn test_shell_type_reexport() {
        // Verify ShellType is accessible from crate root
        let _shell = msvc_kit::ShellType::PowerShell;
    }

    #[test]
    fn test_error_types_reexport() {
        // Verify error types are accessible from crate root
        let _err: msvc_kit::Result<()> = Err(msvc_kit::MsvcKitError::Cancelled);
    }

    #[test]
    fn test_download_functions_exist() {
        // Verify download functions are accessible from crate root
        // We just check that the function types exist, not that they work
        let _: fn(&msvc_kit::DownloadOptions) -> _ = |_| async {
            Ok::<msvc_kit::InstallInfo, msvc_kit::MsvcKitError>(msvc_kit::InstallInfo {
                component_type: String::new(),
                version: String::new(),
                install_path: std::path::PathBuf::new(),
                downloaded_files: vec![],
                arch: msvc_kit::Architecture::X64,
            })
        };
    }

    #[test]
    fn test_config_types_reexport() {
        // Verify config types are accessible from crate root
        let _config = msvc_kit::MsvcKitConfig::default();
    }
}

// ============================================================================
// InstallInfo Tests
// ============================================================================

mod install_info_tests {
    use super::*;

    fn create_test_install_info(component_type: &str) -> InstallInfo {
        InstallInfo {
            component_type: component_type.to_string(),
            version: "14.44.33807".to_string(),
            install_path: PathBuf::from("C:/test/path"),
            downloaded_files: vec![],
            arch: Architecture::X64,
        }
    }

    #[test]
    fn test_install_info_is_valid() {
        let info = create_test_install_info("msvc");
        // Path doesn't exist, so should be invalid
        assert!(!info.is_valid());
    }

    #[test]
    fn test_install_info_total_size() {
        let info = create_test_install_info("msvc");
        // No files, so size should be 0
        assert_eq!(info.total_size(), 0);
    }

    #[test]
    fn test_install_info_bin_dir_msvc() {
        let info = create_test_install_info("msvc");
        let bin_dir = info.bin_dir();
        assert!(bin_dir.to_string_lossy().contains("bin"));
        assert!(bin_dir.to_string_lossy().contains("Hostx64"));
        assert!(bin_dir.to_string_lossy().contains("x64"));
    }

    #[test]
    fn test_install_info_bin_dir_sdk() {
        let info = InstallInfo {
            component_type: "sdk".to_string(),
            version: "10.0.26100.0".to_string(),
            install_path: PathBuf::from("C:/test/sdk"),
            downloaded_files: vec![],
            arch: Architecture::X64,
        };
        let bin_dir = info.bin_dir();
        assert!(bin_dir.to_string_lossy().contains("bin"));
        assert!(bin_dir.to_string_lossy().contains("10.0.26100.0"));
    }

    #[test]
    fn test_install_info_bin_dir_unknown() {
        let info = InstallInfo {
            component_type: "unknown".to_string(),
            version: "1.0".to_string(),
            install_path: PathBuf::from("C:/test"),
            downloaded_files: vec![],
            arch: Architecture::X64,
        };
        let bin_dir = info.bin_dir();
        assert!(bin_dir.to_string_lossy().contains("bin"));
    }

    #[test]
    fn test_install_info_include_dir_msvc() {
        let info = create_test_install_info("msvc");
        let include_dir = info.include_dir();
        assert!(include_dir.to_string_lossy().contains("include"));
    }

    #[test]
    fn test_install_info_include_dir_sdk() {
        let info = InstallInfo {
            component_type: "sdk".to_string(),
            version: "10.0.26100.0".to_string(),
            install_path: PathBuf::from("C:/test/sdk"),
            downloaded_files: vec![],
            arch: Architecture::X64,
        };
        let include_dir = info.include_dir();
        assert!(include_dir.to_string_lossy().contains("Include"));
        assert!(include_dir.to_string_lossy().contains("10.0.26100.0"));
    }

    #[test]
    fn test_install_info_lib_dir_msvc() {
        let info = create_test_install_info("msvc");
        let lib_dir = info.lib_dir();
        assert!(lib_dir.to_string_lossy().contains("lib"));
        assert!(lib_dir.to_string_lossy().contains("x64"));
    }

    #[test]
    fn test_install_info_lib_dir_sdk() {
        let info = InstallInfo {
            component_type: "sdk".to_string(),
            version: "10.0.26100.0".to_string(),
            install_path: PathBuf::from("C:/test/sdk"),
            downloaded_files: vec![],
            arch: Architecture::X64,
        };
        let lib_dir = info.lib_dir();
        assert!(lib_dir.to_string_lossy().contains("Lib"));
        assert!(lib_dir.to_string_lossy().contains("um"));
    }

    #[test]
    fn test_install_info_to_json() {
        let info = create_test_install_info("msvc");
        let json = info.to_json();
        assert_eq!(json["component_type"], "msvc");
        assert_eq!(json["version"], "14.44.33807");
        assert_eq!(json["arch"], "x64");
    }
}

// ============================================================================
// MsvcEnvironment Tests
// ============================================================================

mod msvc_environment_tests {
    use super::*;

    fn create_test_environment() -> MsvcEnvironment {
        MsvcEnvironment {
            vc_install_dir: PathBuf::from("C:\\VC"),
            vc_tools_install_dir: PathBuf::from("C:\\VC\\Tools\\MSVC\\14.44.33807"),
            vc_tools_version: "14.44.33807".to_string(),
            windows_sdk_dir: PathBuf::from("C:\\Windows Kits\\10"),
            windows_sdk_version: "10.0.26100.0".to_string(),
            include_paths: vec![
                PathBuf::from("C:\\VC\\include"),
                PathBuf::from("C:\\Windows Kits\\10\\Include\\10.0.26100.0\\ucrt"),
            ],
            lib_paths: vec![
                PathBuf::from("C:\\VC\\lib\\x64"),
                PathBuf::from("C:\\Windows Kits\\10\\Lib\\10.0.26100.0\\ucrt\\x64"),
            ],
            bin_paths: vec![
                PathBuf::from("C:\\VC\\bin\\Hostx64\\x64"),
                PathBuf::from("C:\\Windows Kits\\10\\bin\\10.0.26100.0\\x64"),
            ],
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        }
    }

    #[test]
    fn test_msvc_environment_include_path_string() {
        let env = create_test_environment();
        let include = env.include_path_string();
        assert!(include.contains("VC"));
        assert!(include.contains("ucrt"));
        assert!(include.contains(";"));
    }

    #[test]
    fn test_msvc_environment_lib_path_string() {
        let env = create_test_environment();
        let lib = env.lib_path_string();
        assert!(lib.contains("x64"));
        assert!(lib.contains(";"));
    }

    #[test]
    fn test_msvc_environment_bin_path_string() {
        let env = create_test_environment();
        let bin = env.bin_path_string();
        assert!(bin.contains("Hostx64"));
        assert!(bin.contains(";"));
    }

    #[test]
    fn test_msvc_environment_has_cl_exe() {
        let env = create_test_environment();
        // Paths don't exist, so should return false
        assert!(!env.has_cl_exe());
    }

    #[test]
    fn test_msvc_environment_cl_exe_path() {
        let env = create_test_environment();
        // Paths don't exist, so should return None
        assert!(env.cl_exe_path().is_none());
    }

    #[test]
    fn test_msvc_environment_link_exe_path() {
        let env = create_test_environment();
        assert!(env.link_exe_path().is_none());
    }

    #[test]
    fn test_msvc_environment_lib_exe_path() {
        let env = create_test_environment();
        assert!(env.lib_exe_path().is_none());
    }

    #[test]
    fn test_msvc_environment_ml64_exe_path() {
        let env = create_test_environment();
        assert!(env.ml64_exe_path().is_none());
    }

    #[test]
    fn test_msvc_environment_nmake_exe_path() {
        let env = create_test_environment();
        assert!(env.nmake_exe_path().is_none());
    }

    #[test]
    fn test_msvc_environment_rc_exe_path() {
        let env = create_test_environment();
        assert!(env.rc_exe_path().is_none());
    }

    #[test]
    fn test_msvc_environment_tool_paths() {
        let env = create_test_environment();
        let paths = env.tool_paths();
        assert!(paths.cl.is_none());
        assert!(paths.link.is_none());
        assert!(paths.lib.is_none());
        assert!(paths.ml64.is_none());
        assert!(paths.nmake.is_none());
        assert!(paths.rc.is_none());
    }

    #[test]
    fn test_msvc_environment_to_json() {
        let env = create_test_environment();
        let json = env.to_json();
        assert_eq!(json["vc_tools_version"], "14.44.33807");
        assert_eq!(json["windows_sdk_version"], "10.0.26100.0");
        assert_eq!(json["arch"], "x64");
        assert_eq!(json["host_arch"], "x64");
    }

    #[test]
    fn test_get_env_vars_platform_info() {
        let env = create_test_environment();
        let vars = get_env_vars(&env);
        assert_eq!(vars.get("Platform").unwrap(), "x64");
        assert_eq!(vars.get("VSCMD_ARG_HOST_ARCH").unwrap(), "x64");
        assert_eq!(vars.get("VSCMD_ARG_TGT_ARCH").unwrap(), "x64");
    }

    #[test]
    fn test_get_env_vars_windows_sdk_bin_path() {
        let env = create_test_environment();
        let vars = get_env_vars(&env);
        let sdk_bin = vars.get("WindowsSdkBinPath").unwrap();
        assert!(sdk_bin.contains("bin"));
        assert!(sdk_bin.contains("10.0.26100.0"));
    }
}

// ============================================================================
// Shell Script Generation Tests
// ============================================================================

mod shell_script_generation_tests {
    use super::*;

    fn create_test_environment() -> MsvcEnvironment {
        MsvcEnvironment {
            vc_install_dir: PathBuf::from("C:\\VC"),
            vc_tools_install_dir: PathBuf::from("C:\\VC\\Tools\\MSVC\\14.44"),
            vc_tools_version: "14.44.33807".to_string(),
            windows_sdk_dir: PathBuf::from("C:\\Windows Kits\\10"),
            windows_sdk_version: "10.0.26100.0".to_string(),
            include_paths: vec![PathBuf::from("C:\\include")],
            lib_paths: vec![PathBuf::from("C:\\lib")],
            bin_paths: vec![PathBuf::from("C:\\bin")],
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        }
    }

    #[test]
    fn test_generate_cmd_script() {
        let env = create_test_environment();
        let script = generate_activation_script(&env, ShellType::Cmd);
        assert!(script.contains("@echo off"));
        assert!(script.contains("set \""));
        assert!(script.contains("MSVC environment"));
    }

    #[test]
    fn test_generate_powershell_script() {
        let env = create_test_environment();
        let script = generate_activation_script(&env, ShellType::PowerShell);
        assert!(script.contains("$env:"));
        assert!(script.contains("Write-Host"));
        assert!(script.contains("MSVC environment"));
    }

    #[test]
    fn test_generate_bash_script() {
        let env = create_test_environment();
        let script = generate_activation_script(&env, ShellType::Bash);
        assert!(script.contains("#!/bin/bash"));
        assert!(script.contains("export "));
        assert!(script.contains("MSVC environment"));
    }

    #[test]
    fn test_shell_type_equality() {
        assert_eq!(ShellType::Cmd, ShellType::Cmd);
        assert_eq!(ShellType::PowerShell, ShellType::PowerShell);
        assert_eq!(ShellType::Bash, ShellType::Bash);
        assert_ne!(ShellType::Cmd, ShellType::PowerShell);
    }
}

// ============================================================================
// Version Installation Check Tests
// ============================================================================

mod version_installation_tests {
    use super::*;

    #[test]
    fn test_is_msvc_installed_nonexistent_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(!is_msvc_installed(temp_dir.path(), "14.44.33807"));
    }

    #[test]
    fn test_is_msvc_installed_empty_msvc_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let msvc_dir = temp_dir.path().join("VC").join("Tools").join("MSVC");
        std::fs::create_dir_all(&msvc_dir).unwrap();
        assert!(!is_msvc_installed(temp_dir.path(), "14.44.33807"));
    }

    #[test]
    fn test_is_msvc_installed_exact_version() {
        let temp_dir = tempfile::tempdir().unwrap();
        let msvc_dir = temp_dir
            .path()
            .join("VC")
            .join("Tools")
            .join("MSVC")
            .join("14.44.33807");
        std::fs::create_dir_all(&msvc_dir).unwrap();
        assert!(is_msvc_installed(temp_dir.path(), "14.44.33807"));
    }

    #[test]
    fn test_is_msvc_installed_prefix_match() {
        let temp_dir = tempfile::tempdir().unwrap();
        let msvc_dir = temp_dir
            .path()
            .join("VC")
            .join("Tools")
            .join("MSVC")
            .join("14.44.33807");
        std::fs::create_dir_all(&msvc_dir).unwrap();
        // Should match prefix
        assert!(is_msvc_installed(temp_dir.path(), "14.44"));
    }

    #[test]
    fn test_is_sdk_installed_nonexistent_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(!is_sdk_installed(temp_dir.path(), "10.0.26100.0"));
    }

    #[test]
    fn test_is_sdk_installed_exact_version() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sdk_dir = temp_dir
            .path()
            .join("Windows Kits")
            .join("10")
            .join("Include")
            .join("10.0.26100.0");
        std::fs::create_dir_all(&sdk_dir).unwrap();
        assert!(is_sdk_installed(temp_dir.path(), "10.0.26100.0"));
    }

    #[test]
    fn test_is_sdk_installed_partial_match() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sdk_dir = temp_dir
            .path()
            .join("Windows Kits")
            .join("10")
            .join("Include")
            .join("10.0.26100.0");
        std::fs::create_dir_all(&sdk_dir).unwrap();
        // Should match partial version
        assert!(is_sdk_installed(temp_dir.path(), "26100"));
    }
}

// ============================================================================
// Version Display Tests
// ============================================================================

mod version_display_tests {
    use super::*;

    #[test]
    fn test_msvc_version_display_not_latest() {
        let version = MsvcVersion::new("14.44.33807", "MSVC 14.44");
        let display = format!("{}", version);
        assert!(display.contains("14.44.33807"));
        assert!(!display.contains("latest"));
    }

    #[test]
    fn test_sdk_version_display_not_latest() {
        let version = SdkVersion::new("10.0.26100.0", "Windows SDK");
        let display = format!("{}", version);
        assert!(display.contains("10.0.26100.0"));
        assert!(!display.contains("latest"));
    }

    #[test]
    fn test_version_component_name() {
        let msvc = MsvcVersion::new("14.44", "MSVC");
        assert_eq!(msvc.component_name(), "MSVC");

        let sdk = SdkVersion::new("10.0.26100.0", "SDK");
        assert_eq!(sdk.component_name(), "Windows SDK");
    }
}

// ============================================================================
// InstalledVersion Tests
// ============================================================================

mod installed_version_tests {
    use super::*;

    #[test]
    fn test_installed_version_creation() {
        let msvc = MsvcVersion::new("14.44.33807", "MSVC 14.44");
        let sdk = SdkVersion::new("10.0.26100.0", "Windows SDK");

        let installed = InstalledVersion {
            msvc: Some(msvc),
            sdk: Some(sdk),
            installed_at: chrono::Utc::now(),
            arch: Architecture::X64,
        };

        assert!(installed.msvc.is_some());
        assert!(installed.sdk.is_some());
        assert_eq!(installed.arch, Architecture::X64);
    }

    #[test]
    fn test_installed_version_serde() {
        let installed = InstalledVersion {
            msvc: Some(MsvcVersion::new("14.44", "MSVC")),
            sdk: None,
            installed_at: chrono::Utc::now(),
            arch: Architecture::Arm64,
        };

        let json = serde_json::to_string(&installed).unwrap();
        let parsed: InstalledVersion = serde_json::from_str(&json).unwrap();
        assert!(parsed.msvc.is_some());
        assert!(parsed.sdk.is_none());
        assert_eq!(parsed.arch, Architecture::Arm64);
    }
}

// ============================================================================
// DownloadOptions Builder Tests
// ============================================================================

mod download_options_builder_tests {
    use super::*;

    #[test]
    fn test_builder_all_options() {
        let options = DownloadOptions::builder()
            .msvc_version("14.44")
            .sdk_version("10.0.26100.0")
            .target_dir("C:/custom")
            .arch(Architecture::Arm64)
            .host_arch(Architecture::X64)
            .verify_hashes(false)
            .parallel_downloads(16)
            .dry_run(true)
            .build();

        assert_eq!(options.msvc_version, Some("14.44".to_string()));
        assert_eq!(options.sdk_version, Some("10.0.26100.0".to_string()));
        assert_eq!(options.target_dir, PathBuf::from("C:/custom"));
        assert_eq!(options.arch, Architecture::Arm64);
        assert_eq!(options.host_arch, Some(Architecture::X64));
        assert!(!options.verify_hashes);
        assert_eq!(options.parallel_downloads, 16);
        assert!(options.dry_run);
    }

    #[test]
    fn test_builder_partial_options() {
        let options = DownloadOptions::builder()
            .msvc_version("14.44")
            .target_dir("C:/test")
            .build();

        assert_eq!(options.msvc_version, Some("14.44".to_string()));
        assert!(options.sdk_version.is_none());
        assert_eq!(options.target_dir, PathBuf::from("C:/test"));
    }

    #[test]
    fn test_download_options_debug() {
        let options = DownloadOptions::default();
        let debug_str = format!("{:?}", options);
        assert!(debug_str.contains("DownloadOptions"));
        assert!(debug_str.contains("msvc_version"));
        assert!(debug_str.contains("target_dir"));
    }
}

// ============================================================================
// DownloadPreview Tests
// ============================================================================

mod download_preview_tests {
    use super::*;

    #[test]
    fn test_download_preview_format() {
        let preview = DownloadPreview {
            component: "MSVC".to_string(),
            version: "14.44.33807".to_string(),
            package_count: 10,
            file_count: 100,
            total_size: 1024 * 1024 * 500, // 500 MB
            packages: vec![],
        };

        let formatted = preview.format();
        assert!(formatted.contains("MSVC"));
        assert!(formatted.contains("14.44.33807"));
        assert!(formatted.contains("10 packages"));
        assert!(formatted.contains("100 files"));
    }

    #[test]
    fn test_package_preview() {
        let package = PackagePreview {
            id: "Microsoft.VC.Tools".to_string(),
            version: "14.44.33807".to_string(),
            file_count: 50,
            size: 1024 * 1024 * 100,
        };

        assert_eq!(package.id, "Microsoft.VC.Tools");
        assert_eq!(package.version, "14.44.33807");
        assert_eq!(package.file_count, 50);
    }
}

// ============================================================================
// ComponentType Tests
// ============================================================================

mod component_type_tests {
    use super::*;

    #[test]
    fn test_component_type_as_str() {
        assert_eq!(ComponentType::Msvc.as_str(), "msvc");
        assert_eq!(ComponentType::Sdk.as_str(), "sdk");
    }

    #[test]
    fn test_component_type_display() {
        assert_eq!(format!("{}", ComponentType::Msvc), "msvc");
        assert_eq!(format!("{}", ComponentType::Sdk), "sdk");
    }

    #[test]
    fn test_component_type_equality() {
        assert_eq!(ComponentType::Msvc, ComponentType::Msvc);
        assert_eq!(ComponentType::Sdk, ComponentType::Sdk);
        assert_ne!(ComponentType::Msvc, ComponentType::Sdk);
    }

    #[test]
    fn test_component_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ComponentType::Msvc);
        set.insert(ComponentType::Sdk);
        assert_eq!(set.len(), 2);
        assert!(set.contains(&ComponentType::Msvc));
        assert!(set.contains(&ComponentType::Sdk));
    }
}

// ============================================================================
// Hash Utility Tests
// ============================================================================

mod hash_utility_tests {
    use super::*;

    #[test]
    fn test_compute_hash_empty() {
        let hash = compute_hash(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_compute_hash_known_value() {
        let hash = compute_hash(b"test");
        assert_eq!(
            hash,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[test]
    fn test_hashes_match_case_insensitive() {
        assert!(hashes_match("ABCDEF", "abcdef"));
        assert!(hashes_match("AbCdEf", "aBcDeF"));
        assert!(hashes_match("123abc", "123ABC"));
    }

    #[test]
    fn test_hashes_match_different() {
        assert!(!hashes_match("abc123", "abc124"));
        assert!(!hashes_match("", "abc"));
    }
}

// ============================================================================
// HttpClientConfig Tests
// ============================================================================

mod http_client_config_tests {
    use super::*;

    #[test]
    fn test_http_client_config_default() {
        let config = HttpClientConfig::default();
        assert!(config.user_agent.contains("msvc-kit"));
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(30)));
        assert_eq!(config.timeout, Some(Duration::from_secs(300)));
    }

    #[test]
    fn test_http_client_config_custom() {
        let config = HttpClientConfig::with_user_agent("custom/1.0")
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60));

        assert_eq!(config.user_agent, "custom/1.0");
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(10)));
        assert_eq!(config.timeout, Some(Duration::from_secs(60)));
    }

    #[test]
    fn test_http_client_config_build() {
        let config = HttpClientConfig::default();
        let _client = config.build();
        // Just verify it doesn't panic
    }
}

// ============================================================================
// Progress Handler Tests
// ============================================================================

mod progress_handler_tests {
    use super::*;

    #[test]
    fn test_noop_progress_handler() {
        let handler = NoopProgressHandler;
        // All methods should be no-ops and not panic
        handler.on_start("MSVC", 100, 1024 * 1024);
        handler.on_file_start("test.vsix", 1024);
        handler.on_progress(512);
        handler.on_file_complete("test.vsix", "downloaded");
        handler.on_complete(10, 5);
        handler.on_error("test error");
        handler.on_message("test message");
    }

    #[test]
    fn test_progress_handler_boxed() {
        let handler: Arc<dyn ProgressHandler> = Arc::new(NoopProgressHandler);
        handler.on_start("SDK", 50, 512 * 1024);
        handler.on_complete(50, 0);
    }
}

// ============================================================================
// FileSystemCacheManager Tests
// ============================================================================

mod cache_manager_tests {
    use super::*;
    use msvc_kit::downloader::CacheManager;

    #[test]
    fn test_filesystem_cache_basic_operations() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSystemCacheManager::new(temp_dir.path());

        // Test set and get
        cache.set("test_key", b"test_value").unwrap();
        assert_eq!(cache.get("test_key"), Some(b"test_value".to_vec()));

        // Test contains
        assert!(cache.contains("test_key"));
        assert!(!cache.contains("nonexistent"));
    }

    #[test]
    fn test_filesystem_cache_invalidate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSystemCacheManager::new(temp_dir.path());

        cache.set("key", b"value").unwrap();
        assert!(cache.contains("key"));

        cache.invalidate("key").unwrap();
        assert!(!cache.contains("key"));
    }

    #[test]
    fn test_filesystem_cache_clear() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSystemCacheManager::new(temp_dir.path());

        cache.set("key1", b"value1").unwrap();
        cache.set("key2", b"value2").unwrap();

        cache.clear().unwrap();

        assert!(!cache.contains("key1"));
        assert!(!cache.contains("key2"));
    }

    #[test]
    fn test_filesystem_cache_entry_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSystemCacheManager::new(temp_dir.path());

        let path = cache.entry_path("some/nested/key");
        assert!(path.ends_with("some/nested/key") || path.ends_with("some\\nested\\key"));
    }

    #[test]
    fn test_filesystem_cache_nested_keys() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSystemCacheManager::new(temp_dir.path());

        cache.set("nested/path/key", b"nested_value").unwrap();
        assert_eq!(cache.get("nested/path/key"), Some(b"nested_value".to_vec()));
    }

    #[test]
    fn test_filesystem_cache_default_dir() {
        let cache = FileSystemCacheManager::default_cache_dir();
        let cache_dir = cache.cache_dir();
        // Just verify it returns a valid path
        assert!(!cache_dir.to_string_lossy().is_empty());
    }
}

// ============================================================================
// Error Conversion Tests
// ============================================================================

mod error_conversion_tests {
    use super::*;

    #[test]
    fn test_error_from_io_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: MsvcKitError = io_error.into();
        assert!(matches!(error, MsvcKitError::Io(_)));
    }

    #[test]
    fn test_error_hash_mismatch() {
        let error = MsvcKitError::HashMismatch {
            file: "test.vsix".to_string(),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.contains("test.vsix"));
        assert!(msg.contains("abc123"));
        assert!(msg.contains("def456"));
    }

    #[test]
    fn test_error_serialization() {
        let error = MsvcKitError::Config("test config error".to_string());
        assert!(error.to_string().contains("test config error"));
    }
}

// ============================================================================
// Constants Tests
// ============================================================================

mod constants_tests {
    use msvc_kit::constants::{download, extraction, hash, progress, USER_AGENT, VS_CHANNEL_URL};

    #[test]
    fn test_user_agent() {
        assert!(USER_AGENT.contains("msvc-kit"));
    }

    #[test]
    fn test_vs_channel_url() {
        assert!(VS_CHANNEL_URL.starts_with("https://"));
        assert!(VS_CHANNEL_URL.contains("vs"));
    }

    #[test]
    fn test_download_constants() {
        // Verify constants are accessible and have expected values
        assert_eq!(download::MAX_RETRIES, 4);
        assert_eq!(download::DEFAULT_PARALLEL_DOWNLOADS, 4);
        assert_eq!(download::MIN_CONCURRENCY, 2);
        // Verify throughput thresholds relationship
        let low = download::LOW_THROUGHPUT_MBPS;
        let high = download::HIGH_THROUGHPUT_MBPS;
        assert!(
            low < high,
            "LOW_THROUGHPUT_MBPS should be less than HIGH_THROUGHPUT_MBPS"
        );
    }

    #[test]
    fn test_progress_constants() {
        // Verify progress constants are accessible
        assert_eq!(progress::SPINNER_TICK_MS, 80);
        assert_eq!(progress::PROGRESS_TICK_MS, 120);
        assert_eq!(progress::UPDATE_INTERVAL.as_millis(), 200);
    }

    #[test]
    fn test_hash_constants() {
        // Verify hash buffer size is 1 MB
        assert_eq!(hash::HASH_BUFFER_SIZE, 1024 * 1024);
    }

    #[test]
    fn test_extraction_constants() {
        // Verify extraction buffer size is 128 KB
        assert_eq!(extraction::EXTRACT_BUFFER_SIZE, 128 * 1024);
    }
}
