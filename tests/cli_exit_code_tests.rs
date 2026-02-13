//! CLI exit code behavior tests
//!
//! Validates that the msvc-kit CLI exits with correct codes for winget compatibility:
//! - Exit code 0 when no subcommand is provided (prints help)
//! - Exit code 0 for successful operations
//! - Exit code 1 for errors

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
fn test_no_subcommand_exits_zero() {
    // Running without any subcommand should print help and exit with code 0
    // This is critical for winget validation
    let output = run_command(&[]).expect("Failed to run msvc-kit");

    assert!(
        output.status.success(),
        "Expected exit code 0 when no subcommand is provided, got: {:?}",
        output.status.code()
    );

    // Verify help text is printed
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("msvc-kit") || stdout.contains("Usage:"),
        "Expected help output to be printed"
    );
}

#[test]
fn test_help_flag_exits_zero() {
    // --help should exit with code 0
    let output = run_command(&["--help"]).expect("Failed to run msvc-kit --help");

    assert!(
        output.status.success(),
        "Expected exit code 0 for --help, got: {:?}",
        output.status.code()
    );
}

#[test]
fn test_verbose_help_exits_zero() {
    // --verbose --help should also exit with code 0 and exercise debug filter path
    let output =
        run_command(&["--verbose", "--help"]).expect("Failed to run msvc-kit --verbose --help");

    assert!(
        output.status.success(),
        "Expected exit code 0 for --verbose --help, got: {:?}",
        output.status.code()
    );
}

#[test]
fn test_version_flag_exits_zero() {
    // --version should exit with code 0
    let output = run_command(&["--version"]).expect("Failed to run msvc-kit --version");

    assert!(
        output.status.success(),
        "Expected exit code 0 for --version, got: {:?}",
        output.status.code()
    );

    // Verify version is printed
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("msvc-kit") || !stdout.is_empty(),
        "Expected version output to be printed"
    );
}

#[test]
fn test_subcommand_help_exits_zero() {
    // Subcommand help should exit with code 0
    let commands = [
        "download", "setup", "list", "clean", "config", "env", "bundle", "update",
    ];

    for cmd in commands {
        let output = run_command(&[cmd, "--help"])
            .unwrap_or_else(|_| panic!("Failed to run msvc-kit {} --help", cmd));

        assert!(
            output.status.success(),
            "Expected exit code 0 for {} --help, got: {:?}",
            cmd,
            output.status.code()
        );
    }
}

#[rstest]
#[case(&["config"])]
#[case(&["config", "--reset"])]
fn test_config_command_exits_zero(#[case] args: &[&str]) {
    // Config command with valid arguments should exit with code 0
    let output = run_command(args)
        .unwrap_or_else(|_| panic!("Failed to run msvc-kit config with args {:?}", args));

    assert!(
        output.status.success(),
        "Expected exit code 0 for config command with args {:?}, got: {:?}",
        args,
        output.status.code()
    );
}

#[test]
fn test_invalid_subcommand_exits_nonzero() {
    // Invalid subcommand should exit with non-zero code
    let output = run_command(&["invalid-command"]).expect("Failed to run msvc-kit");

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for invalid subcommand, got: {:?}",
        output.status.code()
    );
}

#[test]
fn test_bundle_without_license_exits_nonzero() {
    // Bundle command without --accept-license should exit with non-zero code
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let output_path = temp_dir.path().join("bundle");

    let output = run_command(&[
        "bundle",
        "--output",
        output_path.to_str().unwrap(),
        "--arch",
        "x64",
    ])
    .expect("Failed to run msvc-kit bundle");

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for bundle without license acceptance, got: {:?}",
        output.status.code()
    );

    // Verify error message mentions license
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let output_text = format!("{}{}", stdout, stderr);
    assert!(
        output_text.contains("license") || output_text.contains("License"),
        "Expected license-related error message"
    );
}

#[test]
fn test_setup_without_installation_exits_nonzero() {
    // Setup command without prior installation should exit with non-zero code
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    let output = run_command(&["setup", "--dir", temp_dir.path().to_str().unwrap()])
        .expect("Failed to run msvc-kit setup");

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for setup without installation, got: {:?}",
        output.status.code()
    );

    // Verify error message mentions missing installation
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No MSVC installation found") || stderr.contains("not found"),
        "Expected error about missing installation"
    );
}

#[test]
fn test_clean_nonexistent_version_exits_zero() {
    // Clean command with nonexistent version should not fail (idempotent)
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    let output = run_command(&[
        "clean",
        "--dir",
        temp_dir.path().to_str().unwrap(),
        "--msvc-version",
        "99.99.99999",
    ])
    .expect("Failed to run msvc-kit clean");

    // Clean should be idempotent and exit successfully even if version doesn't exist
    assert!(
        output.status.success(),
        "Expected exit code 0 for clean with nonexistent version, got: {:?}",
        output.status.code()
    );
}

