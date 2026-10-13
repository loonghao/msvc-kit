//! Filesystem layout rules shared by resolution, queries and diagnostics.

use std::path::{Path, PathBuf};

use crate::error::{MsvcKitError, Result};
use crate::version::Architecture;

/// VS2015 has no versioned tools directory. Identify only the v140 family;
/// CRT build numbers are not evidence of an exact compiler patch version.
pub(crate) fn is_legacy_vc(vc: &Path) -> bool {
    let Ok(header) = std::fs::read_to_string(vc.join("include/crtversion.h")) else {
        return false;
    };
    let define = |name: &str, value: &str| {
        header.lines().any(|line| {
            let mut tokens = line.split_whitespace();
            tokens.next() == Some("#define")
                && tokens.next() == Some(name)
                && tokens.next() == Some(value)
        })
    };
    vc.join("vcvarsall.bat").is_file()
        && define("_VC_CRT_MAJOR_VERSION", "14")
        && define("_VC_CRT_MINOR_VERSION", "0")
}

pub(crate) fn msvc_lib_dir(vc: &Path, arch: Architecture) -> PathBuf {
    if is_legacy_vc(vc) {
        match arch {
            Architecture::X86 => vc.join("lib"),
            Architecture::X64 => vc.join("lib/amd64"),
            _ => vc.join("lib").join(arch.to_string()),
        }
    } else {
        vc.join("lib").join(arch.to_string())
    }
}

pub(crate) fn msvc_bin_dirs(
    vc: &Path,
    host: Architecture,
    target: Architecture,
) -> Result<Vec<PathBuf>> {
    if !is_legacy_vc(vc) {
        return Ok(vec![vc
            .join("bin")
            .join(host.msvc_host_dir())
            .join(target.msvc_target_dir())]);
    }
    let bin = vc.join("bin");
    let selected = match (host, target) {
        (Architecture::X86, Architecture::X86) => bin.clone(),
        (Architecture::X86, Architecture::X64) => bin.join("x86_amd64"),
        (Architecture::X64, Architecture::X86) => bin.join("amd64_x86"),
        (Architecture::X64, Architecture::X64) => bin.join("amd64"),
        _ => return Err(MsvcKitError::UnsupportedPlatform(format!(
            "VS2015 legacy layout supports x86/x64 hosts and targets only; requested {host}/{target}"
        ))),
    };
    for tool in ["cl.exe", "link.exe", "lib.exe"] {
        if !selected.join(tool).is_file() {
            return Err(MsvcKitError::ComponentNotFound(format!(
                "VS2015 {host}/{target} requires {}; refusing another host/target fallback",
                selected.join(tool).display()
            )));
        }
    }
    // Cross compiler DLLs (including mspdb) live in the native host directory.
    // Keep the selected compiler first; tool lookup must never search this fallback.
    let native = if host == Architecture::X64 {
        bin.join("amd64")
    } else {
        bin
    };
    let mut paths = vec![selected];
    if paths[0] != native {
        paths.push(native);
    }
    Ok(paths)
}

pub(crate) fn sdk_bin_dir(sdk: &Path, version: &str, host: Architecture) -> PathBuf {
    let versioned = sdk.join("bin").join(version);
    // Windows 10 SDK binaries became versioned with 10.0.15063.0. Never
    // rescue an incomplete modern SDK with unrelated unversioned tools.
    if !versioned.is_dir()
        && matches!(version, "10.0.10240.0" | "10.0.10586.0" | "10.0.14393.0")
        && matches!(host, Architecture::X86 | Architecture::X64)
    {
        sdk.join("bin").join(host.to_string())
    } else {
        versioned.join(host.to_string())
    }
}
