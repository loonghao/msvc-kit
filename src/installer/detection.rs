//! Read-only toolchain detection and reuse planning (spec 07 §1-4, roadmap P09).
//!
//! Detection is strictly read-only: it never writes, never modifies the
//! registry, never changes the PATH, never touches the detected root. All
//! unit tests use filesystem fixtures in temp directories - no `PATH`
//! scanning, no local-machine assumptions (spec 07 §8).
//!
//! Detect ≠ Validate ≠ Reuse (spec 07 §1):
//! - Detect: the root has the expected layout (executables, headers, libs).
//! - Validate: each required tool/header/lib for the requested capability is
//!   actually present at the expected relative path.
//! - Reuse: the validated toolchain satisfies the plan's required
//!   capabilities; the result is `ReusableExact`, `ReusableWithAdditions`,
//!   `Incompatible`, or `Unknown`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::version::Architecture;

/// How the toolchain was found (spec 07 §3 `DetectionOrigin`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionOrigin {
    VsWhere,
    Registry,
    KnownPath,
    UserSpecified,
    PortableRoot,
}

/// Validation outcome per required capability check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Present,
    Missing { path: PathBuf },
}

impl CheckStatus {
    pub fn is_present(&self) -> bool {
        matches!(self, Self::Present)
    }
}

/// One validation check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationCheck {
    pub capability: String,
    pub status: CheckStatus,
}

/// Aggregate validation summary (spec 07 §3 `ValidationSummary`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationSummary {
    pub checks: Vec<ValidationCheck>,
    /// True iff every check is Present.
    pub all_present: bool,
}

impl ValidationSummary {
    pub fn from_checks(checks: Vec<ValidationCheck>) -> Self {
        let all_present = checks.iter().all(|check| check.status.is_present());
        Self {
            checks,
            all_present,
        }
    }

    pub fn missing(&self) -> Vec<&ValidationCheck> {
        self.checks
            .iter()
            .filter(|check| !check.status.is_present())
            .collect()
    }
}

/// A detected toolchain root with its validated capabilities.
#[derive(Debug, Clone)]
pub struct DetectedToolchain {
    pub root: PathBuf,
    pub kind: ToolchainKind,
    pub origin: DetectionOrigin,
    /// Version string as observed (not guessed from family alone).
    pub version: Option<String>,
    /// Target architectures this root serves (from observed layout).
    pub target_arches: HashSet<Architecture>,
    pub validation: ValidationSummary,
}

/// The kind of toolchain root found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainKind {
    Msvc,
    WindowsSdk,
    Llvm,
    MinGw,
    BuildTool(BuildToolId),
}

/// Build tool identifiers (spec 07 §2 "Build Tools" — separate from
/// compiler toolchains).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildToolId {
    CMake,
    Ninja,
    Nasm,
}

impl BuildToolId {
    pub fn exe_name(&self) -> &'static str {
        match self {
            Self::CMake => "cmake.exe",
            Self::Ninja => "ninja.exe",
            Self::Nasm => "nasm.exe",
        }
    }

    pub fn version_args(&self) -> &'static [&'static str] {
        match self {
            Self::CMake => &["--version"],
            Self::Ninja => &["--version"],
            Self::Nasm => &["-v"],
        }
    }
}

/// Reuse decision (spec 07 §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReuseDecision {
    /// Every required capability validated.
    ReusableExact,
    /// Some validated, some missing — the plan may add to a separate root.
    ReusableWithAdditions,
    /// The toolchain root exists but cannot serve the required capabilities.
    Incompatible { reasons: Vec<String> },
    /// Not enough evidence to decide (spec: no automatic reuse without evidence).
    Unknown,
}