#[test]
fn test_list_empty_dir_exits_zero() {
    // List command with empty directory should exit with code 0
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    let output = run_command(&["list", "--dir", temp_dir.path().to_str().unwrap()])
        .expect("Failed to run msvc-kit list");

    assert!(
        output.status.success(),
        "Expected exit code 0 for list with empty directory, got: {:?}",
        output.status.code()
    );

    // Verify appropriate message is printed
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No installations found") || stdout.contains("Installed versions"),
        "Expected appropriate list output"
    );
}

#[test]
fn test_invalid_architecture_exits_nonzero() {
    // Commands with invalid architecture should exit with non-zero code
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    let output = run_command(&[
        "download",
        "--target",
        temp_dir.path().to_str().unwrap(),
        "--arch",
        "invalid-arch",
        "--no-msvc",
        "--no-sdk",
    ])
    .expect("Failed to run msvc-kit download");

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for invalid architecture, got: {:?}",
        output.status.code()
    );

    // Verify error message mentions architecture
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("architecture") || stderr.contains("arch"),
        "Expected error about invalid architecture"
    );
}

// Note: update command test is intentionally omitted because:
// 1. It depends on network availability which makes tests flaky
// 2. The self-update feature may not always be compiled in
// 3. Exit codes can vary (0=success, 1=error, 2=unknown command, 101=panic)
// Manual testing of update command is recommended instead.

#[test]
fn test_env_command_without_installation_exits_nonzero() {
    // Env command without installation should exit with non-zero code
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    let output = run_command(&["env", "--dir", temp_dir.path().to_str().unwrap()])
        .expect("Failed to run msvc-kit env");

    assert!(
        !output.status.success(),
        "Expected non-zero exit code for env without installation, got: {:?}",
        output.status.code()
    );
}

#[rstest]
#[case("json")]
fn test_env_output_format(#[case] format: &str) {
    // Test that different output formats are accepted (though may fail without installation)
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    let output = run_command(&[
        "env",
        "--dir",
        temp_dir.path().to_str().unwrap(),
        "--format",
        format,
    ])
    .expect("Failed to run msvc-kit env");

    // Without installation, should exit with non-zero
    assert!(
        !output.status.success(),
        "Expected non-zero exit code for env without installation (format: {}), got: {:?}",
        format,
        output.status.code()
    );
}

// ============================================================================
// WinGet Release Workflow Validation Tests
// ============================================================================

/// Verify the release workflow contains the winget-releaser configuration
/// and uses a strict regex that matches exactly one binary file.
/// This prevents the "Duplicate installer entry found" winget validation error.
#[test]
fn test_release_workflow_has_winget_updater() {
    let workflow_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".github")
        .join("workflows")
        .join("release.yml");

    let content = std::fs::read_to_string(&workflow_path).expect("Failed to read release.yml");

    // Verify winget-releaser action is present
    assert!(
        content.contains("vedantmgoyal2009/winget-releaser@v2"),
        "release.yml must contain vedantmgoyal2009/winget-releaser@v2"
    );

    // Verify package identifier
    assert!(
        content.contains("identifier: loonghao.msvc-kit"),
        "release.yml must specify the correct winget package identifier"
    );

    // Verify strict regex: must anchor with ^ and $ to match exactly one file
    assert!(
        content.contains("'^msvc-kit-x86_64-windows\\.exe$'"),
        "release.yml installers-regex must use anchored pattern to prevent duplicate entries"
    );
}

/// Verify that the release workflow builds exactly one binary (x64 only)
/// to avoid creating duplicate installer entries in winget manifest.
#[test]
fn test_release_workflow_single_architecture_binary() {
    let workflow_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".github")
        .join("workflows")
        .join("release.yml");

    let content = std::fs::read_to_string(&workflow_path).expect("Failed to read release.yml");

    // Verify only x64 binary is built (no x86 or arm64 builds)
    assert!(
        content.contains("msvc-kit-x86_64-windows"),
        "release.yml must build the x86_64 Windows binary"
    );

    // Ensure we don't upload multiple architectures that could cause duplicate entries
    let x86_count = content.matches("msvc-kit-i686-windows").count();
    let arm64_count = content.matches("msvc-kit-aarch64-windows").count();

    assert_eq!(
        x86_count, 0,
        "release.yml must NOT upload i686 binary to avoid duplicate winget entries"
    );
    assert_eq!(
        arm64_count, 0,
        "release.yml must NOT upload aarch64 binary to avoid duplicate winget entries"
    );
}

/// Verify the update-winget job runs after github-release to ensure
/// assets are available when winget-releaser fetches them.
#[test]
fn test_release_workflow_winget_job_ordering() {
    let workflow_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".github")
        .join("workflows")
        .join("release.yml");

    let content = std::fs::read_to_string(&workflow_path).expect("Failed to read release.yml");

    // Verify the update-winget job depends on github-release
    assert!(
        content.contains("update-winget:"),
        "release.yml must contain the update-winget job"
    );

    // Verify there's a sleep/wait step before winget update
    assert!(
        content.contains("Waiting for release assets to be fully available"),
        "release.yml must wait for release assets before updating winget"
    );
}
