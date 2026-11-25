//! Unified safe relative-path resolution for archive extraction output.
//!
//! Every archive extractor (VSIX/ZIP, CAB, and future native MSI backends)
//! resolves each untrusted entry name through [`SafeRelativePath`] before any
//! byte is written, using Windows path semantics on every host so the
//! guarantees are platform-independent (spec 05 §5).
//!
//! The resolver is deliberately conservative: entries that resolve ambiguously
//! on Windows (drive-relative paths, device names, trailing dots/spaces,
//! alternate data streams) are rejected outright rather than normalized.

use std::path::PathBuf;

/// Maximum uncompressed size of a single archive entry (8 GiB).
pub(crate) const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Maximum total uncompressed output of one archive (64 GiB).
pub(crate) const MAX_ARCHIVE_OUTPUT_BYTES: u64 = 64 * 1024 * 1024 * 1024;

/// Reserved Win32 device names, matched case-insensitively on the component
/// stem before the first dot, so `NUL.txt` is rejected alongside `NUL`.
const RESERVED_DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why an archive entry path was considered unsafe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathSafetyError {
    /// The entry name is empty (or only separators).
    EmptyPath,
    /// The entry is an absolute POSIX path (`/...`).
    AbsolutePosix,
    /// The entry uses a drive letter (`C:...`), absolute or drive-relative.
    WindowsDrive,
    /// The entry is a UNC share path (`\\server\share...`, `//server/share...`).
    WindowsUnc,
    /// The entry uses an extended path prefix (`\\?\`, `\\.\`, `\\??\`).
    WindowsExtendedPrefix,
    /// A `..` component would traverse out of the extraction root.
    ParentTraversal,
    /// A `.` component is present.
    CurrentDirComponent,
    /// A path component is empty (double separators, trailing separator).
    EmptyComponent,
    /// The entry contains an alternate data stream (`name:stream`).
    AlternateDataStream,
    /// The entry stem is a reserved Win32 device name.
    ReservedDeviceName(String),
    /// A component ends with a dot or space, colliding under Win32
    /// normalization with the same name without it.
    TrailingDotOrSpace,
}

impl PathSafetyError {
    /// Human-readable reason for diagnostics. Callers must not branch on the
    /// text; the enum is the control-flow contract.
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::EmptyPath => "entry path is empty".to_string(),
            Self::AbsolutePosix => {
                "entry is an absolute POSIX path outside the extraction root".to_string()
            }
            Self::WindowsDrive => "entry uses a drive-letter path".to_string(),
            Self::WindowsUnc => "entry is a UNC share path".to_string(),
            Self::WindowsExtendedPrefix => "entry uses an extended path prefix".to_string(),
            Self::ParentTraversal => "entry traverses out of the extraction root".to_string(),
            Self::CurrentDirComponent => "entry contains a '.' path component".to_string(),
            Self::EmptyComponent => "entry contains an empty path component".to_string(),
            Self::AlternateDataStream => "entry names an NTFS alternate data stream".to_string(),
            Self::ReservedDeviceName(name) => {
                format!("entry uses reserved Win32 device name {name:?}")
            }
            Self::TrailingDotOrSpace => {
                "entry has a trailing dot or space that collides under Win32 normalization"
                    .to_string()
            }
        }
    }
}

/// Build the error surfaced to callers when an entry name is unsafe.
pub(crate) fn unsafe_entry_error(entry: &str, error: &PathSafetyError) -> crate::MsvcKitError {
    crate::MsvcKitError::Other(format!(
        "Refusing unsafe archive entry {:?}: {}",
        entry,
        error.reason()
    ))
}

/// Build the error for an archive that exceeds the uncompressed size caps.
pub(crate) fn oversized_entry_error(entry: &str, size: u64) -> crate::MsvcKitError {
    crate::MsvcKitError::Other(format!(
        "Refusing archive entry {entry:?}: declared uncompressed size {size} exceeds the {MAX_ENTRY_BYTES} byte limit"
    ))
}

pub(crate) fn oversized_archive_error(total: u64) -> crate::MsvcKitError {
    crate::MsvcKitError::Other(format!(
        "Refusing archive: total declared uncompressed size {total} exceeds the {MAX_ARCHIVE_OUTPUT_BYTES} byte limit"
    ))
}

