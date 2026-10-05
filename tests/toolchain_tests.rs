//! Regression contracts for installed version selection and host/target separation.

use msvc_kit::{
    get_env_vars, query_installation, resolve_toolchain, Architecture, QueryComponent,
    QueryOptions, ToolchainRequest,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn install_versions(root: &Path, msvc: &[&str], sdk: &[&str]) {
    for version in msvc {
        std::fs::create_dir_all(root.join("VC/Tools/MSVC").join(version)).unwrap();
    }
    for version in sdk {
        std::fs::create_dir_all(root.join("Windows Kits/10/Include").join(version)).unwrap();
    }
}

fn request(root: &Path) -> ToolchainRequest {
    ToolchainRequest {
        install_dir: root.into(),
        arch: Architecture::X64,
        host_arch: Architecture::X64,
        ..Default::default()
    }
}

fn file(path: PathBuf) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"fixture").unwrap();
}

#[test]
fn latest_versions_are_numeric_and_requested_versions_are_never_replaced() {
    let root = TempDir::new().unwrap();
    install_versions(
        root.path(),
        &["14.9.99999", "14.10.100", "14.10.90", "14.36.1"],
        &["10.0.9999.0", "10.0.10000.0"],
    );
    let mut selected = request(root.path());
    assert_eq!(
        resolve_toolchain(&selected).unwrap().vc_tools_version,
        "14.36.1"
    );
    selected.msvc_version = Some("14.10".into());
    let env = resolve_toolchain(&selected).unwrap();
    assert_eq!(env.vc_tools_version, "14.10.100");
    assert_eq!(env.windows_sdk_version, "10.0.10000.0");
    selected.msvc_version = Some("14.9.99999".into());
    assert_eq!(
        resolve_toolchain(&selected).unwrap().vc_tools_version,
        "14.9.99999"
    );
    selected.msvc_version = Some("14.3".into());
    assert!(matches!(
        resolve_toolchain(&selected),
        Err(msvc_kit::MsvcKitError::VersionNotFound(_))
    ));
}

#[test]
fn explicit_missing_selectors_fail_even_when_no_component_is_installed() {
    let root = TempDir::new().unwrap();
    let mut selected = request(root.path());
    selected.msvc_version = Some("14.44".into());
    assert!(matches!(
        resolve_toolchain(&selected),
        Err(msvc_kit::MsvcKitError::VersionNotFound(_))
    ));
    install_versions(root.path(), &["14.44.1"], &[]);
    selected.sdk_version = Some("10.0.26100".into());
    assert!(matches!(
        resolve_toolchain(&selected),
        Err(msvc_kit::MsvcKitError::VersionNotFound(_))
    ));
    let query = QueryOptions::builder()
        .install_dir(root.path())
        .sdk_version("10.0.26100")
        .build();
    assert!(matches!(
        query_installation(&query),
        Err(msvc_kit::MsvcKitError::VersionNotFound(_))
    ));
}

#[test]
fn sdk_build_shorthand_is_a_complete_build_component_not_a_substring() {
    let root = TempDir::new().unwrap();
    install_versions(
        root.path(),
        &["14.44.1"],
        &["10.0.26100.0", "10.0.26100.10"],
    );
    let mut selected = request(root.path());
    selected.sdk_version = Some("26100".into());
    assert_eq!(
        resolve_toolchain(&selected).unwrap().windows_sdk_version,
        "10.0.26100.10"
    );
    selected.sdk_version = Some("6100".into());
    assert!(resolve_toolchain(&selected).is_err());
}

#[test]
fn cross_environment_uses_host_tools_target_libraries_and_cc_rs_target_overrides() {
    let root = TempDir::new().unwrap();
    install_versions(root.path(), &["14.44.1"], &["10.0.26100.0"]);
    let compiler = root
        .path()
        .join("VC/Tools/MSVC/14.44.1/bin/Hostx64/arm64/cl.exe");
    file(compiler);
    file(
        root.path()
            .join("VC/Tools/MSVC/14.44.1/bin/Hostx64/arm64/link.exe"),
    );
    file(
        root.path()
            .join("Windows Kits/10/bin/10.0.26100.0/x64/rc.exe"),
    );
    let mut selected = request(root.path());
    selected.arch = Architecture::Arm64;
    let env = resolve_toolchain(&selected).unwrap();
    assert!(env.bin_paths[0].ends_with("bin/Hostx64/arm64"));
    assert!(env.bin_paths[1].ends_with("bin/10.0.26100.0/x64"));
    assert!(env.lib_paths.iter().all(|path| path.ends_with("arm64")));
    let vars = get_env_vars(&env);
    assert_eq!(vars["CC"], vars["CC_aarch64_pc_windows_msvc"]);
    assert_eq!(vars["CXX"], vars["CXX_aarch64-pc-windows-msvc"]);
    assert!(vars.contains_key("CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_LINKER"));
    assert_eq!(vars["UCRTVersion"], "10.0.26100.0");
    let query = query_installation(
        &QueryOptions::builder()
            .install_dir(root.path())
            .arch(Architecture::Arm64)
            .host_arch(Architecture::X64)
            .build(),
    )
    .unwrap();
    assert_eq!(query.host_arch, "x64");
    assert_eq!(query.arch, "arm64");
    assert_eq!(query.fingerprint, env.fingerprint());
    assert!(query.tool_path("rc").unwrap().ends_with("x64/rc.exe"));
}

