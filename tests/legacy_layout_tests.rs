//! Real directory fixtures only: no Microsoft binaries or native compilation.

use msvc_kit::doctor::{doctor_with_sdk_root, DoctorOptions, DoctorStatus};
use msvc_kit::{
    get_env_vars, query_installation_with_sdk_root, resolve_toolchain_with_sdk_root, Architecture,
    QueryComponent, QueryOptions, ToolchainRequest,
};
use std::path::Path;
use tempfile::TempDir;

fn file(path: impl AsRef<Path>, content: &str) {
    let path = path.as_ref();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn legacy() -> (TempDir, TempDir) {
    let vs = TempDir::new().unwrap();
    let sdk = TempDir::new().unwrap();
    file(
        vs.path().join("VC/vcvarsall.bat"),
        "fixture; never executed",
    );
    file(vs.path().join("VC/include/crtversion.h"), "#define _VC_CRT_MAJOR_VERSION 14\n#define _VC_CRT_MINOR_VERSION 0\n#define _VC_CRT_BUILD_VERSION 24215\n");
    for header in ["vcruntime.h", "vector"] {
        file(vs.path().join("VC/include").join(header), "fixture");
    }
    for dir in ["bin", "bin/amd64", "bin/x86_amd64", "bin/amd64_x86"] {
        for tool in ["cl.exe", "link.exe", "lib.exe"] {
            file(
                vs.path().join("VC").join(dir).join(tool),
                "fixture; never executed",
            );
        }
    }
    for dir in ["lib", "lib/amd64"] {
        for lib in ["msvcrt.lib", "vcruntime.lib", "msvcprt.lib"] {
            file(vs.path().join("VC").join(dir).join(lib), "fixture");
        }
    }
    sdk_fixture(sdk.path(), "10.0.14393.0", false);
    (vs, sdk)
}

fn sdk_fixture(root: &Path, version: &str, versioned: bool) {
    for (dir, name) in [
        ("ucrt", "stdio.h"),
        ("um", "Windows.h"),
        ("shared", "sdkddkver.h"),
    ] {
        file(
            root.join("Include").join(version).join(dir).join(name),
            "fixture",
        );
    }
    for arch in ["x86", "x64"] {
        for (dir, name) in [("ucrt", "ucrt.lib"), ("um", "kernel32.lib")] {
            file(
                root.join("Lib")
                    .join(version)
                    .join(dir)
                    .join(arch)
                    .join(name),
                "fixture",
            );
        }
        let bin = if versioned {
            root.join("bin").join(version)
        } else {
            root.join("bin")
        };
        for tool in ["rc.exe", "mt.exe"] {
            file(bin.join(arch).join(tool), "fixture; never executed");
        }
    }
}

fn request(vs: &Path, host: Architecture, target: Architecture) -> ToolchainRequest {
    ToolchainRequest {
        install_dir: vs.into(),
        host_arch: host,
        arch: target,
        msvc_version: Some("14.0".into()),
        sdk_version: Some("10.0.14393.0".into()),
    }
}

fn query(request: &ToolchainRequest, component: QueryComponent) -> QueryOptions {
    QueryOptions {
        install_dir: request.install_dir.clone(),
        host_arch: request.host_arch,
        arch: request.arch,
        msvc_version: request.msvc_version.clone(),
        sdk_version: request.sdk_version.clone(),
        component,
        ..Default::default()
    }
}

#[test]
fn legacy_native_and_cross_paths_agree_across_resolver_query_environment_and_doctor() {
    let (vs, sdk) = legacy();
    for (host, target, bin, lib, native) in [
        (Architecture::X64, Architecture::X64, "amd64", "amd64", None),
        (
            Architecture::X86,
            Architecture::X64,
            "x86_amd64",
            "amd64",
            Some(""),
        ),
        (
            Architecture::X64,
            Architecture::X86,
            "amd64_x86",
            "",
            Some("amd64"),
        ),
        (Architecture::X86, Architecture::X86, "", "", None),
    ] {
        let request = request(vs.path(), host, target);
        let env = resolve_toolchain_with_sdk_root(&request, Some(sdk.path())).unwrap();
        let result = query_installation_with_sdk_root(
            &query(&request, QueryComponent::All),
            Some(sdk.path()),
        )
        .unwrap();
        let vc = vs.path().join("VC");
        assert_eq!(env.vc_install_dir, vc);
        assert_eq!(env.vc_tools_install_dir, vc);
        assert_eq!(env.vc_tools_version, "14.0");
        assert_eq!(env.bin_paths[0], vc.join("bin").join(bin));
        assert_eq!(env.lib_paths[0], vc.join("lib").join(lib));
        assert_eq!(
            result.msvc.as_ref().unwrap().lib_paths,
            vec![env.lib_paths[0].clone()]
        );
        assert_eq!(result.tool_path("cl"), env.cl_exe_path().as_ref());
        assert_eq!(
            result.tool_path("rc"),
            Some(&sdk.path().join("bin").join(host.to_string()).join("rc.exe"))
        );
        assert_eq!(result.env_vars, get_env_vars(&env));
        assert_eq!(result.fingerprint, env.fingerprint());
        assert!(!env.supports_activation_scripts());
        if let Some(native) = native {
            assert_eq!(env.bin_paths[1], vc.join("bin").join(native));
        }
        assert_eq!(
            result.env_vars["WindowsSdkVerBinPath"],
            sdk.path().join("bin").display().to_string()
        );
        let report = doctor_with_sdk_root(
            &DoctorOptions {
                toolchain: request,
                compile_probe: false,
            },
            Some(sdk.path()),
        );
        assert!(report.is_success(), "{}", report.format_summary());
        assert!(report
            .checks
            .iter()
            .any(|check| check.id == "version.msvc" && check.status == DoctorStatus::Skipped));
        assert!(report.checks.iter().any(|check| check.id == "lib.runtime"
            && check.path == Some(vc.join("lib").join(lib).join("msvcrt.lib"))));
    }
}

#[test]
fn legacy_selectors_do_not_claim_patch_versions_or_choose_other_toolsets() {
    let (vs, sdk) = legacy();
    let mut req = request(vs.path(), Architecture::X64, Architecture::X64);
    std::fs::create_dir_all(vs.path().join("VC/Tools/MSVC/14.44.99999")).unwrap();
    assert_eq!(
        resolve_toolchain_with_sdk_root(&req, Some(sdk.path()))
            .unwrap()
            .vc_tools_version,
        "14.0"
    );
    for selector in ["14.0.24215", "14.00", "14.1"] {
        req.msvc_version = Some(selector.into());
        assert!(
            resolve_toolchain_with_sdk_root(&req, Some(sdk.path())).is_err(),
            "{selector}"
        );
    }
    req.msvc_version = None;
    assert_eq!(
        resolve_toolchain_with_sdk_root(&req, Some(sdk.path()))
            .unwrap()
            .vc_tools_version,
        "14.44.99999"
    );
    assert!(msvc_kit::version::is_msvc_installed(vs.path(), "14.0"));
    assert!(!msvc_kit::version::is_msvc_installed(
        vs.path(),
        "14.0.24215"
    ));
}

#[test]
fn ambiguous_or_missing_legacy_metadata_is_not_treated_as_v140() {
    let (vs, sdk) = legacy();
    let req = request(vs.path(), Architecture::X64, Architecture::X64);
    for header in [
        "#define _VC_CRT_MAJOR_VERSION 12\n#define _VC_CRT_MINOR_VERSION 0",
        "#define _VC_CRT_MAJOR_VERSION 14\n#define _VC_CRT_MINOR_VERSION 1",
        "fixture",
    ] {
        file(vs.path().join("VC/include/crtversion.h"), header);
        assert!(resolve_toolchain_with_sdk_root(&req, Some(sdk.path())).is_err());
    }
}

#[test]
fn legacy_renamed_bins_or_libraries_fail_without_modern_aliases_or_host_fallback() {
    let (vs, sdk) = legacy();
    let req = request(vs.path(), Architecture::X86, Architecture::X64);
    // Native x86 cl.exe still exists but may not satisfy x86/x64 selection.
    std::fs::rename(
        vs.path().join("VC/bin/x86_amd64"),
        vs.path().join("VC/bin/x64"),
    )
    .unwrap();
    let error = resolve_toolchain_with_sdk_root(&req, Some(sdk.path()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("x86_amd64") && error.contains("refusing"),
        "{error}"
    );
    let req = request(vs.path(), Architecture::X64, Architecture::X64);
    std::fs::rename(vs.path().join("VC/lib/amd64"), vs.path().join("VC/lib/x64")).unwrap();
    let report = doctor_with_sdk_root(
        &DoctorOptions {
            toolchain: req,
            compile_probe: false,
        },
        Some(sdk.path()),
    );
    assert!(!report.is_success());
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "lib.runtime" && check.status == DoctorStatus::Failed));
}

