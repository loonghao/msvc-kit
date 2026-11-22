//! File extraction utilities for VSIX, MSI, and CAB files

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use sha2::Digest;

use super::path_safety::{
    oversized_archive_error, oversized_entry_error, unsafe_entry_error, PathOccupancy,
    SafeRelativePath, MAX_ARCHIVE_OUTPUT_BYTES, MAX_ENTRY_BYTES,
};
use crate::constants::{extraction as ext_const, progress as progress_const};
use crate::error::{MsvcKitError, Result};

/// Maximum retries for MSI extraction when encountering error 1618
const MSI_MAX_RETRIES: u32 = 5;
/// Delay between retries in milliseconds
const MSI_RETRY_DELAY_MS: u64 = 2000;

fn explain_windows_installer_code(code: Option<i32>) -> &'static str {
    match code {
        Some(1603) => {
            "Windows Installer error 1603 is a generic fatal install/extract failure. For Windows SDK administrative extraction this is often caused by optional SDK MSI payloads, path length limits, or a locked target directory."
        }
        Some(1618) => {
            "Windows Installer error 1618 means another installation is already in progress."
        }
        Some(1619) => {
            "Windows Installer error 1619 means the MSI package could not be opened. This is commonly caused by missing source media next to the MSI, an incomplete download cache, or an inaccessible path."
        }
        Some(1620) => {
            "Windows Installer error 1620 means the package is not a valid Windows Installer package for this environment."
        }
        _ => "Windows Installer returned a non-zero exit status.",
    }
}

fn prefer_shorter_path(original: &Path, candidate: Option<PathBuf>) -> PathBuf {
    candidate
        .filter(|path| path.as_os_str().len() < original.as_os_str().len())
        .unwrap_or_else(|| original.to_path_buf())
}

#[cfg(windows)]
fn windows_short_path(path: &Path) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    #[link(name = "kernel32")]
    extern "system" {
        fn GetShortPathNameW(
            lpszLongPath: *const u16,
            lpszShortPath: *mut u16,
            cchBuffer: u32,
        ) -> u32;
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    // Ask Windows for the buffer size first. If 8.3 names are disabled for
    // this path or any segment does not exist, this returns 0 and we fall back.
    let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return None;
    }

    let mut buffer = vec![0u16; needed as usize];
    let written = unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), needed) };
    if written == 0 || written >= needed {
        return None;
    }

    buffer.truncate(written as usize);
    Some(PathBuf::from(OsString::from_wide(&buffer)))
}

#[cfg(windows)]
fn msiexec_path_arg(path: &Path) -> String {
    let selected = prefer_shorter_path(path, windows_short_path(path));
    if selected != path {
        tracing::debug!(
            "Using Windows short path for msiexec: {:?} -> {:?}",
            path,
            selected
        );
    }
    // Windows Installer cannot operate on a path containing forward slashes:
    // `msiexec /a <msi>` returns 1619 ("package could not be opened") and an
    // administrative install into a forward-slash TARGETDIR returns 1603.
    // GetShortPathNameW preserves the original separators, so the short-path
    // fallback above is not sufficient on its own. Normalize the final argument
    // to backslashes so both the package path and TARGETDIR are always openable.
    selected.to_string_lossy().replace('/', "\\")
}

#[cfg(windows)]
fn format_msiexec_failure(
    status: std::process::ExitStatus,
    file_name: &str,
    msi_path: &Path,
    target_dir: &Path,
) -> String {
    let code = status.code();
    let mut message = format!(
        "msiexec failed with status: {} for {}\n{}\nSource MSI: {}\nTarget directory: {}",
        status,
        file_name,
        explain_windows_installer_code(code),
        msi_path.display(),
        target_dir.display()
    );

    match code {
        Some(1603) => message.push_str(
            "\nNext steps: retry with a shorter --target-dir such as C:\\msvc-kit, make sure antivirus or another installer is not locking the target directory, then rerun the command.",
        ),
        Some(1619) => message.push_str(
            "\nNext steps: delete the downloads/sdk cache for this SDK version and rerun so the MSI and its adjacent CAB source files are downloaded together.",
        ),
        Some(1618) => message.push_str(
            "\nNext steps: wait for Windows Update or another installer to finish, then rerun msvc-kit.",
        ),
        _ => message.push_str(
            "\nNext steps: rerun with RUST_LOG=debug for more context, or try a shorter target directory and a fresh download cache.",
        ),
    }

    message
}

