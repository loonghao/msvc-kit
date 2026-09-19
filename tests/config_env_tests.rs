use std::process::Command;

#[test]
fn cli_reads_install_directory_from_environment() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("kit with spaces");
    let output = Command::new(env!("CARGO_BIN_EXE_msvc-kit"))
        .arg("config")
        .env("MSVC_KIT_DIR", &target)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(&target.display().to_string()));
    assert!(
        !target.exists(),
        "reading configuration must not install anything"
    );
}