#[test]
fn legacy_rejects_unsupported_host_and_target_architectures() {
    let (vs, sdk) = legacy();
    for (host, target) in [
        (Architecture::Arm64, Architecture::X64),
        (Architecture::X64, Architecture::Arm64),
        (Architecture::X86, Architecture::Arm),
    ] {
        let error =
            resolve_toolchain_with_sdk_root(&request(vs.path(), host, target), Some(sdk.path()))
                .unwrap_err()
                .to_string();
        assert!(error.contains("supports x86/x64"), "{error}");
    }
}

#[test]
fn early_sdk_versioned_bins_win_and_modern_sdk_never_falls_back_to_old_bins() {
    let (vs, sdk) = legacy();
    let mut req = request(vs.path(), Architecture::X64, Architecture::X64);
    sdk_fixture(sdk.path(), "10.0.14393.0", true);
    assert_eq!(
        resolve_toolchain_with_sdk_root(&req, Some(sdk.path()))
            .unwrap()
            .rc_exe_path(),
        Some(sdk.path().join("bin/10.0.14393.0/x64/rc.exe"))
    );
    sdk_fixture(sdk.path(), "10.0.22621.0", false);
    req.sdk_version = Some("10.0.22621.0".into());
    assert!(resolve_toolchain_with_sdk_root(&req, Some(sdk.path()))
        .unwrap()
        .rc_exe_path()
        .is_none());
    assert!(!doctor_with_sdk_root(
        &DoctorOptions {
            toolchain: req,
            compile_probe: false
        },
        Some(sdk.path())
    )
    .is_success());
}

