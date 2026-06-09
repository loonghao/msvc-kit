//! Install a downloaded MSVC toolchain into Visual Studio so UBT can discover it.
//!
//! UBT (Unreal Build Tool) discovers MSVC toolchains by scanning the VS BuildTools
//! installation directory at `%ProgramFiles%/.../VC/Tools/MSVC/`. When msvc-kit
//! downloads a toolchain to a custom directory (e.g. `C:\msvc-kit\14.36`), UBT
//! cannot find it. This module copies the toolchain files into the VS directory
//! tree, making them visible to UBT.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A detected Visual Studio instance suitable for toolchain installation.
#[derive(Debug, Clone)]
pub struct VsInstance {
    /// Root installation path (e.g. `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools`)
    pub install_path: PathBuf,
    /// Display version (e.g. "17.0")
    pub version: String,
    /// Human-readable label
    pub label: String,
}

/// Result of the install-into-vs operation.
#[derive(Debug)]
pub struct VsInstallResult {
    /// The VS instance that was used
    pub vs_instance: VsInstance,
    /// MSVC toolchain versions currently registered with this VS instance
    pub registered_versions: Vec<String>,
    /// The source directory that was installed
    pub installed_source: Option<PathBuf>,
}

/// Find all Visual Studio instances with the VC Tools workload installed.
///
/// Uses, in order:
/// 1. `vswhere.exe` (most reliable, included with VS 2017+)
/// 2. Registry queries on Windows
/// 3. Common installation paths
pub fn find_vs_instances() -> Vec<VsInstance> {
    let mut instances = Vec::new();

    // Strategy 1: vswhere.exe
    if let Ok(vs) = find_via_vswhere() {
        instances.extend(vs);
    }

    // Strategy 2: Registry (Windows only)
    #[cfg(windows)]
    if instances.is_empty() {
        if let Ok(vs) = find_via_registry() {
            instances.extend(vs);
        }
    }

    // Strategy 3: Common paths fallback
    if instances.is_empty() {
        instances.extend(find_via_common_paths());
    }

    instances
}

fn find_via_vswhere() -> Result<Vec<VsInstance>, String> {
    let vswhere_paths = [
        Path::new(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"),
        Path::new(r"C:\Program Files\Microsoft Visual Studio\Installer\vswhere.exe"),
    ];

    let vswhere = vswhere_paths.iter().find(|p| p.exists());
    let vswhere = match vswhere {
        Some(p) => p,
        None => return Err("vswhere.exe not found".into()),
    };

    let output = Command::new(vswhere)
        .args([
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-format",
            "json",
            "-utf8",
        ])
        .output()
        .map_err(|e| format!("failed to run vswhere: {}", e))?;

    if !output.status.success() {
        return Err(format!("vswhere exited with code {}", output.status));
    }

    #[derive(serde::Deserialize)]
    struct VsWhereInstance {
        #[serde(rename = "installationPath")]
        installation_path: String,
        #[serde(rename = "installationVersion")]
        installation_version: String,
        #[serde(rename = "displayName", default)]
        display_name: Option<String>,
    }

    let parsed: Vec<VsWhereInstance> =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("json parse: {}", e))?;

    Ok(parsed
        .into_iter()
        .map(|inst| {
            let label = inst
                .display_name
                .unwrap_or_else(|| format!("VS {}", &inst.installation_version[..4]));
            VsInstance {
                install_path: PathBuf::from(inst.installation_path),
                version: inst.installation_version,
                label,
            }
        })
        .collect())
}

#[cfg(windows)]
fn find_via_registry() -> Result<Vec<VsInstance>, String> {
    use std::io::ErrorKind;
    use winreg::enums::*;
    use winreg::RegKey;

    let mut instances = Vec::new();

    // VS 2017+ stores instances under HKLM\SOFTWARE\Microsoft\VisualStudio\Setup\VS
    let setup_key = match RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\VisualStudio\Setup\VS", KEY_READ)
        .or_else(|_| {
            RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(
                r"SOFTWARE\WOW6432Node\Microsoft\VisualStudio\Setup\VS",
                KEY_READ,
            )
        }) {
        Ok(k) => k,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(instances),
        Err(e) => return Err(format!("registry: {}", e)),
    };

    for name in setup_key.enum_keys().filter_map(|r| r.ok()) {
        match setup_key.open_subkey_with_flags(&name, KEY_READ) {
            Ok(inst_key) => {
                let install_dir: Option<String> = inst_key.get_value("InstallDir").ok();
                if let Some(dir) = install_dir {
                    let path = PathBuf::from(dir);
                    // Check if this instance has MSVC tools
                    let msvc_dir = path.join("VC").join("Tools").join("MSVC");
                    if msvc_dir.exists() {
                        let version: String = inst_key
                            .get_value("InstallationVersion")
                            .or_else(|_| inst_key.get_value("Version"))
                            .unwrap_or_else(|_| "unknown".into());
                        instances.push(VsInstance {
                            install_path: path,
                            version,
                            label: name.clone(),
                        });
                    }
                }
            }
            Err(_) => continue,
        }
    }

    Ok(instances)
}

