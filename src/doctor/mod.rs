//! Read-only diagnostics and an explicit isolated Windows build probe.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::env::MsvcEnvironment;
use crate::execution::configure_command;
use crate::query::{query_installation, QueryOptions, QueryResult};
use crate::toolchain::{resolve_toolchain, ToolchainRequest};
use crate::version::Architecture;

/// Diagnostic outcome. Skips never hide a failed prerequisite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorStatus {
    /// Check or report passed.
    Passed,
    /// Check or report failed.
    Failed,
    /// Explicitly unexecuted optional stage.
    Skipped,
}

/// One diagnostic with a stable machine-readable ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    /// Stable check identifier.
    pub id: String,
    /// Outcome.
    pub status: DoctorStatus,
    /// Human-readable evidence or reason.
    pub message: String,
    /// Checked file when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// Diagnostic request. Compilation is opt-in and isolated in a temporary folder.
#[derive(Debug, Clone, Default)]
pub struct DoctorOptions {
    /// Installed toolchain selection.
    pub toolchain: ToolchainRequest,
    /// Compile C++, resource, link, embed a manifest and execute native outputs.
    pub compile_probe: bool,
}

/// Stable JSON report, also returned for selection or filesystem failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Report contract version.
    pub schema: String,
    /// Passed only if all required checks passed.
    pub status: DoctorStatus,
    /// Diagnostics in deterministic order.
    pub checks: Vec<DoctorCheck>,
    /// Selected versions, environment, fingerprint and tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<QueryResult>,
}

impl DoctorReport {
    /// Whether all required checks passed.
    pub fn is_success(&self) -> bool {
        self.status == DoctorStatus::Passed
    }

    /// Render checks for interactive CLI use.
    pub fn format_summary(&self) -> String {
        self.checks
            .iter()
            .map(|check| format!("{:?} {}: {}\n", check.status, check.id, check.message))
            .collect()
    }

    fn push(
        &mut self,
        id: &str,
        status: DoctorStatus,
        message: impl Into<String>,
        path: Option<PathBuf>,
    ) {
        if status == DoctorStatus::Failed {
            self.status = DoctorStatus::Failed;
        }
        self.checks.push(DoctorCheck {
            id: id.into(),
            status,
            message: message.into(),
            path,
        });
    }

    fn check_file(&mut self, id: &str, path: PathBuf) {
        let passed = path.is_file();
        self.push(
            id,
            if passed {
                DoctorStatus::Passed
            } else {
                DoctorStatus::Failed
            },
            if passed {
                "Required file is present"
            } else {
                "Required file is missing"
            },
            Some(path),
        );
    }
}

/// Inspect an installation. Default mode never launches tools or changes it.
pub fn doctor(options: &DoctorOptions) -> DoctorReport {
    let mut report = DoctorReport {
        schema: "msvc-kit.doctor.v1".into(),
        status: DoctorStatus::Passed,
        checks: vec![],
        toolchain: None,
    };
    let env = match resolve_toolchain(&options.toolchain) {
        Ok(env) => env,
        Err(error) => {
            report.push("resolution", DoctorStatus::Failed, error.to_string(), None);
            return report;
        }
    };
    let query_options = QueryOptions {
        install_dir: options.toolchain.install_dir.clone(),
        arch: env.arch,
        host_arch: env.host_arch,
        msvc_version: Some(env.vc_tools_version.clone()),
        sdk_version: (!env.windows_sdk_version.is_empty()).then(|| env.windows_sdk_version.clone()),
        ..QueryOptions::default()
    };
    match query_installation(&query_options) {
        Ok(query) => report.toolchain = Some(query),
        Err(error) => report.push("resolution", DoctorStatus::Failed, error.to_string(), None),
    }
    for (name, path) in [
        ("cl", env.cl_exe_path()),
        ("link", env.link_exe_path()),
        ("lib", env.lib_exe_path()),
        ("rc", env.rc_exe_path()),
        ("mt", env.mt_exe_path()),
    ] {
        report.push(
            &format!("tool.{}", name),
            if path.is_some() {
                DoctorStatus::Passed
            } else {
                DoctorStatus::Failed
            },
            if path.is_some() {
                "Required executable is present"
            } else {
                "Required executable is missing for the selected host/target"
            },
            path,
        );
    }
    let vc = &env.vc_tools_install_dir;
    report.check_file("header.msvc", vc.join("include").join("vcruntime.h"));
    report.check_file("header.cpp", vc.join("include").join("vector"));
    let vc_lib = vc.join("lib").join(env.arch.to_string());
    for (id, name) in [
        ("runtime", "msvcrt.lib"),
        ("vcruntime", "vcruntime.lib"),
        ("cpp", "msvcprt.lib"),
    ] {
        report.check_file(&format!("lib.{}", id), vc_lib.join(name));
    }
    if env.windows_sdk_version.is_empty() {
        report.push(
            "sdk",
            DoctorStatus::Failed,
            "Windows SDK is not installed; a complete Windows build requires an SDK",
            None,
        );
    } else {
        let sdk_include = env
            .windows_sdk_dir
            .join("Include")
            .join(&env.windows_sdk_version);
        for (id, directory, name) in [
            ("ucrt", "ucrt", "stdio.h"),
            ("windows", "um", "Windows.h"),
            ("shared", "shared", "sdkddkver.h"),
        ] {
            report.check_file(
                &format!("header.{}", id),
                sdk_include.join(directory).join(name),
            );
        }
        let sdk_lib = env
            .windows_sdk_dir
            .join("Lib")
            .join(&env.windows_sdk_version);
        report.check_file(
            "lib.ucrt",
            sdk_lib
                .join("ucrt")
                .join(env.arch.to_string())
                .join("ucrt.lib"),
        );
        report.check_file(
            "lib.windows",
            sdk_lib
                .join("um")
                .join(env.arch.to_string())
                .join("kernel32.lib"),
        );
    }
    if options.compile_probe {
        if !report.is_success() {
            report.push(
                "probe.compile",
                DoctorStatus::Skipped,
                "Required files failed diagnostics; build probe was not started",
                None,
            );
        } else if !cfg!(windows) {
            report.push(
                "probe.compile",
                DoctorStatus::Failed,
                "Executing the Windows build probe requires a Windows host",
                None,
            );
        } else {
            run_build_probe(&env, &mut report);
        }
    }
    report
}