/// Strip a single trailing path separator (`/` or `\\`) from an entry name.
///
/// R2: a directory entry is conventionally written as `dir/`. Exactly one
/// trailing separator is stripped before validation; any remaining empty
/// component (from `dir//`) or `.` component (from `dir/./`) is then rejected
/// by [`SafeRelativePath::resolve`]. Stripping all trailing separators would
/// wrongly accept `dir//`.
pub(crate) fn strip_one_trailing_separator(entry_name: &str) -> &str {
    let bytes = entry_name.as_bytes();
    match bytes.last() {
        Some(b'/') | Some(b'\\') => &entry_name[..bytes.len() - 1],
        _ => entry_name,
    }
}

/// A validated, normalized archive entry path relative to the extraction root.
///
/// Every component passed strict Windows-semantics validation; the path can
/// never escape the root it is joined onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SafeRelativePath {
    components: Vec<String>,
}

impl SafeRelativePath {
    /// Resolve an untrusted archive entry name under Windows path semantics.
    ///
    /// The name must already be relative and separator-terminated as a plain
    /// file name (callers strip trailing separators for directory entries).
    pub(crate) fn resolve(entry_name: &str) -> Result<Self, PathSafetyError> {
        if entry_name.is_empty() {
            return Err(PathSafetyError::EmptyPath);
        }
        // POSIX absolute and forward-slash UNC: "/x", "//server/share".
        if let Some(rest) = entry_name.strip_prefix('/') {
            if rest.starts_with('/') {
                return Err(PathSafetyError::WindowsUnc);
            }
            return Err(PathSafetyError::AbsolutePosix);
        }
        // Verbatim/device prefixes and backslash UNC: "\\?\C:\x", "\\.\x",
        // "\\??\x", "\\server\share".
        if entry_name.starts_with("\\\\?\\")
            || entry_name.starts_with("\\\\.\\")
            || entry_name.starts_with("\\\\??\\")
        {
            return Err(PathSafetyError::WindowsExtendedPrefix);
        }
        if entry_name.starts_with("\\\\") || entry_name.starts_with("//") {
            return Err(PathSafetyError::WindowsUnc);
        }
        // Any remaining colon is either a drive letter ("C:", "C:x") or an
        // alternate data stream ("file.txt:stream"); both are unsafe.
        if let Some(index) = entry_name.find(':') {
            if index == 1 && entry_name.as_bytes()[0].is_ascii_alphabetic() {
                return Err(PathSafetyError::WindowsDrive);
            }
            return Err(PathSafetyError::AlternateDataStream);
        }

        let mut components = Vec::new();
        for component in entry_name.split(['/', '\\']) {
            if component.is_empty() {
                return Err(PathSafetyError::EmptyComponent);
            }
            if component == "." {
                return Err(PathSafetyError::CurrentDirComponent);
            }
            if component == ".." {
                return Err(PathSafetyError::ParentTraversal);
            }
            let stem = component.split('.').next().unwrap_or(component);
            if RESERVED_DEVICE_NAMES
                .iter()
                .any(|device| device.eq_ignore_ascii_case(stem))
            {
                return Err(PathSafetyError::ReservedDeviceName(component.to_string()));
            }
            if component.ends_with('.') || component.ends_with(' ') {
                return Err(PathSafetyError::TrailingDotOrSpace);
            }
            components.push(component.to_string());
        }
        if components.is_empty() {
            return Err(PathSafetyError::EmptyPath);
        }
        Ok(Self { components })
    }

    /// Collision key: NFC-normalized, case-folded canonical path.
    ///
    /// Windows matches file names case-insensitively and preserves (but does
    /// not normalize) Unicode forms, so two entries whose keys are equal
    /// address the same physical file. Entries with equal keys are resolved
    /// by content: identical bytes deduplicate, differing bytes conflict.
    pub(crate) fn collision_key(&self) -> String {
        use unicode_normalization::UnicodeNormalization;
        self.components
            .join("\\")
            .nfc()
            .collect::<String>()
            .to_lowercase()
    }

    /// Canonical relative path, built per component so it is host-correct.
    pub(crate) fn to_path_buf(&self) -> PathBuf {
        let mut path = PathBuf::new();
        for component in &self.components {
            path.push(component);
        }
        path
    }