fn find_via_common_paths() -> Vec<VsInstance> {
    let candidates = [
        (
            r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools",
            "VS 2022 BuildTools",
        ),
        (
            r"C:\Program Files\Microsoft Visual Studio\2022\BuildTools",
            "VS 2022 BuildTools",
        ),
        (
            r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Community",
            "VS 2022 Community",
        ),
        (
            r"C:\Program Files\Microsoft Visual Studio\2022\Community",
            "VS 2022 Community",
        ),
        (
            r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional",
            "VS 2022 Professional",
        ),
        (
            r"C:\Program Files\Microsoft Visual Studio\2022\Professional",
            "VS 2022 Professional",
        ),
        (
            r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise",
            "VS 2022 Enterprise",
        ),
        (
            r"C:\Program Files\Microsoft Visual Studio\2022\Enterprise",
            "VS 2022 Enterprise",
        ),
    ];

    candidates
        .iter()
        .filter(|(p, _)| Path::new(p).join("VC").join("Tools").join("MSVC").exists())
        .map(|(p, label)| VsInstance {
            install_path: PathBuf::from(p),
            version: "17.0".into(),
            label: label.to_string(),
        })
        .collect()
}

/// List MSVC toolchain versions registered with the given VS instance.
pub fn list_vs_msvc_versions(vs_path: &Path) -> Vec<String> {
    let msvc_dir = vs_path.join("VC").join("Tools").join("MSVC");
    if !msvc_dir.exists() {
        return vec![];
    }

    let mut versions: Vec<String> = std::fs::read_dir(&msvc_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                Some(entry.file_name().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();

    versions.sort_by(|a, b| {
        // Sort by version number descending (latest first)
        let a_parts: Vec<u32> = a.split('.').filter_map(|s| s.parse().ok()).collect();
        let b_parts: Vec<u32> = b.split('.').filter_map(|s| s.parse().ok()).collect();
        b_parts.cmp(&a_parts)
    });

    versions
}

/// Find the MSVC toolchain version directory within an msvc-kit download directory.
///
/// msvc-kit downloads to a structure like:
///   <source_dir>/VC/Tools/MSVC/<version>/
pub fn find_msvc_kit_version(source_dir: &Path) -> Option<PathBuf> {
    let msvc_dir = source_dir.join("VC").join("Tools").join("MSVC");
    if !msvc_dir.exists() {
        return None;
    }

    std::fs::read_dir(&msvc_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| e.file_type().ok().is_some_and(|t| t.is_dir()))
        .map(|e| e.path())
}

/// Detect whether we can write to the VS MSVC toolchain directory.
pub fn can_write_to_vs(vs_path: &Path) -> bool {
    // Test specific paths in order, avoiding temporary borrow issues
    let candidates = [
        vs_path.join("VC").join("Tools").join("MSVC"),
        vs_path.join("VC").join("Tools"),
        vs_path.to_path_buf(),
    ];

    for candidate in &candidates {
        let test_file = candidate.join(".msvc_kit_write_test");
        match std::fs::write(&test_file, b"test") {
            Ok(_) => {
                let _ = std::fs::remove_file(&test_file);
                return true;
            }
            Err(_) => continue,
        }
    }
    false
}

/// Install a downloaded MSVC toolchain into a Visual Studio instance.
///
/// `source_dir` should point to the msvc-kit download root that contains
/// `VC/Tools/MSVC/<version>/`. The toolchain files are copied into the
/// VS instance's `VC/Tools/MSVC/<version>/` directory.
///
/// Returns the version string of the installed toolchain on success.
pub fn install_into_vs(
    source_dir: &Path,
    vs_instance: &VsInstance,
) -> Result<VsInstallResult, String> {
    let version_dir = find_msvc_kit_version(source_dir).ok_or_else(|| {
        format!(
            "No MSVC toolchain found in {}/VC/Tools/MSVC/",
            source_dir.display()
        )
    })?;

    let version_name = version_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid version directory name".to_string())?;

    let target_dir = vs_instance
        .install_path
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join(version_name);

    if target_dir.exists() {
        return Err(format!(
            "MSVC {} is already installed at {}",
            version_name,
            target_dir.display()
        ));
    }

    // Create parent directory
    let parent = target_dir
        .parent()
        .ok_or_else(|| "Invalid target path".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("Cannot create {}: {}", parent.display(), e))?;

    // Copy the toolchain files
    fn copy_recursively(src: &Path, dst: &Path) -> Result<(), String> {
        if src.is_dir() {
            std::fs::create_dir_all(dst)
                .map_err(|e| format!("Cannot create {}: {}", dst.display(), e))?;
            for entry in std::fs::read_dir(src)
                .map_err(|e| format!("Cannot read {}: {}", src.display(), e))?
            {
                let entry = entry.map_err(|e| format!("Entry error: {}", e))?;
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                copy_recursively(&src_path, &dst_path)?;
            }
            Ok(())
        } else if src.is_file() || src.is_symlink() {
            std::fs::copy(src, dst).map_err(|e| {
                format!("Cannot copy {} -> {}: {}", src.display(), dst.display(), e)
            })?;
            Ok(())
        } else {
            Ok(())
        }
    }

    copy_recursively(&version_dir, &target_dir)?;

    // Verify cl.exe exists
    let cl_path = target_dir
        .join("bin")
        .join("Hostx64")
        .join("x64")
        .join("cl.exe");
    if !cl_path.exists() {
        // Try alternative paths (different host/target combos)
        let alt_cl = target_dir
            .join("bin")
            .join("Hostx86")
            .join("x64")
            .join("cl.exe");
        if !alt_cl.exists() {
            return Err(format!(
                "Toolchain installed but cl.exe not found at expected path. \
                 Files copied to: {}. Verify manually.",
                target_dir.display()
            ));
        }
    }

    let registered = list_vs_msvc_versions(&vs_instance.install_path);

    Ok(VsInstallResult {
        vs_instance: vs_instance.clone(),
        registered_versions: registered,
        installed_source: Some(version_dir),
    })
}