pub(crate) fn inner_progress_enabled() -> bool {
    matches!(
        env::var("MSVC_KIT_INNER_PROGRESS")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(crate) fn progress_style_bytes() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] {wide_bar:.cyan/blue} {bytes}/{total_bytes} @ {bytes_per_sec} ETA {eta} | {msg}")
        .unwrap()
        .progress_chars("##-")
}

pub(crate) fn progress_style_items() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] {wide_bar:.cyan/blue} {pos}/{len} files | {msg}",
        )
        .unwrap()
        .progress_chars("##-")
}

/// Extract a VSIX file (which is a ZIP archive) with optional progress bar
pub(crate) async fn extract_vsix_with_progress(
    vsix_path: &Path,
    target_dir: &Path,
    show_progress: bool,
) -> Result<()> {
    let vsix_path = vsix_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || extract_vsix_sync(&vsix_path, &target_dir, show_progress))
        .await
        .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

/// Extract a VSIX file (which is a ZIP archive) with progress bar
pub async fn extract_vsix(vsix_path: &Path, target_dir: &Path) -> Result<()> {
    extract_vsix_with_progress(vsix_path, target_dir, inner_progress_enabled()).await
}

/// A validated VSIX/ZIP entry ready to be written.
struct PlannedVsixEntry {
    index: usize,
    safe: SafeRelativePath,
}

