//! Library re-exports tests
//!
//! Tests to verify that all required types are properly re-exported
//! from the crate root for ergonomic imports.

use std::path::PathBuf;

#[test]
fn test_architecture_reexport() {
    let _arch: msvc_kit::Architecture = msvc_kit::Architecture::X64;
    assert_eq!(_arch, msvc_kit::version::Architecture::X64);
}

#[test]
fn test_download_options_reexport() {
    let options = msvc_kit::DownloadOptions::default();
    assert_eq!(options.arch, msvc_kit::Architecture::X64);
}

#[test]
fn test_download_options_builder_reexport() {
    let options = msvc_kit::DownloadOptions::builder()
        .arch(msvc_kit::Architecture::X64)
        .build();
    assert_eq!(options.arch, msvc_kit::Architecture::X64);
}

#[test]
fn test_msvc_environment_reexport() {
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
    let _shell = msvc_kit::ShellType::PowerShell;
}

#[test]
fn test_error_types_reexport() {
    let _err: msvc_kit::Result<()> = Err(msvc_kit::MsvcKitError::Cancelled);
}

#[test]
fn test_download_functions_exist() {
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
    let _config = msvc_kit::MsvcKitConfig::default();
}

#[test]
fn test_download_all_reexport() {
    // Verify download_all is accessible from crate root
    // Just verify the function exists and is callable (type check)
    let _fn_ptr = msvc_kit::download_all;
}

#[test]
fn test_cache_manager_reexport() {
    // Verify CacheManager and FileSystemCacheManager are accessible
    let _: Option<msvc_kit::BoxedCacheManager> = None;

    // Verify the type exists and can be referenced
    fn _check_trait(_: &dyn msvc_kit::CacheManager) {}
}

#[test]
fn test_download_options_builder_cache_manager() {
    // Verify cache_manager builder method works
    let options = msvc_kit::DownloadOptions::builder()
        .arch(msvc_kit::Architecture::X64)
        .target_dir("/tmp/test")
        .build();

    assert!(options.cache_manager.is_none());
}

#[test]
#[allow(unreachable_code)]
fn test_bundle_types_reexport() {
    let _: fn(msvc_kit::BundleOptions) -> _ =
        |_| async { Ok::<msvc_kit::BundleResult, msvc_kit::MsvcKitError>(todo!()) };

    let _opts = msvc_kit::BundleOptions::default();
    assert_eq!(_opts.arch, msvc_kit::Architecture::X64);
}