    /// Display form for diagnostics (backslash-separated, like Windows).
    pub(crate) fn display(&self) -> String {
        self.components.join("\\")
    }
}

/// Keys of every proper ancestor directory of the path ("a\b\c" yields
/// "a" and "a\b").
pub(crate) fn ancestor_keys(path: &SafeRelativePath) -> Vec<String> {
    let mut keys = Vec::new();
    for end in 1..path.components.len() {
        keys.push(path.components[..end].join("\\"));
    }
    keys.into_iter()
        .map(|ancestor| {
            use unicode_normalization::UnicodeNormalization;
            ancestor.nfc().collect::<String>().to_lowercase()
        })
        .collect()
}

/// Tracks occupied paths while planning an archive so file/directory
/// conflicts are detected before anything is written.
#[derive(Debug, Default)]
pub(crate) struct PathOccupancy {
    file_keys: std::collections::HashMap<String, String>,
    dir_keys: std::collections::HashSet<String>,
}

impl PathOccupancy {
    /// Register a validated file entry. Returns `Err(path)` when the entry
    /// collides with an already-occupied directory or one of its ancestors is
    /// an already-registered file ("a" file + "a/b" file).
    pub(crate) fn register_file(&mut self, path: &SafeRelativePath) -> Result<(), String> {
        let key = path.collision_key();
        if self.dir_keys.contains(&key) {
            return Err(path.display());
        }
        if ancestor_keys(path)
            .iter()
            .any(|ancestor| self.file_keys.contains_key(ancestor))
        {
            return Err(path.display());
        }
        self.file_keys.insert(key, path.display());
        for ancestor in ancestor_keys(path) {
            self.dir_keys.insert(ancestor);
        }
        Ok(())
    }

    /// Register an explicit directory entry.
    pub(crate) fn register_dir(&mut self, path: &SafeRelativePath) -> Result<(), String> {
        let key = path.collision_key();
        if self.file_keys.contains_key(&key) {
            return Err(path.display());
        }
        if ancestor_keys(path)
            .iter()
            .any(|ancestor| self.file_keys.contains_key(ancestor))
        {
            return Err(path.display());
        }
        self.dir_keys.insert(key);
        for ancestor in ancestor_keys(path) {
            self.dir_keys.insert(ancestor);
        }
        Ok(())
    }

    /// Whether a file entry with this key was already registered.
    pub(crate) fn contains_file(&self, key: &str) -> bool {
        self.file_keys.contains_key(key)
    }
}

/// Check that no component of `path` (from `root` down to the full path) is
/// a reparse point (symlink/junction). Returns the first offending component.
/// G-6 (R7): prevents a symlink placed inside the staging tree from
/// redirecting writes outside the boundary (TOCTOU hardening).
///
/// Fail-closed: a path that is not lexically inside `root` (a `..` or an
/// absolute component from an untrusted journal) is refused, and a metadata
/// error other than "not found" is propagated instead of being treated as a
/// clean component. On Windows, `FileType::is_symlink` is true for both
/// symlinks and junctions (both reparse tags are name surrogates).
pub(crate) fn check_no_reparse_within(
    root: &std::path::Path,
    path: &std::path::Path,
) -> std::result::Result<(), crate::MsvcKitError> {
    // G-6 (R7): check the root itself first (it may be a reparse point),
    // then every component from root down to the full path.
    check_not_reparse(root)?;
    let relative = path.strip_prefix(root).map_err(|_| {
        crate::MsvcKitError::Other(format!(
            "Refusing path {:?}: it is outside the install root {:?}",
            path, root
        ))
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(crate::MsvcKitError::Other(format!(
                "Refusing path {:?}: non-normal component in the path below the install root",
                path
            )));
        }
        current.push(component);
        check_not_reparse(&current)?;
    }
    Ok(())
}

