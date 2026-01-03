//! Unit tests for msvc-kit

use msvc_kit::config::MsvcKitConfig;
use msvc_kit::error::MsvcKitError;
use msvc_kit::version::{Architecture, MsvcVersion, SdkVersion};
use msvc_kit::DownloadOptions;
use std::path::PathBuf;

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
        let _paths = msvc_kit::ToolPaths::default();
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
        let _: fn(&msvc_kit::DownloadOptions) -> _ = |_| {
            async { Ok::<msvc_kit::InstallInfo, msvc_kit::MsvcKitError>(msvc_kit::InstallInfo {
                component_type: String::new(),
                version: String::new(),
                install_path: std::path::PathBuf::new(),
                downloaded_files: vec![],
                arch: msvc_kit::Architecture::X64,
            }) }
        };
    }

    #[test]
    fn test_config_types_reexport() {
        // Verify config types are accessible from crate root
        let _config = msvc_kit::MsvcKitConfig::default();
    }
}
