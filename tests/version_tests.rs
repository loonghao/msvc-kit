//! Version and Architecture tests

use msvc_kit::version::{
    is_msvc_installed, is_sdk_installed, Architecture, InstalledVersion, MsvcVersion, SdkVersion,
};

// ============================================================================
// Architecture Tests
// ============================================================================

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

// ============================================================================
// Version Tests
// ============================================================================

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

    std::fs::create_dir_all(msvc_dir.join("14.44.33807")).unwrap();
    std::fs::create_dir_all(msvc_dir.join("14.43.34808")).unwrap();

    let versions = msvc_kit::version::list_installed_msvc(temp_dir.path());
    assert_eq!(versions.len(), 2);
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

    std::fs::create_dir_all(sdk_dir.join("10.0.26100.0")).unwrap();
    std::fs::create_dir_all(sdk_dir.join("10.0.22621.0")).unwrap();

    let versions = msvc_kit::version::list_installed_sdk(temp_dir.path());
    assert_eq!(versions.len(), 2);
    assert!(versions[0].version.starts_with("10.0.26100"));
    assert!(versions[0].is_latest);
}

// ============================================================================
// Version Installation Check Tests
// ============================================================================

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
    assert!(is_sdk_installed(temp_dir.path(), "26100"));
}

// ============================================================================
// Version Display Tests
// ============================================================================

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

// ============================================================================
// InstalledVersion Tests
// ============================================================================

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
