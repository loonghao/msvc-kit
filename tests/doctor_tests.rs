//! Diagnostics must separate file evidence from opt-in compiler execution.

use msvc_kit::{doctor, Architecture, DoctorOptions, DoctorStatus, ToolchainRequest};
use std::path::Path;
use tempfile::TempDir;

fn file(root: &Path, relative: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"read-only fixture, not an executable").unwrap();
}

fn complete_fixture(root: &Path, target: &str) {
    for name in ["cl", "link", "lib"] {
        file(
            root,
            &format!("VC/Tools/MSVC/14.44.1/bin/Hostx64/{target}/{name}.exe"),
        );
    }
    for name in ["rc", "mt"] {
        file(
            root,
            &format!("Windows Kits/10/bin/10.0.26100.0/x64/{name}.exe"),
        );
    }
    for name in ["vcruntime.h", "vector"] {
        file(root, &format!("VC/Tools/MSVC/14.44.1/include/{name}"));
    }
    for name in ["msvcrt.lib", "vcruntime.lib", "msvcprt.lib"] {
        file(root, &format!("VC/Tools/MSVC/14.44.1/lib/{target}/{name}"));
    }
    for (dir, name) in [
        ("ucrt", "stdio.h"),
        ("um", "Windows.h"),
        ("shared", "sdkddkver.h"),
    ] {
        file(
            root,
            &format!("Windows Kits/10/Include/10.0.26100.0/{dir}/{name}"),
        );
    }
    for (dir, name) in [("ucrt", "ucrt.lib"), ("um", "kernel32.lib")] {
        file(
            root,
            &format!("Windows Kits/10/Lib/10.0.26100.0/{dir}/{target}/{name}"),
        );
    }
}

fn options(root: &Path) -> DoctorOptions {
    DoctorOptions {
        toolchain: ToolchainRequest {
            install_dir: root.into(),
            host_arch: Architecture::X64,
            arch: Architecture::X64,
            ..Default::default()
        },
        compile_probe: false,
    }
}

#[test]
fn default_diagnostics_are_read_only_and_report_stable_json() {
    let root = TempDir::new().unwrap();
    complete_fixture(root.path(), "x64");
    let report = doctor(&options(root.path()));
    assert!(report.is_success(), "{}", report.format_summary());
    assert!(report
        .checks
        .iter()
        .all(|check| !check.id.starts_with("probe.")));
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["schema"], "msvc-kit.doctor.v1");
    assert_eq!(json["status"], "passed");
    assert_eq!(json["toolchain"]["msvc"]["version"], "14.44.1");
    assert_eq!(json["toolchain"]["host_arch"], "x64");
    assert_eq!(json["toolchain"]["fingerprint"].as_str().unwrap().len(), 64);
    assert!(!root.path().join("probe.cpp").exists());
}

#[test]
fn missing_tool_or_target_library_fails_and_skips_compile() {
    let root = TempDir::new().unwrap();
    complete_fixture(root.path(), "x64");
    std::fs::remove_file(
        root.path()
            .join("Windows Kits/10/bin/10.0.26100.0/x64/mt.exe"),
    )
    .unwrap();
    std::fs::remove_file(
        root.path()
            .join("Windows Kits/10/Lib/10.0.26100.0/um/x64/kernel32.lib"),
    )
    .unwrap();
    let mut requested = options(root.path());
    requested.compile_probe = true;
    let report = doctor(&requested);
    assert_eq!(report.status, DoctorStatus::Failed);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "tool.mt" && check.status == DoctorStatus::Failed));
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "lib.windows" && check.status == DoctorStatus::Failed));
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "probe.compile" && check.status == DoctorStatus::Skipped));
}

#[test]
fn executable_directory_does_not_pass_file_checks() {
    let root = TempDir::new().unwrap();
    complete_fixture(root.path(), "x64");
    let cl = root
        .path()
        .join("VC/Tools/MSVC/14.44.1/bin/Hostx64/x64/cl.exe");
    std::fs::remove_file(&cl).unwrap();
    std::fs::create_dir(cl).unwrap();
    let report = doctor(&options(root.path()));
    assert!(!report.is_success());
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "tool.cl" && check.status == DoctorStatus::Failed));
}

#[test]
fn explicit_selection_failure_is_a_json_report() {
    let root = TempDir::new().unwrap();
    let mut requested = options(root.path());
    requested.toolchain.msvc_version = Some("14.99".into());
    let report = doctor(&requested);
    assert_eq!(report.status, DoctorStatus::Failed);
    assert_eq!(report.checks[0].id, "resolution");
    assert!(report.checks[0].message.contains("14.99"));
    assert!(report.toolchain.is_none());
}

#[test]
fn opt_in_compile_failure_cannot_be_reported_as_success() {
    let root = TempDir::new().unwrap();
    complete_fixture(root.path(), "x64");
    let mut requested = options(root.path());
    requested.compile_probe = true;
    // The file evidence is complete, but the fixture compiler is not executable.
    // Only the explicit probe discovers this and must stop before linking.
    let report = doctor(&requested);
    assert_eq!(report.status, DoctorStatus::Failed);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "probe.compile" && check.status == DoctorStatus::Failed));
    assert!(!report.checks.iter().any(|check| check.id == "probe.link"));
}

#[test]
fn cross_diagnostics_require_target_libraries_and_host_sdk_executables() {
    let root = TempDir::new().unwrap();
    complete_fixture(root.path(), "arm64");
    let mut requested = options(root.path());
    requested.toolchain.arch = Architecture::Arm64;
    let report = doctor(&requested);
    assert!(report.is_success(), "{}", report.format_summary());
    let rc = report
        .checks
        .iter()
        .find(|check| check.id == "tool.rc")
        .unwrap();
    assert!(rc.path.as_ref().unwrap().ends_with("x64/rc.exe"));
    let library = report
        .checks
        .iter()
        .find(|check| check.id == "lib.windows")
        .unwrap();
    assert!(library
        .path
        .as_ref()
        .unwrap()
        .ends_with("arm64/kernel32.lib"));
}

#[test]
fn absent_sdk_is_reported_instead_of_using_an_assumed_version() {
    let root = TempDir::new().unwrap();
    file(root.path(), "VC/Tools/MSVC/14.44.1/include/vector");
    let report = doctor(&options(root.path()));
    assert!(!report.is_success());
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "sdk" && check.status == DoctorStatus::Failed));
    assert!(report.toolchain.as_ref().unwrap().sdk.is_none());
}