#[test]
fn explicit_sdk_root_is_strict_and_component_queries_stay_separate() {
    let (vs, sdk) = legacy();
    let req = request(vs.path(), Architecture::X86, Architecture::X64);
    let msvc =
        query_installation_with_sdk_root(&query(&req, QueryComponent::Msvc), Some(sdk.path()))
            .unwrap();
    assert!(msvc.sdk.is_none());
    assert!(!msvc.env_vars.contains_key("WindowsSdkDir"));
    assert!(msvc.env_vars["PATH"].contains(&vs.path().join("VC/bin").display().to_string()));
    assert_eq!(msvc.msvc.unwrap().bin_paths.len(), 2);
    let sdk_only =
        query_installation_with_sdk_root(&query(&req, QueryComponent::Sdk), Some(sdk.path()))
            .unwrap();
    assert!(sdk_only.msvc.is_none());
    assert!(!sdk_only.env_vars.contains_key("VCINSTALLDIR"));
    assert_eq!(
        sdk_only.env_vars["PATH"],
        sdk.path().join("bin/x86").display().to_string()
    );
    let mut missing = req.clone();
    let absent = sdk.path().join("missing");
    assert!(resolve_toolchain_with_sdk_root(&missing, Some(&absent)).is_err());
    assert!(
        query_installation_with_sdk_root(&query(&missing, QueryComponent::All), Some(&absent))
            .is_err()
    );
    let empty = TempDir::new().unwrap();
    assert!(resolve_toolchain_with_sdk_root(&missing, Some(empty.path())).is_err());
    for selector in ["10.0", "10.0.99999.0"] {
        missing.sdk_version = Some(selector.into());
        assert!(resolve_toolchain_with_sdk_root(&missing, Some(sdk.path())).is_err());
    }
}

