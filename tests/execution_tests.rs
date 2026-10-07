//! Child isolation and generated toolchain paths are public execution contracts.
use msvc_kit::execution::{configure_command, run_in_toolchain, write_cmake_toolchain};
use msvc_kit::{Architecture, MsvcEnvironment};
use std::path::Path;
use std::process::Command;

fn environment(root: &Path) -> MsvcEnvironment {
    MsvcEnvironment {
        vc_install_dir: root.join("VC"),
        vc_tools_install_dir: root.join("VC/Tools/MSVC/14.44.35207"),
        vc_tools_version: "14.44.35207".into(),
        windows_sdk_dir: root.join("Windows Kits/10"),
        windows_sdk_version: "10.0.26100.0".into(),
        include_paths: vec![root.join("include")],
        lib_paths: vec![root.join("lib")],
        bin_paths: vec![root.join("tools ]=] and ]]")],
        arch: Architecture::Arm64,
        host_arch: Architecture::X64,
    }
}

#[test]
fn child_environment_prepends_selected_tools_and_leaves_parent_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let env = environment(root.path());
    let inherited = std::env::var_os("PATH");
    let mut command = Command::new("unused");
    configure_command(&mut command, &env).unwrap();
    let vars: std::collections::HashMap<_, _> = command.get_envs().collect();
    let child_path = vars[std::ffi::OsStr::new("PATH")].unwrap();
    let paths: Vec<_> = std::env::split_paths(child_path).collect();
    assert_eq!(paths[0], env.bin_paths[0]);
    if let Some(path) = &inherited {
        assert_eq!(paths[1..], std::env::split_paths(path).collect::<Vec<_>>());
    }
    assert_eq!(
        vars[std::ffi::OsStr::new("VCToolsVersion")].unwrap(),
        "14.44.35207"
    );
    assert_eq!(std::env::var_os("PATH"), inherited);
}

#[test]
fn cmake_file_quotes_paths_and_failed_generation_preserves_previous_file() {
    let root = tempfile::tempdir().unwrap();
    let env = environment(root.path());
    std::fs::create_dir_all(&env.bin_paths[0]).unwrap();
    for name in ["cl", "link", "lib", "rc", "mt"] {
        std::fs::write(env.bin_paths[0].join(format!("{name}.exe")), b"fixture").unwrap();
    }
    let output = root.path().join("generated/toolchain.cmake");
    write_cmake_toolchain(&env, &output).unwrap();
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("set(CMAKE_SYSTEM_PROCESSOR arm64)"));
    assert!(content.contains("set(CMAKE_RC_COMPILER [==["));
    assert!(content.contains("set(CMAKE_MT [==["));
    assert!(!content.contains('\\'));
    for name in ["cl", "link", "lib"] {
        let tool = env.bin_paths[0].join(format!("{name}.exe"));
        std::fs::remove_file(&tool).unwrap();
        assert!(write_cmake_toolchain(&env, &output).is_err());
        assert_eq!(std::fs::read_to_string(&output).unwrap(), content);
        std::fs::write(tool, b"fixture").unwrap();
    }
    for name in ["rc", "mt"] {
        std::fs::remove_file(env.bin_paths[0].join(format!("{name}.exe"))).unwrap();
    }
    write_cmake_toolchain(&env, &output).unwrap();
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(!content.contains("CMAKE_RC_COMPILER"));
    assert!(!content.contains("CMAKE_MT"));
}

#[test]
fn child_exit_and_spawn_failure_are_returned_to_the_caller() {
    let root = tempfile::tempdir().unwrap();
    let env = environment(root.path());
    let (program, arguments) = if cfg!(windows) {
        ("cmd", vec!["/D", "/C", "exit 23"])
    } else {
        ("sh", vec!["-c", "exit 23"])
    };
    let arguments: Vec<_> = arguments
        .into_iter()
        .map(std::ffi::OsString::from)
        .collect();
    assert_eq!(
        run_in_toolchain(&env, program.as_ref(), &arguments)
            .unwrap()
            .code(),
        Some(23)
    );
    assert!(run_in_toolchain(&env, root.path().join("absent-tool").as_os_str(), &[]).is_err());
}