fn extract_vsix_sync(vsix_path: &Path, target_dir: &Path, show_progress: bool) -> Result<()> {
    // Pass 1: resolve and validate every entry name, symlink mode and size
    // limit, detect file/directory conflicts, and hash each entry's content
    // before anything is written. A rejected archive therefore leaves the
    // target tree untouched, and duplicate collision keys are resolved
    // upfront: equal bytes deduplicate, differing bytes are a hard conflict.
    // (Byte-identical duplicate names are already refused by the zip reader.)
    let (total_bytes, planned_dirs, planned_files) = {
        let file = File::open(vsix_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut total = 0u64;
        let mut occupancy = PathOccupancy::default();
        let mut seen: HashMap<String, (String, String)> = HashMap::new();
        let mut planned_dirs = Vec::new();
        let mut planned_files = Vec::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let name = entry.name()?.to_string();

            // Skip metadata files
            if name.starts_with('[') || name == "extension.vsixmanifest" {
                continue;
            }

            // Remove "Contents/" prefix if present
            let stripped = name.strip_prefix("Contents/").unwrap_or(&name);
            let is_dir = entry.is_dir() || stripped.ends_with('/') || stripped.ends_with('\\');
            let relative = stripped.trim_end_matches(['/', '\\']);
            let safe = SafeRelativePath::resolve(relative)
                .map_err(|error| unsafe_entry_error(&name, &error))?;

            if entry.is_symlink() {
                return Err(MsvcKitError::Other(format!(
                    "Refusing archive entry {name:?}: symlink entries are not extracted"
                )));
            }

            if is_dir {
                occupancy.register_dir(&safe).map_err(|conflict| {
                    MsvcKitError::Other(format!(
                        "Refusing archive entry {name:?}: file/directory path conflict with {conflict:?}"
                    ))
                })?;
                planned_dirs.push(safe);
                continue;
            }

            let size = entry.size();
            if size > MAX_ENTRY_BYTES {
                return Err(oversized_entry_error(&name, size));
            }

            occupancy.register_file(&safe).map_err(|conflict| {
                MsvcKitError::Other(format!(
                    "Refusing archive entry {name:?}: file/directory path conflict with {conflict:?}"
                ))
            })?;

            // Hash the full entry content before any file is written.
            let mut hasher = sha2::Sha256::new();
            let mut buffer = [0u8; ext_const::EXTRACT_BUFFER_SIZE];
            loop {
                let n = entry.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            let hash = hex::encode(hasher.finalize());

            let key = safe.collision_key();
            if let Some((first_path, existing_hash)) = seen.get(&key) {
                if existing_hash == &hash {
                    continue;
                }
                return Err(MsvcKitError::Other(format!(
                    "Archive entry collision: {first_path:?} and {name:?} resolve to the same path with different content"
                )));
            }
            seen.insert(key, (safe.display(), hash));

            total = total.saturating_add(size);
            if total > MAX_ARCHIVE_OUTPUT_BYTES {
                return Err(oversized_archive_error(total));
            }
            planned_files.push(PlannedVsixEntry { index, safe });
        }
        (total, planned_dirs, planned_files)
    };

    let pb = if show_progress {
        let pb = ProgressBar::new(total_bytes.max(1));
        pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(4));
        pb.set_style(progress_style_bytes());
        pb.set_message(
            vsix_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "extracting".to_string()),
        );
        Some(pb)
    } else {
        None
    };

    // Pass 2: write. Only entries that survived pass-1 deduplication remain,
    // so every write is a first occurrence and nothing is overwritten.
    let file = File::open(vsix_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for safe in &planned_dirs {
        let out_path = target_dir.join(safe.to_path_buf());
        std::fs::create_dir_all(&out_path)?;
    }

    for planned in planned_files {
        let mut entry = archive.by_index(planned.index)?;

        if let Some(pb) = pb.as_ref() {
            pb.set_message(planned.safe.display());
        }

        let out_path = target_dir.join(planned.safe.to_path_buf());
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out_file = File::create(&out_path)?;
        let mut buffer = [0u8; ext_const::EXTRACT_BUFFER_SIZE];
        loop {
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            out_file.write_all(&buffer[..n])?;
            if let Some(pb) = pb.as_ref() {
                pb.inc(n as u64);
            }
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message("Extracted");
    }
    Ok(())
}

/// Extract an MSI file
///
/// On Windows, uses msiexec. On other platforms, attempts to use msitools.
pub(crate) async fn extract_msi_with_progress(
    msi_path: &Path,
    target_dir: &Path,
    show_progress: bool,
) -> Result<()> {
    let msi_path = msi_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || extract_msi_sync(&msi_path, &target_dir, show_progress))
        .await
        .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

pub async fn extract_msi(msi_path: &Path, target_dir: &Path) -> Result<()> {
    extract_msi_with_progress(msi_path, target_dir, inner_progress_enabled()).await
}

fn extract_msi_sync(msi_path: &Path, target_dir: &Path, show_progress: bool) -> Result<()> {
    let file_name = msi_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.msi")
        .to_string();

    let pb = if show_progress {
        let pb = ProgressBar::new_spinner();
        pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(4));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap()
                .tick_chars("⠁⠃⠇⠋⠙⠸⠴⠦"),
        );
        pb.set_message(format!("msiexec extracting {}", file_name));
        pb.enable_steady_tick(Duration::from_millis(progress_const::PROGRESS_TICK_MS));
        Some(pb)
    } else {
        None
    };

    // Windows Installer is shared across targets and msvc-kit processes.
    // An OS lock releases automatically if a process exits or crashes.
    let _lock = crate::storage::lock_file(
        &crate::config::default_cache_root().join("locks/msi-extract.lock"),
    )?;

    #[cfg(windows)]
    {
        use std::process::Command;

        let msi_path_str = msiexec_path_arg(msi_path);
        let target_dir_str = format!("TARGETDIR={}", msiexec_path_arg(target_dir));

        // Retry loop for handling error 1618 (another installation in progress)
        // This can happen if system Windows Installer is busy with other operations
        let mut last_error = None;
        for attempt in 1..=MSI_MAX_RETRIES {
            let status = Command::new("msiexec")
                .args(["/a", &msi_path_str, "/qn", &target_dir_str])
                .status()?;

            if status.success() {
                if let Some(pb) = pb {
                    pb.finish_with_message(format!("MSI extracted: {}", file_name));
                }
                return Ok(());
            }

            // Check for error 1618 (another installation in progress)
            // This can still happen if system-level installers are running
            if let Some(code) = status.code() {
                if code == 1618 && attempt < MSI_MAX_RETRIES {
                    tracing::warn!(
                        "msiexec returned 1618 (another installation in progress) for {}, retry {}/{}",
                        file_name,
                        attempt,
                        MSI_MAX_RETRIES
                    );
                    if let Some(pb) = pb.as_ref() {
                        pb.set_message(format!(
                            "msiexec waiting (retry {}/{}) {}",
                            attempt, MSI_MAX_RETRIES, file_name
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(MSI_RETRY_DELAY_MS));
                    continue;
                }
            }

            last_error = Some(status);
            break;
        }

        if let Some(status) = last_error {
            if let Some(pb) = pb.as_ref() {
                pb.abandon_with_message(format!("msiexec failed: {}", file_name));
            }
            return Err(MsvcKitError::Other(format_msiexec_failure(
                status, &file_name, msi_path, target_dir,
            )));
        }
    }

    #[cfg(not(windows))]
    {
        // On non-Windows, try using msitools (msiextract)
        use std::process::Command;

        let status = Command::new("msiextract")
            .args([
                "-C",
                target_dir
                    .to_str()
                    .ok_or_else(|| MsvcKitError::Other("Invalid target path".to_string()))?,
                msi_path
                    .to_str()
                    .ok_or_else(|| MsvcKitError::Other("Invalid MSI path".to_string()))?,
            ])
            .status();

        match status {
            Ok(s) if s.success() => {
                if let Some(pb) = pb {
                    pb.finish_with_message(format!("MSI extracted: {}", file_name));
                }
                return Ok(());
            }
            Ok(s) => {
                if let Some(pb) = pb.as_ref() {
                    pb.abandon_with_message("msiextract failed");
                }
                return Err(MsvcKitError::Other(format!(
                    "msiextract failed with status: {}",
                    s
                )));
            }
            Err(e) => {
                if let Some(pb) = pb.as_ref() {
                    pb.abandon_with_message("msiextract failed");
                }
                return Err(MsvcKitError::Other(format!(
                    "Failed to run msiextract (is msitools installed?): {}",
                    e
                )));
            }
        }
    }

    #[cfg(windows)]
    {
        if let Some(pb) = pb {
            pb.finish_with_message(format!("MSI extracted: {}", file_name));
        }
        Ok(())
    }
}

/// Extract a CAB file with a simple file-count progress bar
pub(crate) async fn extract_cab_with_progress(
    cab_path: &Path,
    target_dir: &Path,
    show_progress: bool,
) -> Result<()> {
    let cab_path = cab_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || extract_cab_sync(&cab_path, &target_dir, show_progress))
        .await
        .map_err(|e| MsvcKitError::Other(format!("Task join error: {}", e)))??;

    Ok(())
}