/// Classify an existing path without ever following a symlink/junction.
///
/// Returns the metadata when the path is a real directory or file, `Ok(None)`
/// when it does not exist, and an error for any other metadata failure or for
/// any reparse point (which may be a symlink, a Windows junction, or another
/// name-surrogate reparse tag). Security decisions must use this instead of
/// `Path::is_dir`/`is_file`/`exists`, which follow links and swallow errors.
pub(crate) fn real_dir_or_file(
    path: &std::path::Path,
) -> std::result::Result<Option<std::fs::Metadata>, crate::MsvcKitError> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if meta.file_type().is_symlink() {
        return Err(crate::MsvcKitError::Other(format!(
            "Refusing reparse point at {:?}",
            path
        )));
    }
    Ok(Some(meta))
}

/// Ensure a directory exists under `root` as a chain of real directories.
///
/// The relative path must be non-empty and contain only normal components;
/// no absolute paths or `..` are accepted. Every existing component is
/// checked with [`check_no_reparse_within`] before it is used, each directory
/// is created one level at a time (never a recursive traversal through an
/// unknown path), and every created level is re-checked to be a real
/// directory. Suspicious paths are reported, never repaired or removed.
pub(crate) fn ensure_safe_directory(
    root: &std::path::Path,
    relative: &std::path::Path,
) -> std::result::Result<std::path::PathBuf, crate::MsvcKitError> {
    if relative.as_os_str().is_empty() {
        return Err(crate::MsvcKitError::Other(
            "Refusing empty relative directory path".to_string(),
        ));
    }
    let mut current = root.to_path_buf();
    check_no_reparse_within(root, &current)?;
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(crate::MsvcKitError::Other(format!(
                "Refusing unsafe directory component in {:?}",
                relative
            )));
        }
        current.push(component);
        check_no_reparse_within(root, &current)?;
        match real_dir_or_file(&current)? {
            Some(meta) if meta.is_dir() => {}
            Some(_) => {
                return Err(crate::MsvcKitError::Other(format!(
                    "Refusing non-directory at {:?}",
                    current
                )))
            }
            None => {
                std::fs::create_dir(&current)?;
                // Re-validate after creation: the component must be a real
                // directory and must not have become a reparse point.
                match real_dir_or_file(&current)? {
                    Some(meta) if meta.is_dir() => {}
                    other => {
                        return Err(crate::MsvcKitError::Other(format!(
                            "Unexpected state {:?} after creating directory {:?}",
                            other.map(|meta| meta.file_type().is_dir()),
                            current
                        )))
                    }
                }
            }
        }
    }
    Ok(current)
}