#[test]
fn compile_probe_is_never_claimed_on_non_windows_fixture_hosts() {
    if cfg!(windows) {
        return;
    }
    let (vs, sdk) = legacy();
    let report = doctor_with_sdk_root(
        &DoctorOptions {
            toolchain: request(vs.path(), Architecture::X64, Architecture::X64),
            compile_probe: true,
        },
        Some(sdk.path()),
    );
    assert!(!report.is_success());
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "probe.compile"
            && check.message.contains("requires a Windows host")));
}

#[test]
fn all_supported_early_sdk_versions_keep_host_selection_and_partial_versioned_roots() {
    let (vs, sdk) = legacy();
    for version in ["10.0.10240.0", "10.0.10586.0", "10.0.14393.0"] {
        sdk_fixture(sdk.path(), version, false);
        let mut req = request(vs.path(), Architecture::X86, Architecture::X64);
        req.sdk_version = Some(version.into());
        assert_eq!(
            resolve_toolchain_with_sdk_root(&req, Some(sdk.path()))
                .unwrap()
                .rc_exe_path(),
            Some(sdk.path().join("bin/x86/rc.exe"))
        );
        let report = doctor_with_sdk_root(
            &DoctorOptions {
                toolchain: req.clone(),
                compile_probe: false,
            },
            Some(sdk.path()),
        );
        assert!(report
            .checks
            .iter()
            .any(|check| check.id == "version.sdk_tools" && check.status == DoctorStatus::Skipped));
        std::fs::create_dir_all(sdk.path().join("bin").join(version)).unwrap();
        assert!(resolve_toolchain_with_sdk_root(&req, Some(sdk.path()))
            .unwrap()
            .rc_exe_path()
            .is_none());
    }
}

#[test]
fn missing_legacy_sdk_host_tools_are_not_replaced_by_target_tools() {
    let (vs, sdk) = legacy();
    std::fs::remove_file(sdk.path().join("bin/x86/rc.exe")).unwrap();
    let req = request(vs.path(), Architecture::X86, Architecture::X64);
    assert!(resolve_toolchain_with_sdk_root(&req, Some(sdk.path()))
        .unwrap()
        .rc_exe_path()
        .is_none());
    assert!(
        query_installation_with_sdk_root(&query(&req, QueryComponent::All), Some(sdk.path()))
            .unwrap()
            .tool_path("rc")
            .is_none()
    );
    let report = doctor_with_sdk_root(
        &DoctorOptions {
            toolchain: req,
            compile_probe: false,
        },
        Some(sdk.path()),
    );
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "tool.rc" && check.status == DoctorStatus::Failed));
}

#[test]
fn original_public_struct_literals_and_portable_entrypoints_remain_compatible() {
    let (vs, _external_sdk) = legacy();
    sdk_fixture(&vs.path().join("Windows Kits/10"), "10.0.14393.0", false);
    let request = ToolchainRequest {
        install_dir: vs.path().into(),
        arch: Architecture::X64,
        host_arch: Architecture::X64,
        msvc_version: Some("14.0".into()),
        sdk_version: Some("10.0.14393.0".into()),
    };
    let options = QueryOptions {
        install_dir: request.install_dir.clone(),
        arch: request.arch,
        host_arch: request.host_arch,
        component: QueryComponent::All,
        property: msvc_kit::QueryProperty::All,
        msvc_version: request.msvc_version.clone(),
        sdk_version: request.sdk_version.clone(),
    };
    let env = msvc_kit::resolve_toolchain(&request).unwrap();
    let query = msvc_kit::query_installation(&options).unwrap();
    assert_eq!(query.env_vars, get_env_vars(&env));
    assert!(msvc_kit::doctor(&DoctorOptions {
        toolchain: request,
        compile_probe: false
    })
    .is_success());
}