fn run_build_probe(env: &MsvcEnvironment, report: &mut DoctorReport) {
    let workspace = match tempfile::tempdir() {
        Ok(workspace) => workspace,
        Err(error) => {
            report.push(
                "probe.compile",
                DoctorStatus::Failed,
                error.to_string(),
                None,
            );
            return;
        }
    };
    let root = workspace.path();
    let source = root.join("probe.cpp");
    let resource = root.join("probe.rc");
    let manifest = root.join("probe.manifest");
    let files = [
        (&source, "#include <Windows.h>\n#include <vector>\nint main() { std::vector<DWORD> ids{GetCurrentProcessId()}; return ids[0] == 0; }\n"),
        (&resource, "1 RCDATA { 1, 2, 3, 4 }\n"),
        (&manifest, "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><assembly xmlns=\"urn:schemas-microsoft-com:asm.v1\" manifestVersion=\"1.0\"><assemblyIdentity version=\"1.0.0.0\" processorArchitecture=\"*\" name=\"msvc-kit.probe\" type=\"win32\"/></assembly>"),
    ];
    for (path, contents) in files {
        if let Err(error) = std::fs::write(path, contents) {
            report.push(
                "probe.compile",
                DoctorStatus::Failed,
                error.to_string(),
                Some(path.clone()),
            );
            return;
        }
    }
    let mut compiler = Command::new(env.cl_exe_path().expect("compiler was checked"));
    compiler.args(["/nologo", "/EHsc", "/MD", "/c", "probe.cpp", "/Foprobe.obj"]);
    if !run_stage("probe.compile", compiler, env, root, report) {
        return;
    }
    let mut rc = Command::new(env.rc_exe_path().expect("resource compiler was checked"));
    rc.args(["/nologo", "/foprobe.res", "probe.rc"]);
    if !run_stage("probe.resource", rc, env, root, report) {
        return;
    }
    let mut linker = Command::new(env.link_exe_path().expect("linker was checked"));
    let machine = match env.arch {
        Architecture::X64 => "X64",
        Architecture::X86 => "X86",
        Architecture::Arm64 => "ARM64",
        Architecture::Arm => "ARM",
    };
    linker
        .args([
            "/nologo",
            "/OUT:probe.exe",
            "/SUBSYSTEM:CONSOLE",
            "/MANIFEST:NO",
            "probe.obj",
            "probe.res",
            "kernel32.lib",
        ])
        .arg(format!("/MACHINE:{}", machine));
    if !run_stage("probe.link", linker, env, root, report) {
        return;
    }
    let mut mt = Command::new(env.mt_exe_path().expect("manifest tool was checked"));
    mt.args([
        "/nologo",
        "/manifest",
        "probe.manifest",
        "/outputresource:probe.exe;#1",
    ]);
    if !run_stage("probe.manifest", mt, env, root, report) {
        return;
    }
    if env.host_arch == env.arch && env.host_arch == Architecture::host() {
        run_stage(
            "probe.execute",
            Command::new(root.join("probe.exe")),
            env,
            root,
            report,
        );
    } else {
        report.push(
            "probe.execute",
            DoctorStatus::Skipped,
            "Cross-compiled output was not executed on this host",
            None,
        );
    }
}

fn run_stage(
    id: &str,
    mut command: Command,
    env: &MsvcEnvironment,
    root: &Path,
    report: &mut DoctorReport,
) -> bool {
    let output_path = root.join(format!("{}.log", id));
    let result = (|| -> crate::Result<String> {
        configure_command(&mut command, env)?;
        let stdout = std::fs::File::create(&output_path)?;
        command
            .current_dir(root)
            .stdin(Stdio::null())
            .stderr(stdout.try_clone()?)
            .stdout(stdout);
        let mut child = command.spawn()?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                let output_bytes = std::fs::read(&output_path).unwrap_or_default();
                let output = String::from_utf8_lossy(&output_bytes);
                if status.success() {
                    return Ok("Tool completed successfully".into());
                }
                return Err(crate::MsvcKitError::Other(format!(
                    "Tool exited with {}: {}",
                    status,
                    output.trim()
                )));
            }
            if started.elapsed() > Duration::from_secs(30) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::MsvcKitError::Other(
                    "Tool exceeded the 30-second diagnostic timeout".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    match result {
        Ok(message) => {
            report.push(id, DoctorStatus::Passed, message, None);
            true
        }
        Err(error) => {
            report.push(id, DoctorStatus::Failed, error.to_string(), None);
            false
        }
    }
}