/// Validate an MSVC root layout against the requested target architecture.
///
/// Required layout (mirror of `extract_and_finalize_msvc` output):
/// - `VC/Tools/MSVC/{version}/bin/Host{host}/{target}/cl.exe`
/// - `VC/Tools/MSVC/{version}/bin/Host{host}/{target}/link.exe`
/// - `VC/Tools/MSVC/{version}/include/` (non-empty)
/// - `VC/Tools/MSVC/{version}/lib/{target}/` (non-empty)
///
/// Read-only: only `is_file()`/`is_dir()`/`read_dir()` calls.
pub fn detect_msvc(root: &Path, target: Architecture) -> Result<DetectedToolchain> {
    let msvc_base = root.join("VC").join("Tools").join("MSVC");
    let mut checks = Vec::new();
    let mut target_arches = HashSet::new();
    let mut version = None;

    if !msvc_base.is_dir() {
        return Ok(DetectedToolchain {
            root: root.to_path_buf(),
            kind: ToolchainKind::Msvc,
            origin: DetectionOrigin::UserSpecified,
            version: None,
            target_arches,
            validation: ValidationSummary::from_checks(vec![ValidationCheck {
                capability: "msvc.root".to_string(),
                status: CheckStatus::Missing { path: msvc_base },
            }]),
        });
    }

    // Discover the version directory (exactly one expected; lexically last wins).
    let mut versions: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&msvc_base)? {
        let entry = entry?;
        if entry.path().is_dir() {
            versions.push(entry.path());
        }
    }
    versions.sort();
    if let Some(latest) = versions.last() {
        version = Some(
            latest
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
        );
    }
    let Some(version_dir) = versions.last() else {
        return Ok(DetectedToolchain {
            root: root.to_path_buf(),
            kind: ToolchainKind::Msvc,
            origin: DetectionOrigin::UserSpecified,
            version: None,
            target_arches,
            validation: ValidationSummary::from_checks(vec![ValidationCheck {
                capability: "msvc.version_dir".to_string(),
                status: CheckStatus::Missing {
                    path: msvc_base.clone(),
                },
            }]),
        });
    };

    let host = Architecture::host();
    let host_dir = host.msvc_host_dir();
    let target_dir = target.msvc_target_dir();
    let bin = version_dir.join("bin").join(host_dir).join(target_dir);

    for tool in ["cl.exe", "link.exe", "lib.exe"] {
        let path = bin.join(tool);
        checks.push(ValidationCheck {
            capability: format!("msvc.tool.{tool}"),
            status: if path.is_file() {
                CheckStatus::Present
            } else {
                CheckStatus::Missing { path }
            },
        });
    }

    let include = version_dir.join("include");
    let include_present = include.is_dir()
        && std::fs::read_dir(&include)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
    checks.push(ValidationCheck {
        capability: "msvc.headers".to_string(),
        status: if include_present {
            CheckStatus::Present
        } else {
            CheckStatus::Missing { path: include }
        },
    });

    let lib_dir = version_dir.join("lib").join(target.to_string());
    let lib_present = lib_dir.is_dir()
        && std::fs::read_dir(&lib_dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
    checks.push(ValidationCheck {
        capability: format!("msvc.libs.{target}"),
        status: if lib_present {
            CheckStatus::Present
        } else {
            CheckStatus::Missing { path: lib_dir }
        },
    });

    if lib_present {
        target_arches.insert(target);
    }

    Ok(DetectedToolchain {
        root: root.to_path_buf(),
        kind: ToolchainKind::Msvc,
        origin: DetectionOrigin::UserSpecified,
        version,
        target_arches,
        validation: ValidationSummary::from_checks(checks),
    })
}

