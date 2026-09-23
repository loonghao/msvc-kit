//! `setup --portable-root` anchoring tests
//!
//! Covers the two behaviours the CLI relies on:
//! - a supplied value becomes the install root of every generated script
//! - without a value the script keeps carrying the resolved install directory,
//!   and the script-relative (bundle) form is untouched

use msvc_kit::{
    generate_portable_scripts, generate_script, Architecture, ScriptContext, ShellType,
};
use std::path::PathBuf;

const MSVC_VERSION: &str = "14.44.34823";
const SDK_VERSION: &str = "10.0.26100.0";

fn anchored(root: &str) -> ScriptContext {
    ScriptContext::portable_root(
        PathBuf::from(root),
        MSVC_VERSION,
        SDK_VERSION,
        Architecture::X64,
        Architecture::X64,
    )
}

#[test]
fn portable_root_value_is_used_as_the_install_root() {
    let ctx = anchored("D:\\build\\runtime");
    let scripts = generate_portable_scripts(&ctx).unwrap();

    assert!(
        scripts
            .cmd
            .contains("set \"BUNDLE_ROOT=D:\\build\\runtime\""),
        "CMD script should anchor the root at the supplied value:\n{}",
        scripts.cmd
    );
    assert!(
        scripts
            .cmd
            .contains("%BUNDLE_ROOT%\\VC\\Tools\\MSVC\\14.44.34823"),
        "CMD script should keep using the anchored root for toolchain paths"
    );

    assert!(
        scripts
            .powershell
            .contains("$BundleRoot = \"D:\\build\\runtime\""),
        "PowerShell script should anchor the root at the supplied value:\n{}",
        scripts.powershell
    );

    assert!(
        scripts.bash.contains("BUNDLE_ROOT=\"/d/build/runtime\""),
        "bash script should anchor the root at the supplied value:\n{}",
        scripts.bash
    );
    assert!(
        !scripts.bash.contains("wslpath"),
        "bash script should no longer derive the root from the script location:\n{}",
        scripts.bash
    );
}

#[test]
fn portable_root_accepts_shell_placeholders() {
    let cmd = generate_script(&anchored("%~dp0runtime"), ShellType::Cmd).unwrap();
    assert!(cmd.contains("set \"BUNDLE_ROOT=%~dp0runtime\""));

    let powershell =
        generate_script(&anchored("$PSScriptRoot\\runtime"), ShellType::PowerShell).unwrap();
    assert!(powershell.contains("$BundleRoot = \"$PSScriptRoot\\runtime\""));

    let bash = generate_script(&anchored("$SCRIPT_DIR/runtime"), ShellType::Bash).unwrap();
    assert!(bash.contains("BUNDLE_ROOT=\"$SCRIPT_DIR/runtime\""));
    assert!(bash.contains("SCRIPT_DIR="));
}

#[test]
fn portable_root_keeps_every_toolchain_path_pointing_at_the_anchor() {
    let scripts = generate_portable_scripts(&anchored("C:\\runtime")).unwrap();

    for script in [&scripts.cmd, &scripts.powershell, &scripts.bash] {
        assert!(script.contains(MSVC_VERSION));
        assert!(script.contains(SDK_VERSION));
    }

    // No leftover reference to the script's own directory.
    let script = generate_script(&anchored("C:\\runtime"), ShellType::Cmd).unwrap();
    assert!(!script.contains("BUNDLE_ROOT=%~dp0"));
}

#[test]
fn without_a_value_the_install_directory_is_kept() {
    // The CLI default: the install directory is inlined into the script.
    let ctx = ScriptContext::absolute(
        PathBuf::from("C:\\msvc-kit"),
        MSVC_VERSION,
        SDK_VERSION,
        Architecture::X64,
        Architecture::X64,
    );
    let scripts = generate_portable_scripts(&ctx).unwrap();

    assert!(scripts.cmd.contains("C:\\msvc-kit"));
    assert!(!scripts.cmd.contains("%BUNDLE_ROOT%"));
    assert!(scripts.bash.contains("/c/msvc-kit"));
}

#[test]
fn script_relative_scripts_still_derive_their_root() {
    let ctx = ScriptContext::portable(
        MSVC_VERSION,
        SDK_VERSION,
        Architecture::X64,
        Architecture::X64,
    );
    let scripts = generate_portable_scripts(&ctx).unwrap();

    assert!(scripts.cmd.contains("set \"BUNDLE_ROOT=%~dp0\""));
    assert!(scripts.powershell.contains("$BundleRoot = $PSScriptRoot"));
    assert!(scripts.bash.contains("wslpath"));
}
