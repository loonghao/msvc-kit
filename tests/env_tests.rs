//! Environment and shell script tests

use msvc_kit::env::{generate_activation_script, get_env_vars, MsvcEnvironment};
use msvc_kit::installer::InstallInfo;
use msvc_kit::version::Architecture;
use msvc_kit::ShellType;
use std::path::PathBuf;

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

// ============================================================================
// MsvcEnvironment Tests
// ============================================================================

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
    assert!(!env.has_cl_exe());
}

#[test]
fn test_msvc_environment_cl_exe_path() {
    let env = create_test_environment();
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

// ============================================================================
// Environment Tests (Windows-specific)
// ============================================================================

#[cfg(windows)]
mod windows_env_tests {
    use super::*;

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

#[test]
fn test_shell_type_script_extension() {
    assert_eq!(ShellType::Cmd.script_extension(), "bat");
    assert_eq!(ShellType::PowerShell.script_extension(), "ps1");
    assert_eq!(ShellType::Bash.script_extension(), "sh");
}

#[test]
fn test_shell_type_detect() {
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

#[test]
fn test_generate_cmd_script() {
    let env = MsvcEnvironment {
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
    };
    let script = generate_activation_script(&env, ShellType::Cmd).unwrap();
    assert!(script.contains("@echo off"));
    assert!(script.contains("set \""));
    assert!(script.contains("MSVC Toolchain activated"));
}

#[test]
fn test_generate_powershell_script() {
    let env = MsvcEnvironment {
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
    };
    let script = generate_activation_script(&env, ShellType::PowerShell).unwrap();
    assert!(script.contains("$env:"));
    assert!(script.contains("Write-Host"));
    assert!(script.contains("MSVC Toolchain activated"));
}

#[test]
fn test_generate_bash_script() {
    let env = MsvcEnvironment {
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
    };
    let script = generate_activation_script(&env, ShellType::Bash).unwrap();
    assert!(script.contains("#!/bin/bash"));
    assert!(script.contains("export "));
    assert!(script.contains("MSVC Toolchain activated"));
}

#[test]
fn test_shell_type_equality() {
    assert_eq!(ShellType::Cmd, ShellType::Cmd);
    assert_eq!(ShellType::PowerShell, ShellType::PowerShell);
    assert_eq!(ShellType::Bash, ShellType::Bash);
    assert_ne!(ShellType::Cmd, ShellType::PowerShell);
}

// ============================================================================
// InstallInfo Tests
// ============================================================================

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
    assert!(!info.is_valid());
}

#[test]
fn test_install_info_total_size() {
    let info = create_test_install_info("msvc");
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