pub async fn extract_cab(cab_path: &Path, target_dir: &Path) -> Result<()> {
    extract_cab_with_progress(cab_path, target_dir, inner_progress_enabled()).await
}

fn extract_cab_sync(cab_path: &Path, target_dir: &Path, show_progress: bool) -> Result<()> {
    // Pass 1: resolve and validate every entry name and size limit before
    // anything is written, so a rejected archive leaves the target untouched.
    // The cab crate looks files up by name, so duplicate entry names cannot be
    // extracted unambiguously and are rejected outright.
    let planned: Vec<(String, SafeRelativePath)> = {
        let file = File::open(cab_path)?;
        let cabinet = cab::Cabinet::new(file)
            .map_err(|e| MsvcKitError::Cab(format!("Failed to open CAB: {}", e)))?;
        let mut occupancy = PathOccupancy::default();
        let mut planned = Vec::new();
        let mut total_bytes = 0u64;
        for entry in cabinet
            .folder_entries()
            .flat_map(|folder| folder.file_entries())
        {
            let name = entry.name().to_string();
            let safe = SafeRelativePath::resolve(&name)
                .map_err(|error| unsafe_entry_error(&name, &error))?;
            // The cab crate resolves entries by name, so duplicates cannot be
            // read unambiguously and must not be extracted.
            if occupancy.contains_file(&safe.collision_key()) {
                return Err(MsvcKitError::Other(format!(
                    "Refusing CAB entry {name:?}: duplicate entry name in cabinet"
                )));
            }
            occupancy.register_file(&safe).map_err(|conflict| {
                MsvcKitError::Other(format!(
                    "Refusing CAB entry {name:?}: file/directory path conflict with {conflict:?}"
                ))
            })?;
            let size = entry.uncompressed_size() as u64;
            if size > MAX_ENTRY_BYTES {
                return Err(oversized_entry_error(&name, size));
            }
            total_bytes = total_bytes.saturating_add(size);
            if total_bytes > MAX_ARCHIVE_OUTPUT_BYTES {
                return Err(oversized_archive_error(total_bytes));
            }
            planned.push((name, safe));
        }
        planned
    };

    let total_files = planned.len() as u64;
    let pb = if show_progress {
        let pb = ProgressBar::new(total_files.max(1));
        pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(4));
        pb.set_style(progress_style_items());
        pb.set_message(
            cab_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Extracting CAB".to_string()),
        );
        Some(pb)
    } else {
        None
    };

    // Pass 2: extraction (cab crate requires re-opening for each file read).
    for (idx, (name, safe)) in planned.iter().enumerate() {
        let out_path = target_dir.join(safe.to_path_buf());

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if let Some(pb) = pb.as_ref() {
            pb.set_message(format!("{} ({}/{})", safe.display(), idx + 1, total_files));
        }

        // Re-open cabinet to read the file (cab crate limitation)
        let file = File::open(cab_path)?;
        let mut cabinet = cab::Cabinet::new(file)
            .map_err(|e| MsvcKitError::Cab(format!("Failed to open CAB: {}", e)))?;

        let mut reader = cabinet
            .read_file(name)
            .map_err(|e| MsvcKitError::Cab(format!("Failed to read file {}: {}", name, e)))?;

        let mut out_file = File::create(&out_path)?;
        let mut buffer = [0u8; ext_const::EXTRACT_BUFFER_SIZE];
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|e| MsvcKitError::Cab(format!("Failed to read file content: {}", e)))?;
            if n == 0 {
                break;
            }
            out_file.write_all(&buffer[..n])?;
        }

        if let Some(pb) = pb.as_ref() {
            pb.inc(1);
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message("CAB extracted");
    }
    Ok(())
}

