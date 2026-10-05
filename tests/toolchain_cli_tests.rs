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
    let config = temp.path().join("empty.toml");
    std::fs::write(&config, "").unwrap();
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
