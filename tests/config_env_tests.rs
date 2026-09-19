use std::path::Path;
use std::process::Command;

const CACHE_DIR_LABEL: &str = "Cache directory: ";

/// Run `msvc-kit config` and return the reported cache directory.
fn reported_cache_dir(install_dir: Option<&Path>) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_msvc-kit"));
    command.arg("config").env_remove("MSVC_KIT_DIR");

    if let Some(dir) = install_dir {
        command.env("MSVC_KIT_DIR", dir);
    }

    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "msvc-kit config failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix(CACHE_DIR_LABEL))
        .unwrap_or_else(|| panic!("cache directory missing from output:\n{stdout}"))
        .trim()
        .to_string()
}

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

#[test]
fn cli_reports_cache_directory() {
    let cache_dir = reported_cache_dir(None);
    assert!(
        !cache_dir.is_empty(),
        "the cache directory must be reported"
    );
}

#[test]
fn cli_cache_directory_follows_install_directory_override() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("relocated kit");

    let default_cache_dir = reported_cache_dir(None);
    let relocated_cache_dir = reported_cache_dir(Some(&target));

    assert_eq!(
        relocated_cache_dir,
        target.join("cache").display().to_string(),
        "the cache must follow MSVC_KIT_DIR"
    );
    assert_ne!(
        relocated_cache_dir, default_cache_dir,
        "the override must move the cache away from the default location"
    );
    assert!(
        !target.exists(),
        "reading configuration must not create directories"
    );
}