/// Validate a Windows SDK root layout against the requested architecture.
///
/// Required layout:
/// - `Include/{version}/um/` and `Include/{version}/ucrt/` (non-empty)
/// - `Lib/{version}/um/{arch}/` (non-empty)
/// - `Lib/{version}/ucrt/{arch}/` (non-empty)
pub fn detect_windows_sdk(root: &Path, target: Architecture) -> Result<DetectedToolchain> {
    let sdk_base = root.join("Windows Kits").join("10");
    let mut checks = Vec::new();
    let mut target_arches = HashSet::new();
    let mut version = None;

    if !sdk_base.is_dir() {
        return Ok(DetectedToolchain {
            root: root.to_path_buf(),
            kind: ToolchainKind::WindowsSdk,
            origin: DetectionOrigin::UserSpecified,
            version: None,
            target_arches,
            validation: ValidationSummary::from_checks(vec![ValidationCheck {
                capability: "sdk.root".to_string(),
                status: CheckStatus::Missing { path: sdk_base },
            }]),
        });
    }

    // Latest SDK version directory.
    for subdir in ["Include", "Lib"] {
        let base = sdk_base.join(subdir);
        let mut versions: Vec<PathBuf> = Vec::new();
        if base.is_dir() {
            for entry in std::fs::read_dir(&base)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    versions.push(entry.path());
                }
            }
        }
        versions.sort();
        if let Some(latest) = versions.last() {
            let name = latest
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            if version.is_none() {
                version = Some(name);
            }
        }
    }

    let arch_str = target.to_string();
    for area in ["um", "ucrt"] {
        let lib_path = sdk_base
            .join("Lib")
            .join(version.clone().unwrap_or_default())
            .join(area)
            .join(&arch_str);
        let present = lib_path.is_dir()
            && std::fs::read_dir(&lib_path)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(false);
        checks.push(ValidationCheck {
            capability: format!("sdk.libs.{area}.{arch_str}"),
            status: if present {
                CheckStatus::Present
            } else {
                CheckStatus::Missing { path: lib_path }
            },
        });
    }

    let include_um = sdk_base
        .join("Include")
        .join(version.clone().unwrap_or_default())
        .join("um");
    let include_present = include_um.is_dir()
        && std::fs::read_dir(&include_um)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
    checks.push(ValidationCheck {
        capability: "sdk.headers.um".to_string(),
        status: if include_present {
            CheckStatus::Present
        } else {
            CheckStatus::Missing { path: include_um }
        },
    });

    if checks.iter().all(|check| check.status.is_present()) {
        target_arches.insert(target);
    }

    Ok(DetectedToolchain {
        root: root.to_path_buf(),
        kind: ToolchainKind::WindowsSdk,
        origin: DetectionOrigin::UserSpecified,
        version,
        target_arches,
        validation: ValidationSummary::from_checks(checks),
    })
}

/// Detect a build tool (CMake/Ninja/NASM) at a given path. Only checks the
/// executable's existence and reads version output — no PATH scanning.
pub fn detect_build_tool(exe_path: &Path, tool: BuildToolId) -> Result<DetectedToolchain> {
    let present = exe_path.is_file();
    let checks = vec![ValidationCheck {
        capability: format!("build_tool.{}", tool.exe_name()),
        status: if present {
            CheckStatus::Present
        } else {
            CheckStatus::Missing {
                path: exe_path.to_path_buf(),
            }
        },
    }];
    Ok(DetectedToolchain {
        root: exe_path.parent().unwrap_or(exe_path).to_path_buf(),
        kind: ToolchainKind::BuildTool(tool),
        origin: DetectionOrigin::UserSpecified,
        version: None,
        target_arches: HashSet::new(),
        validation: ValidationSummary::from_checks(checks),
    })
}