/// Refuse a single path if it is a reparse point. A missing path is fine (it
/// will be created/checked again later); any other metadata failure means the
/// path state is unknown and is therefore refused.
fn check_not_reparse(path: &std::path::Path) -> std::result::Result<(), crate::MsvcKitError> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(crate::MsvcKitError::Other(format!(
            "Refusing to write through reparse point at {:?}",
            path
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(name: &str) -> Result<SafeRelativePath, PathSafetyError> {
        SafeRelativePath::resolve(name)
    }

    #[test]
    fn benign_names_resolve() {
        for name in [
            "file.txt",
            "a/b/c.txt",
            "中文 目录/深 层/file name.txt",
            "a.b.c.txt",
            "COM10.txt",
            "auxiliary/x.txt",
        ] {
            let resolved = resolve(name).unwrap_or_else(|e| panic!("{name} rejected: {e:?}"));
            assert_eq!(resolved.display(), name.replace('/', "\\"));
        }
    }

    #[test]
    fn traversal_is_rejected() {
        for name in ["../escape.txt", "a/../../escape.txt", "..\\escape.txt"] {
            assert_eq!(resolve(name).unwrap_err(), PathSafetyError::ParentTraversal);
        }
    }

    #[test]
    fn absolute_forms_are_rejected() {
        assert_eq!(
            resolve("/absolute/file").unwrap_err(),
            PathSafetyError::AbsolutePosix
        );
        assert_eq!(
            resolve("C:\\abs").unwrap_err(),
            PathSafetyError::WindowsDrive
        );
        assert_eq!(
            resolve("C:/abs").unwrap_err(),
            PathSafetyError::WindowsDrive
        );
        assert_eq!(
            resolve("C:relative").unwrap_err(),
            PathSafetyError::WindowsDrive
        );
        assert_eq!(
            resolve("\\\\server\\share").unwrap_err(),
            PathSafetyError::WindowsUnc
        );
        assert_eq!(
            resolve("//server/share").unwrap_err(),
            PathSafetyError::WindowsUnc
        );
        assert_eq!(
            resolve("\\\\?\\C:\\outside").unwrap_err(),
            PathSafetyError::WindowsExtendedPrefix
        );
        assert_eq!(
            resolve("\\\\.\\device").unwrap_err(),
            PathSafetyError::WindowsExtendedPrefix
        );
    }

    #[test]
    fn ads_is_rejected() {
        assert_eq!(
            resolve("file.txt:payload").unwrap_err(),
            PathSafetyError::AlternateDataStream
        );
        assert_eq!(
            resolve("a/b:stream").unwrap_err(),
            PathSafetyError::AlternateDataStream
        );
    }

    #[test]
    fn reserved_names_are_rejected_with_and_without_extensions() {
        for name in ["CON", "NUL.txt", "AUX", "PRN", "COM1.log", "LPT9", "com7"] {
            let error = resolve(name).unwrap_err();
            assert!(
                matches!(error, PathSafetyError::ReservedDeviceName(_)),
                "{name} should be rejected as reserved, got {error:?}"
            );
        }
        // Reserve names must not over-match substrings.
        assert!(resolve("auxiliary/x.txt").is_ok());
        assert!(resolve("console.txt").is_ok());
    }

    #[test]
    fn trailing_dot_space_and_dot_components_are_rejected() {
        assert_eq!(
            resolve("foo.").unwrap_err(),
            PathSafetyError::TrailingDotOrSpace
        );
        assert_eq!(
            resolve("bar ").unwrap_err(),
            PathSafetyError::TrailingDotOrSpace
        );
        assert_eq!(
            resolve("a/./b").unwrap_err(),
            PathSafetyError::CurrentDirComponent
        );
        assert_eq!(
            resolve("a//b").unwrap_err(),
            PathSafetyError::EmptyComponent
        );
        assert_eq!(resolve("").unwrap_err(), PathSafetyError::EmptyPath);
        assert_eq!(
            resolve("dir/").unwrap_err(),
            PathSafetyError::EmptyComponent
        );
    }

    #[test]
    fn collision_key_folds_case_and_unicode_normalization() {
        let upper = resolve("CAF\u{c9}.TXT").unwrap();
        let lower = resolve("caf\u{e9}.txt").unwrap();
        let decomposed = resolve("cafe\u{301}.txt").unwrap();
        assert_eq!(upper.collision_key(), lower.collision_key());
        assert_eq!(lower.collision_key(), decomposed.collision_key());
        assert_ne!(
            resolve("a/b.txt").unwrap().collision_key(),
            resolve("a/bb.txt").unwrap().collision_key()
        );
    }

    #[test]
    fn occupancy_detects_file_directory_conflicts() {
        let mut occupancy = PathOccupancy::default();
        let file_a = resolve("a").unwrap();
        occupancy.register_file(&file_a).unwrap();
        // A file beneath an existing file path conflicts.
        let nested = resolve("a/b.txt").unwrap();
        assert_eq!(occupancy.register_file(&nested).unwrap_err(), "a\\b.txt");
        // An explicit directory on an existing file path conflicts.
        let dir_a = resolve("a").unwrap();
        assert_eq!(occupancy.register_dir(&dir_a).unwrap_err(), "a");
    }

    #[test]
    fn occupancy_detects_directory_then_file_conflicts() {
        let mut occupancy = PathOccupancy::default();
        occupancy.register_dir(&resolve("dir").unwrap()).unwrap();
        // A file where an explicit directory stands conflicts.
        assert!(occupancy.register_file(&resolve("dir").unwrap()).is_err());
        // Sibling files under a registered directory are fine.
        assert!(occupancy
            .register_file(&resolve("dir/file.txt").unwrap())
            .is_ok());
    }

    #[test]
    fn to_path_buf_is_relative_and_ordered() {
        let path = resolve("VC/Tools/MSVC/14.44/x.txt").unwrap().to_path_buf();
        let text = path.to_string_lossy().replace('\\', "/");
        assert_eq!(text, "VC/Tools/MSVC/14.44/x.txt");
        assert!(path.is_relative());
    }

    /// G-7 (R2): a directory entry written as `dir/` is accepted after
    /// stripping exactly one trailing separator; `dir//` (a remaining empty
    /// component) and `dir/./` (a remaining `.` component) are still rejected.
    #[test]
    fn g7_trailing_separator_strips_exactly_one() {
        // Positive: `dir/` and `dir\\` strip to `dir` and resolve.
        for raw in ["dir/", "dir\\"] {
            let stripped = strip_one_trailing_separator(raw);
            assert_eq!(stripped, "dir");
            let resolved = resolve(stripped).unwrap_or_else(|e| panic!("{raw} rejected: {e:?}"));
            assert_eq!(resolved.display(), "dir");
        }

        // Negative: `dir//` strips one separator, leaving `dir/` which has an
        // empty component — still rejected.
        assert_eq!(strip_one_trailing_separator("dir//"), "dir/");
        assert_eq!(
            resolve(strip_one_trailing_separator("dir//")).unwrap_err(),
            PathSafetyError::EmptyComponent
        );

        // Negative: `dir/./` strips one separator, leaving `dir/.` which has a
        // `.` component — still rejected.
        assert_eq!(strip_one_trailing_separator("dir/./"), "dir/.");
        assert_eq!(
            resolve(strip_one_trailing_separator("dir/./")).unwrap_err(),
            PathSafetyError::CurrentDirComponent
        );

        // A file entry with no trailing separator is untouched.
        assert_eq!(strip_one_trailing_separator("file.txt"), "file.txt");
    }

    /// G-6 (R7): a pre-existing reparse point (directory symlink) anywhere on
    /// the path from root to target is detected and refused before any write.
    ///
    /// Creating a directory symlink needs developer mode or elevation on
    /// Windows; when the environment does not permit it the test is skipped
    /// (not failed) rather than reporting a false positive.
    #[cfg(windows)]
    #[test]
    fn g6_check_no_reparse_within_detects_directory_symlink() {
        use std::os::windows::fs::symlink_dir;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();

        // A real directory symlink at `root/linked` pointing outside the root.
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let linked = root.join("linked");
        if let Err(_e) = symlink_dir(&outside, &linked) {
            eprintln!("G-6 symlink test skipped: cannot create directory symlink (needs dev mode/elevation)");
            return;
        }

        // A path that traverses the symlink must be refused.
        let through_link = root.join("linked").join("nested.txt");
        let err = check_no_reparse_within(&root, &through_link).unwrap_err();
        assert!(
            err.to_string().contains("reparse point"),
            "expected reparse-point refusal, got: {err}"
        );

        // A clean path (no symlink) is accepted.
        let clean = root.join("plain").join("file.txt");
        assert!(check_no_reparse_within(&root, &clean).is_ok());
    }

    /// P1 (v3): directories created as junctions (mount points) are reparse
    /// points too. On Windows `FileType::is_symlink` treats their reparse tag
    /// as a name surrogate, so the guard must refuse a path below one.
    #[cfg(windows)]
    #[test]
    fn junction_is_treated_as_a_reparse_point() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let junction = root.join("junction");
        // `mklink /J` needs no elevation, unlike a real directory symlink.
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if !status.is_ok_and(|status| status.success()) {
            eprintln!("junction test skipped: mklink /J unavailable");
            return;
        }
        assert!(
            std::fs::symlink_metadata(&junction)
                .unwrap()
                .file_type()
                .is_symlink(),
            "a junction must classify as a reparse point"
        );
        let err = check_no_reparse_within(&root, &junction.join("nested.txt")).unwrap_err();
        assert!(err.to_string().contains("reparse point"), "{err}");
    }

    /// P1 (v3): a path that lexically escapes the root (an untrusted journal
    /// could carry `..`) must be refused instead of silently being treated as
    /// a clean path below the root.
    #[test]
    fn escaping_paths_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();

        let parent = root.join("..").join("escape.txt");
        let err = check_no_reparse_within(&root, &parent).unwrap_err();
        assert!(err.to_string().contains("non-normal component"), "{err}");

        let outside = dir.path().join("outside.txt");
        let err = check_no_reparse_within(&root, &outside).unwrap_err();
        assert!(
            err.to_string().contains("outside the install root"),
            "{err}"
        );
    }
}
