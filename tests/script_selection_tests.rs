use msvc_kit::{generate_script, Architecture, ScriptContext, ShellType};

#[test]
fn activation_scripts_run_sdk_tools_on_host_and_select_target_libraries() {
    let context = ScriptContext::absolute(
        "C:/selected".into(),
        "14.44.35207",
        "10.0.26100.0",
        Architecture::Arm64,
        Architecture::X64,
    );
    for shell in [ShellType::Cmd, ShellType::PowerShell, ShellType::Bash] {
        let script = generate_script(&context, shell).unwrap().replace('\\', "/");
        assert!(script.contains("bin/Hostx64/arm64"));
        assert!(script.contains("10/bin/10.0.26100.0/x64"));
        assert!(!script.contains("10/bin/10.0.26100.0/arm64"));
        assert!(script.contains("Lib/10.0.26100.0/um/arm64"));
        assert!(script.contains("VSCMD_ARG_HOST_ARCH") && script.contains("x64"));
    }
}

#[test]
fn activation_without_sdk_does_not_create_sdk_paths() {
    let context = ScriptContext::absolute(
        "C:/selected".into(),
        "14.44.35207",
        "",
        Architecture::X64,
        Architecture::X64,
    );
    for shell in [ShellType::Cmd, ShellType::PowerShell, ShellType::Bash] {
        let script = generate_script(&context, shell).unwrap();
        assert!(!script.contains("Windows Kits"));
        assert!(!script.contains("WindowsSdkDir="));
    }
}