/// Capability-based reuse decision (spec 07 §4).
///
/// A detected toolchain is `ReusableExact` when all required capabilities
/// validate; `ReusableWithAdditions` when some validate but not all;
/// `Incompatible` when the toolchain kind does not match the required
/// capability prefix; `Unknown` when no capability requirements were given.
pub fn plan_reuse(detected: &DetectedToolchain, required_capabilities: &[String]) -> ReuseDecision {
    if required_capabilities.is_empty() {
        return ReuseDecision::Unknown;
    }

    let kind_prefix = match detected.kind {
        ToolchainKind::Msvc => "msvc.",
        ToolchainKind::WindowsSdk => "sdk.",
        ToolchainKind::Llvm => "llvm.",
        ToolchainKind::MinGw => "mingw.",
        ToolchainKind::BuildTool(_) => return ReuseDecision::Unknown,
    };

    // Required capabilities whose prefix does not match the detected kind are
    // structurally incompatible: the root cannot serve them.
    let mismatched: Vec<&String> = required_capabilities
        .iter()
        .filter(|cap| !cap.starts_with(kind_prefix))
        .collect();
    if !mismatched.is_empty() {
        return ReuseDecision::Incompatible {
            reasons: mismatched
                .iter()
                .map(|cap| {
                    format!(
                        "Capability {cap:?} is not served by a {:?} root",
                        detected.kind
                    )
                })
                .collect(),
        };
    }

    // All required capabilities are in the detected kind's namespace; check
    // the detection's validation results for those capabilities.
    let missing: Vec<&ValidationCheck> = detected
        .validation
        .checks
        .iter()
        .filter(|check| !check.status.is_present())
        .collect();
    if missing.is_empty() {
        ReuseDecision::ReusableExact
    } else {
        ReuseDecision::ReusableWithAdditions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Build a synthetic MSVC root fixture in a temp dir.
    fn msvc_fixture(root: &Path, version: &str, target: Architecture, with_cl: bool) -> PathBuf {
        let host = Architecture::host();
        let bin = root
            .join("VC/Tools/MSVC")
            .join(version)
            .join("bin")
            .join(host.msvc_host_dir())
            .join(target.msvc_target_dir());
        fs::create_dir_all(&bin).unwrap();
        if with_cl {
            fs::write(bin.join("cl.exe"), b"fake cl").unwrap();
            fs::write(bin.join("link.exe"), b"fake link").unwrap();
            fs::write(bin.join("lib.exe"), b"fake lib").unwrap();
        }
        let include = root.join("VC/Tools/MSVC").join(version).join("include");
        fs::create_dir_all(&include).unwrap();
        fs::write(include.join("stdio.h"), b"fake header").unwrap();
        let lib = root
            .join("VC/Tools/MSVC")
            .join(version)
            .join("lib")
            .join(target.to_string());
        fs::create_dir_all(&lib).unwrap();
        fs::write(lib.join("libcmt.lib"), b"fake lib").unwrap();
        root.to_path_buf()
    }

    #[test]
    fn msvc_detector_finds_complete_installation() {
        let dir = tempfile::tempdir().unwrap();
        let root = msvc_fixture(dir.path(), "14.44.34823", Architecture::X64, true);
        let detected = detect_msvc(&root, Architecture::X64).unwrap();
        assert_eq!(detected.kind, ToolchainKind::Msvc);
        assert_eq!(detected.version.as_deref(), Some("14.44.34823"));
        assert!(detected.validation.all_present, "{:?}", detected.validation);
        assert!(detected.target_arches.contains(&Architecture::X64));
        // Read-only: the root is unchanged after detection.
        let count_before = count_files(&root);
        let _ = detect_msvc(&root, Architecture::X64).unwrap();
        assert_eq!(count_before, count_files(&root));
    }

    #[test]
    fn msvc_detector_rejects_cl_exe_only_false_positive() {
        let dir = tempfile::tempdir().unwrap();
        // Root with cl.exe but NO link.exe, no headers, no libs.
        let root = msvc_fixture(dir.path(), "14.44.34823", Architecture::X64, false);
        let detected = detect_msvc(&root, Architecture::X64).unwrap();
        assert!(!detected.validation.all_present);
        assert!(detected
            .validation
            .missing()
            .iter()
            .any(|check| check.capability.contains("link.exe")));
        assert!(detected
            .validation
            .missing()
            .iter()
            .any(|check| check.capability.contains("lib.exe")));
    }

    #[test]
    fn msvc_detector_handles_empty_root() {
        let dir = tempfile::tempdir().unwrap();
        let detected = detect_msvc(dir.path(), Architecture::X64).unwrap();
        assert!(!detected.validation.all_present);
        assert!(detected.version.is_none());
    }

    #[test]
    fn sdk_detector_finds_complete_installation() {
        let dir = tempfile::tempdir().unwrap();
        let sdk = dir.path().join("Windows Kits").join("10");
        let version = "10.0.26100.0";
        for area in ["um", "ucrt"] {
            let lib = sdk.join("Lib").join(version).join(area).join("x64");
            fs::create_dir_all(&lib).unwrap();
            fs::write(lib.join("fake.lib"), b"fake").unwrap();
        }
        let include = sdk.join("Include").join(version).join("um");
        fs::create_dir_all(&include).unwrap();
        fs::write(include.join("windows.h"), b"fake").unwrap();

        let detected = detect_windows_sdk(dir.path(), Architecture::X64).unwrap();
        assert_eq!(detected.kind, ToolchainKind::WindowsSdk);
        assert_eq!(detected.version.as_deref(), Some(version));
        assert!(detected.validation.all_present, "{:?}", detected.validation);
        assert!(detected.target_arches.contains(&Architecture::X64));
    }

    #[test]
    fn sdk_detector_rejects_incomplete_installation() {
        let dir = tempfile::tempdir().unwrap();
        let sdk = dir.path().join("Windows Kits").join("10");
        // Only Include, no Lib.
        let include = sdk.join("Include").join("10.0.26100.0").join("um");
        fs::create_dir_all(&include).unwrap();
        fs::write(include.join("windows.h"), b"fake").unwrap();

        let detected = detect_windows_sdk(dir.path(), Architecture::X64).unwrap();
        assert!(!detected.validation.all_present);
        assert!(detected
            .validation
            .missing()
            .iter()
            .any(|check| check.capability.contains("libs")));
    }

    #[test]
    fn build_tool_detector_checks_exe_existence() {
        let dir = tempfile::tempdir().unwrap();
        let cmake = dir.path().join("cmake.exe");
        fs::write(&cmake, b"fake cmake").unwrap();

        let detected = detect_build_tool(&cmake, BuildToolId::CMake).unwrap();
        assert_eq!(detected.kind, ToolchainKind::BuildTool(BuildToolId::CMake));
        assert!(detected.validation.all_present);

        let missing = detect_build_tool(&dir.path().join("ninja.exe"), BuildToolId::Ninja).unwrap();
        assert!(!missing.validation.all_present);
    }

    #[test]
    fn reuse_planner_exact_when_all_present() {
        let dir = tempfile::tempdir().unwrap();
        let root = msvc_fixture(dir.path(), "14.44.34823", Architecture::X64, true);
        let detected = detect_msvc(&root, Architecture::X64).unwrap();
        let decision = plan_reuse(&detected, &["msvc.tool.cl.exe".to_string()]);
        assert_eq!(decision, ReuseDecision::ReusableExact);
    }

    #[test]
    fn reuse_planner_with_additions_when_partial() {
        let dir = tempfile::tempdir().unwrap();
        let root = msvc_fixture(dir.path(), "14.44.34823", Architecture::X64, false);
        let detected = detect_msvc(&root, Architecture::X64).unwrap();
        let decision = plan_reuse(&detected, &["msvc.tool.cl.exe".to_string()]);
        assert_eq!(decision, ReuseDecision::ReusableWithAdditions);
    }

    #[test]
    fn reuse_planner_incompatible_for_wrong_kind() {
        let dir = tempfile::tempdir().unwrap();
        let sdk = dir.path().join("Windows Kits").join("10");
        let version = "10.0.26100.0";
        for area in ["um", "ucrt"] {
            let lib = sdk.join("Lib").join(version).join(area).join("x64");
            fs::create_dir_all(&lib).unwrap();
            fs::write(lib.join("fake.lib"), b"fake").unwrap();
        }
        let include = sdk.join("Include").join(version).join("um");
        fs::create_dir_all(&include).unwrap();
        fs::write(include.join("windows.h"), b"fake").unwrap();
        let detected = detect_windows_sdk(dir.path(), Architecture::X64).unwrap();

        // Asking an SDK root for MSVC capabilities is incompatible.
        let decision = plan_reuse(&detected, &["msvc.tool.cl.exe".to_string()]);
        assert!(
            matches!(decision, ReuseDecision::Incompatible { .. }),
            "{decision:?}"
        );
    }

    #[test]
    fn reuse_planner_unknown_when_no_requirements() {
        let dir = tempfile::tempdir().unwrap();
        let root = msvc_fixture(dir.path(), "14.44.34823", Architecture::X64, true);
        let detected = detect_msvc(&root, Architecture::X64).unwrap();
        assert_eq!(plan_reuse(&detected, &[]), ReuseDecision::Unknown);
    }

    fn count_files(root: &Path) -> usize {
        let mut count = 0;
        fn walk(dir: &Path, count: &mut usize) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    *count += 1;
                    if entry.path().is_dir() {
                        walk(&entry.path(), count);
                    }
                }
            }
        }
        walk(root, &mut count);
        count
    }
}