/// Determine the extraction method based on file extension
pub fn get_extractor(path: &Path) -> Option<fn(&Path, &Path) -> Result<()>> {
    let extension = path.extension()?.to_str()?.to_lowercase();

    match extension.as_str() {
        "vsix" | "zip" => {
            Some(|p, t| tokio::runtime::Handle::current().block_on(extract_vsix(p, t)))
        }
        "msi" => Some(|p, t| tokio::runtime::Handle::current().block_on(extract_msi(p, t))),
        "cab" => Some(|p, t| tokio::runtime::Handle::current().block_on(extract_cab(p, t))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(unused_imports)]
    use tempfile::TempDir;

    #[test]
    fn test_windows_installer_code_explanations() {
        assert!(explain_windows_installer_code(Some(1603)).contains("fatal"));
        assert!(explain_windows_installer_code(Some(1618)).contains("another installation"));
        assert!(explain_windows_installer_code(Some(1619)).contains("could not be opened"));
        assert!(explain_windows_installer_code(Some(1620)).contains("not a valid"));
        assert!(explain_windows_installer_code(None).contains("non-zero"));
    }

    #[test]
    fn test_prefer_shorter_path() {
        let original = Path::new(
            "C:/Users/example/AppData/Local/loonghao/msvc-kit/data/downloads/sdk/26100_arm64/Installers",
        );
        let shorter = PathBuf::from("C:/Users/EXAMPL~1/AppData/Local/LOONGH~1/MSVC-K~1/data");
        let longer = PathBuf::from(format!(
            "{}/extra/path/that/is/not/shorter",
            original.display()
        ));

        assert_eq!(
            prefer_shorter_path(original, Some(shorter.clone())),
            shorter
        );
        assert_eq!(prefer_shorter_path(original, Some(longer)), original);
        assert_eq!(prefer_shorter_path(original, None), original);
    }

    #[cfg(windows)]
    #[test]
    fn test_msiexec_path_arg_returns_existing_path() {
        let temp = TempDir::new().unwrap();
        let selected = msiexec_path_arg(temp.path());

        assert!(!selected.is_empty());
        assert!(Path::new(&selected).exists());
        assert!(selected.len() <= temp.path().to_string_lossy().len());
    }

    /// The VS 2026 / SDK 10.0.28000 manifest yields forward-slash payload paths
    /// such as `Redistributable/10.1.0.0/UAPSDKAddOn-x86.msi`. A path containing
    /// `/` makes `msiexec /a` fail with 1619, so the argument handed to msiexec
    /// must never contain a forward slash.
    #[cfg(windows)]
    #[test]
    fn test_msiexec_path_arg_strips_forward_slashes() {
        let forward = Path::new(
            "C:/cache/downloads/sdk/28000_x64/Redistributable/10.1.0.0/UAPSDKAddOn-x86.msi",
        );
        let arg = msiexec_path_arg(forward);

        assert!(
            !arg.contains('/'),
            "msiexec argument must not contain '/': {arg}"
        );
        assert!(
            arg.contains(r"Redistributable\10.1.0.0\UAPSDKAddOn-x86.msi"),
            "forward slashes must be normalized to backslashes: {arg}"
        );
    }

    /// Regression guard for the working v17 / SDK 10.0.26100 path: an
    /// already-backslash argument must be passed through unchanged.
    #[cfg(windows)]
    #[test]
    fn test_msiexec_path_arg_preserves_backslash_paths() {
        let backslash =
            Path::new(r"C:\cache\downloads\sdk\26100_x64\Installers\Windows SDK Headers-x86.msi");
        let arg = msiexec_path_arg(backslash);

        assert!(!arg.contains('/'));
        assert!(
            arg.ends_with(r"Installers\Windows SDK Headers-x86.msi"),
            "backslash layout must be preserved: {arg}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_format_msiexec_failure_includes_guidance() {
        use std::process::Command;

        let status_1603 = Command::new("cmd")
            .args(["/C", "exit", "1603"])
            .status()
            .unwrap();
        let message_1603 = format_msiexec_failure(
            status_1603,
            "WinRT Intellisense UAP - en-us-x86_en-us.msi",
            Path::new(
                "C:/msvc-kit/downloads/sdk/Installers/WinRT Intellisense UAP - en-us-x86_en-us.msi",
            ),
            Path::new("C:/msvc-kit"),
        );
        assert!(message_1603.contains("1603"));
        assert!(message_1603.contains("shorter --target-dir"));
        assert!(message_1603.contains("Source MSI:"));
        assert!(message_1603.contains("Target directory:"));

        let status_1619 = Command::new("cmd")
            .args(["/C", "exit", "1619"])
            .status()
            .unwrap();
        let message_1619 = format_msiexec_failure(
            status_1619,
            "Application Verifier arm64 External Package (DesktopEditions)-arm64_en-us.msi",
            Path::new("C:/msvc-kit/downloads/sdk/Installers/Application Verifier arm64 External Package (DesktopEditions)-arm64_en-us.msi"),
            Path::new("C:/msvc-kit"),
        );
        assert!(message_1619.contains("1619"));
        assert!(message_1619.contains("downloads/sdk cache"));
        assert!(message_1619.contains("adjacent CAB source files"));
    }

    #[test]
    fn test_get_extractor() {
        assert!(get_extractor(Path::new("test.vsix")).is_some());
        assert!(get_extractor(Path::new("test.msi")).is_some());
        assert!(get_extractor(Path::new("test.cab")).is_some());
        assert!(get_extractor(Path::new("test.unknown")).is_none());
    }
}
