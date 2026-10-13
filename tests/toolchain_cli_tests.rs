use std::process::Command;

fn install(root: &std::path::Path, version: &str) {
    std::fs::create_dir_all(root.join("VC/Tools/MSVC").join(version)).unwrap();
}

#[test]
fn cli_environment_obeys_configured_version_and_json_stays_flat() {
    let temp = tempfile::tempdir().unwrap();
    install(temp.path(), "14.40.33807");
    install(temp.path(), "14.44.35207");
    let config = temp.path().join("config.toml");
    let settings = msvc_kit::MsvcKitConfig {
        install_dir: temp.path().into(),
        default_msvc_version: Some("14.40".into()),
        ..Default::default()
    };
    std::fs::write(&config, toml::to_string(&settings).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_msvc-kit"))
        .args(["--config"])
        .arg(&config)
        .args(["env", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let vars: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(vars["VCToolsVersion"], "14.40.33807");
    assert!(vars.get("env_vars").is_none());
    let failed = Command::new(env!("CARGO_BIN_EXE_msvc-kit"))
        .args(["--config"])
        .arg(&config)
        .args(["env", "--msvc-version", "14.99", "--format", "json"])
        .output()
        .unwrap();
    assert!(!failed.status.success());
}

#[test]
fn doctor_failure_is_machine_readable_and_run_preserves_child_exit() {
    let temp = tempfile::tempdir().unwrap();
    install(temp.path(), "14.44.35207");
    // Exercise doctor/run with valid isolated settings. An empty TOML file is
    // an invalid explicit configuration and must fail before either command.
    let config = temp.path().join("config.toml");
    let settings = msvc_kit::MsvcKitConfig {
        install_dir: temp.path().into(),
        ..Default::default()
    };
    std::fs::write(&config, toml::to_string(&settings).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_msvc-kit"))
        .arg("--config")
        .arg(&config)
        .args(["doctor", "--dir"])
        .arg(temp.path())
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(10));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "msvc-kit.doctor.v1");
    assert_eq!(report["status"], "failed");
    let mut child = Command::new(env!("CARGO_BIN_EXE_msvc-kit"));
    child
        .arg("--config")
        .arg(config)
        .args(["run", "--dir"])
        .arg(temp.path())
        .arg("--");
    if cfg!(windows) {
        child.args(["cmd", "/D", "/C", "exit 23"]);
    } else {
        child.args(["sh", "-c", "exit 23"]);
    }
    assert_eq!(child.output().unwrap().status.code(), Some(23));
}

#[test]
fn cli_legacy_selection_uses_original_vs_and_sdk_roots() {
    let vs = tempfile::tempdir().unwrap();
    let sdk = tempfile::tempdir().unwrap();
    let file = |root: &std::path::Path, relative: &str, contents: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    };
    file(vs.path(), "VC/vcvarsall.bat", "fixture; never executed");
    file(
        vs.path(),
        "VC/include/crtversion.h",
        "#define _VC_CRT_MAJOR_VERSION 14\n#define _VC_CRT_MINOR_VERSION 0\n",
    );
    for path in [
        "VC/bin/x86_amd64/cl.exe",
        "VC/bin/x86_amd64/link.exe",
        "VC/bin/x86_amd64/lib.exe",
        "VC/include/vcruntime.h",
        "VC/include/vector",
        "VC/lib/amd64/msvcrt.lib",
        "VC/lib/amd64/vcruntime.lib",
        "VC/lib/amd64/msvcprt.lib",
    ] {
        file(vs.path(), path, "fixture; never executed");
    }
    for path in [
        "bin/x86/rc.exe",
        "bin/x86/mt.exe",
        "Include/10.0.14393.0/ucrt/stdio.h",
        "Include/10.0.14393.0/um/Windows.h",
        "Include/10.0.14393.0/shared/sdkddkver.h",
        "Lib/10.0.14393.0/ucrt/x64/ucrt.lib",
        "Lib/10.0.14393.0/um/x64/kernel32.lib",
    ] {
        file(sdk.path(), path, "fixture; never executed");
    }
    let config = vs.path().join("config.toml");
    std::fs::write(
        &config,
        toml::to_string(&msvc_kit::MsvcKitConfig::default()).unwrap(),
    )
    .unwrap();
    let command = |name: &str| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_msvc-kit"));
        cmd.arg("--config")
            .arg(&config)
            .arg(name)
            .arg("--dir")
            .arg(vs.path())
            .arg("--sdk-dir")
            .arg(sdk.path())
            .args([
                "--msvc-version",
                "14.0",
                "--sdk-version",
                "10.0.14393.0",
                "--host-arch",
                "x86",
                "--arch",
                "x64",
            ]);
        cmd
    };
    let env = command("env").args(["--format", "json"]).output().unwrap();
    assert!(
        env.status.success(),
        "{}",
        String::from_utf8_lossy(&env.stderr)
    );
    let vars: serde_json::Value = serde_json::from_slice(&env.stdout).unwrap();
    assert_eq!(vars["VCToolsVersion"], "14.0");
    assert_eq!(
        std::path::Path::new(vars["CC"].as_str().unwrap()),
        vs.path().join("VC/bin/x86_amd64/cl.exe")
    );
    assert_eq!(vars["WindowsSdkDir"], sdk.path().display().to_string());
    let query = command("query")
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    let selected: serde_json::Value = serde_json::from_slice(&query.stdout).unwrap();
    assert_eq!(selected["env_vars"], vars);
    assert_eq!(selected["msvc"]["version"], "14.0");
    assert_eq!(selected["host_arch"], "x86");
    assert_eq!(selected["arch"], "x64");
    let doctor = command("doctor")
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(report["status"], "passed");
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check["id"] == "version.msvc" && check["status"] == "skipped"));
    // Portable templates must never print a fictional modern tree for this selection.
    let setup = command("setup").arg("--script").output().unwrap();
    assert!(!setup.status.success());
    assert!(String::from_utf8_lossy(&setup.stderr).contains("use env or run"));
    let lock = command("lock")
        .arg("--output")
        .arg(vs.path().join("legacy.lock.json"))
        .output()
        .unwrap();
    assert!(!lock.status.success());
    assert!(!vs.path().join("legacy.lock.json").exists());
    assert!(String::from_utf8_lossy(&lock.stderr).contains("exact numeric version"));
}