#[test]
fn fingerprint_is_portable_and_changes_with_versions_or_architecture() {
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    for root in [first.path(), second.path()] {
        install_versions(root, &["14.44.1"], &["10.0.26100.0"]);
    }
    let env = resolve_toolchain(&request(first.path())).unwrap();
    assert_eq!(
        env.fingerprint(),
        resolve_toolchain(&request(second.path()))
            .unwrap()
            .fingerprint()
    );
    let mut cross = request(second.path());
    cross.arch = Architecture::X86;
    assert_ne!(
        env.fingerprint(),
        resolve_toolchain(&cross).unwrap().fingerprint()
    );
    cross.arch = Architecture::X64;
    cross.host_arch = Architecture::Arm64;
    assert_ne!(
        env.fingerprint(),
        resolve_toolchain(&cross).unwrap().fingerprint()
    );
}

#[test]
fn component_only_queries_do_not_fabricate_other_component_metadata() {
    let root = TempDir::new().unwrap();
    install_versions(root.path(), &["14.44.1"], &["10.0.26100.0"]);
    file(
        root.path()
            .join("Windows Kits/10/bin/10.0.26100.0/x64/rc.exe"),
    );
    let query = query_installation(
        &QueryOptions::builder()
            .install_dir(root.path())
            .component(QueryComponent::Sdk)
            .host_arch(Architecture::X64)
            .arch(Architecture::Arm64)
            .build(),
    )
    .unwrap();
    assert!(query.msvc.is_none());
    assert!(!query.env_vars.contains_key("CC"));
    assert!(!query.env_vars.contains_key("VCINSTALLDIR"));
    assert!(query.env_vars["PATH"].ends_with("x64"));
    assert!(query.env_vars["LIB"].contains("arm64"));
    assert!(query.tools.contains_key("rc"));
    let query = query_installation(
        &QueryOptions::builder()
            .install_dir(root.path())
            .component(QueryComponent::Msvc)
            .build(),
    )
    .unwrap();
    assert!(query.sdk.is_none());
    assert!(!query.env_vars.contains_key("WindowsSDKVersion"));
    assert_eq!(query.env_vars["INCLUDE"].split(';').count(), 1);
}

#[test]
fn msvc_only_environment_does_not_assume_a_default_sdk() {
    let root = TempDir::new().unwrap();
    install_versions(root.path(), &["14.44.1"], &[]);
    let env = resolve_toolchain(&request(root.path())).unwrap();
    assert!(env.windows_sdk_version.is_empty());
    assert_eq!(env.include_paths.len(), 1);
    assert_eq!(env.lib_paths.len(), 1);
    assert_eq!(env.bin_paths.len(), 1);
    assert!(!get_env_vars(&env).contains_key("WindowsSdkDir"));
}

#[cfg(windows)]
#[test]
fn verbatim_installation_paths_are_normalized_for_compiler_environment_values() {
    let root = TempDir::new().unwrap();
    install_versions(root.path(), &["14.44.1"], &["10.0.26100.0"]);
    let verbatim = std::fs::canonicalize(root.path()).unwrap();
    assert!(verbatim.to_string_lossy().starts_with(r"\\?\"));
    let env = resolve_toolchain(&request(&verbatim)).unwrap();
    for value in get_env_vars(&env).values() {
        assert!(
            !value.contains(r"\\?\"),
            "MSVC environment cannot contain verbatim prefixes: {value}"
        );
    }
    let query = query_installation(&QueryOptions::builder().install_dir(verbatim).build()).unwrap();
    assert!(!query.install_dir.to_string_lossy().starts_with(r"\\?\"));
    assert!(!query
        .msvc_install_path()
        .unwrap()
        .to_string_lossy()
        .starts_with(r"\\?\"));
}
