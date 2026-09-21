//! Configuration location tests
//!
//! Covers the reported cache directory and the switches that decide which
//! `config.toml` a run uses (`--config`, `MSVC_KIT_CONFIG`, `MSVC_KIT_PORTABLE`).

use std::path::{Path, PathBuf};
use std::process::Command;

const CACHE_DIR_LABEL: &str = "Cache directory: ";
const CONFIG_ENV_VAR: &str = "MSVC_KIT_CONFIG";
const PORTABLE_ENV_VAR: &str = "MSVC_KIT_PORTABLE";

/// Run the CLI with a clean configuration environment
fn msvc_kit() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_msvc-kit"));
    command
        .env_remove(CONFIG_ENV_VAR)
        .env_remove(PORTABLE_ENV_VAR)
        .env_remove("MSVC_KIT_DIR");
    command
}

fn stdout_of(output: &std::process::Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// The `Config file:` line reports the resolved path
fn reported_config_path(output: &std::process::Output) -> PathBuf {
    let stdout = stdout_of(output);
    let line = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("Config file:"))
        .unwrap_or_else(|| panic!("no config path in output:\n{stdout}"));
    let value = line
        .split_once("Config file:")
        .expect("separator")
        .1
        .split(" (")
        .next()
        .expect("source suffix")
        .trim();
    PathBuf::from(value)
}

/// Write a complete configuration file; `MsvcKitConfig` has required fields
fn write_config(path: &Path, install_dir: &str) {
    let content = format!(
        "install_dir = '{install_dir}'\n\
         default_msvc_version = '14.44'\n\
         default_sdk_version = '10.0.26100.0'\n\
         default_arch = 'x64'\n\
         verify_hashes = true\n\
         parallel_downloads = 4\n\
         cache_dir = '{install_dir}/cache'\n"
    );
    std::fs::write(path, content).unwrap();
}

/// Run `msvc-kit config` and return the reported cache directory.
fn reported_cache_dir(install_dir: Option<&Path>) -> String {
    let mut command = msvc_kit();
    command.arg("config");

    if let Some(dir) = install_dir {
        command.env("MSVC_KIT_DIR", dir);
    }

    let output = command.output().unwrap();
    let stdout = stdout_of(&output);
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
    let output = msvc_kit()
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

#[test]
fn config_flag_selects_the_configuration_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("custom.toml");

    let output = msvc_kit()
        .args(["--config", &config_path.display().to_string()])
        .args(["config", "--set-dir", "D:/from-flag"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(reported_config_path(&output), config_path);
    assert!(config_path.is_file(), "config file must be created");
    assert!(std::fs::read_to_string(&config_path)
        .unwrap()
        .contains("from-flag"));
}

#[test]
fn config_flag_accepts_a_directory() {
    let dir = tempfile::tempdir().unwrap();

    let output = msvc_kit()
        .args(["--config", &dir.path().display().to_string()])
        .args(["config", "--set-dir", "D:/from-dir"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        reported_config_path(&output),
        dir.path().join("config.toml")
    );
}

#[test]
fn config_environment_variable_selects_the_configuration_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("from-env.toml");
    write_config(&config_path, "D:/stored-in-env-file");

    // Reading: the stored install directory must come from the env file.
    let output = msvc_kit()
        .arg("config")
        .env(CONFIG_ENV_VAR, &config_path)
        .output()
        .unwrap();
    let stdout = stdout_of(&output);
    assert_eq!(reported_config_path(&output), config_path);
    assert!(stdout.contains("D:/stored-in-env-file"));

    // Writing: updates land in the same file.
    let output = msvc_kit()
        .args(["config", "--set-dir", "D:/updated-in-env-file"])
        .env(CONFIG_ENV_VAR, &config_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(std::fs::read_to_string(&config_path)
        .unwrap()
        .contains("updated-in-env-file"));
}

#[test]
fn config_flag_wins_over_the_environment_variable() {
    let dir = tempfile::tempdir().unwrap();
    let flag_path = dir.path().join("flag.toml");
    let env_path = dir.path().join("env.toml");

    let output = msvc_kit()
        .args(["--config", &flag_path.display().to_string()])
        .args(["config", "--set-dir", "D:/from-flag"])
        .env(CONFIG_ENV_VAR, &env_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(reported_config_path(&output), flag_path);
    assert!(!env_path.exists(), "the env path must stay untouched");
}

#[test]
fn portable_environment_variable_relocates_the_configuration() {
    let exe_dir = Path::new(env!("CARGO_BIN_EXE_msvc-kit"))
        .parent()
        .expect("exe directory")
        .to_path_buf();

    // Read-only check: portable mode must not create files on its own.
    let output = msvc_kit()
        .arg("config")
        .env(PORTABLE_ENV_VAR, "1")
        .output()
        .unwrap();

    let stdout = stdout_of(&output);
    assert_eq!(reported_config_path(&output), exe_dir.join("config.toml"));
    assert!(stdout.contains("portable mode"));
}

#[test]
fn empty_portable_environment_variable_is_ignored() {
    let output = msvc_kit()
        .arg("config")
        .env(PORTABLE_ENV_VAR, "")
        .output()
        .unwrap();

    let stdout = stdout_of(&output);
    assert!(
        !stdout.contains("portable mode"),
        "empty value must not enable portable mode:\n{stdout}"
    );
    assert_ne!(
        reported_config_path(&output),
        Path::new(env!("CARGO_BIN_EXE_msvc-kit"))
            .parent()
            .unwrap()
            .join("config.toml")
    );
}
