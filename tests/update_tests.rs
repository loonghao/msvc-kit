//! Tests for the self-update feature (axoupdater integration)
//!
//! These tests validate the update subcommand CLI interface and configuration.
//! Network-dependent tests are intentionally avoided for CI stability.

use rstest::rstest;
use std::process::Command;

/// Helper function to get the path to the msvc-kit binary
fn get_binary_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe()
        .expect("Failed to get current executable path")
        .parent()
        .expect("Failed to get parent directory")
        .to_path_buf();

    // Navigate from target/{debug|release}/deps to target/{debug|release}
    if path.ends_with("deps") {
        path.pop();
    }

    path.push(if cfg!(windows) {
        "msvc-kit.exe"
    } else {
        "msvc-kit"
    });

    path
}

/// Helper function to run msvc-kit command and capture output
fn run_command(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(get_binary_path()).args(args).output()
}

#[test]
fn test_update_help_exits_zero() {
    // update --help should exit with code 0
    let output = run_command(&["update", "--help"]).expect("Failed to run msvc-kit update --help");

    assert!(
        output.status.success(),
        "Expected exit code 0 for update --help, got: {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Update") || stdout.contains("update"),
        "Expected update help output to contain 'update'"
    );
}

#[test]
fn test_update_help_shows_check_flag() {
    let output = run_command(&["update", "--help"]).expect("Failed to run msvc-kit update --help");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("--check"),
        "Expected update help to show --check flag"
    );
}

#[test]
fn test_update_help_shows_version_flag() {
    let output = run_command(&["update", "--help"]).expect("Failed to run msvc-kit update --help");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("--version"),
        "Expected update help to show --version flag"
    );
}

#[rstest]
#[case("update")]
fn test_update_subcommand_is_registered(#[case] cmd: &str) {
    // Verify the update subcommand appears in the main help
    let output = run_command(&["--help"]).expect("Failed to run msvc-kit --help");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains(cmd),
        "Expected main help to list '{}' subcommand",
        cmd
    );
}

#[cfg(feature = "self-update")]
mod axoupdater_config_tests {
    use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};

    #[test]
    fn test_release_source_configuration() {
        let source = ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner: "loonghao".to_string(),
            name: "msvc-kit".to_string(),
            app_name: "msvc-kit".to_string(),
        };

        assert_eq!(source.owner, "loonghao");
        assert_eq!(source.name, "msvc-kit");
        assert_eq!(source.app_name, "msvc-kit");
        assert_eq!(source.release_type, ReleaseSourceType::GitHub);
    }

    #[test]
    fn test_updater_creation_with_source() {
        let source = ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner: "loonghao".to_string(),
            name: "msvc-kit".to_string(),
            app_name: "msvc-kit".to_string(),
        };

        let mut updater = AxoUpdater::new_for("msvc-kit");
        updater.set_release_source(source);

        // Setting current version should not error for valid semver
        let result = updater.set_current_version("0.2.5".parse().unwrap());
        assert!(
            result.is_ok(),
            "set_current_version should succeed for valid semver"
        );
    }

    #[test]
    fn test_update_request_variants() {
        use axoupdater::UpdateRequest;

        // Test Latest
        let _latest = UpdateRequest::Latest;

        // Test SpecificVersion
        let _specific = UpdateRequest::SpecificVersion("1.0.0".to_string());

        // Test SpecificTag
        let _tag = UpdateRequest::SpecificTag("v1.0.0".to_string());

        // Test LatestMaybePrerelease
        let _pre = UpdateRequest::LatestMaybePrerelease;
    }
}
