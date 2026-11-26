//! Archive extraction path-safety tests.
//!
//! Covers the spec test matrix (`08_TEST_STRATEGY_AND_CASES.md` §3) for the
//! VSIX/ZIP and CAB extractors: path traversal, absolute/UNC/verbatim paths,
//! alternate data streams, reserved device names, trailing dot/space
//! collisions, entry collisions, file/directory conflicts, symlink entries
//! and benign filenames that must keep working.
//!
//! Every rejection test asserts that extraction fails AND that nothing was
//! written outside (or inside) the extraction root: the target tree must be
//! byte-identical before and after, and the escape sibling must not exist.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Build a ZIP archive with raw (unsanitized) entry names.
fn make_zip(path: &Path, entries: &[ZipEntry]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    for entry in entries {
        match entry.kind {
            EntryKind::File => {
                let mut options =
                    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
                if let Some(mode) = entry.unix_mode {
                    options = options.unix_permissions(mode);
                }
                zip.start_file(entry.name, options).unwrap();
                zip.write_all(entry.contents).unwrap();
            }
            EntryKind::Dir => {
                zip.add_directory(entry.name, SimpleFileOptions::default())
                    .unwrap();
            }
            // The writer's unix_permissions() drops the file-type bits, so a
            // real symlink entry must go through add_symlink (which forces a
            // Unix system field and preserves the S_IFLNK bit).
            EntryKind::Symlink => {
                zip.add_symlink(
                    entry.name,
                    std::str::from_utf8(entry.contents).unwrap(),
                    SimpleFileOptions::default(),
                )
                .unwrap();
            }
        }
    }
    zip.finish().unwrap();
}

fn make_cab(path: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut builder = cab::CabinetBuilder::new();
    {
        let folder = builder.add_folder(cab::CompressionType::None);
        for (name, _) in entries {
            folder.add_file(*name);
        }
    }
    let mut writer = builder.build(file).unwrap();
    for (name, contents) in entries {
        while let Some(mut fw) = writer.next_file().unwrap() {
            if fw.file_name() == *name {
                fw.write_all(contents).unwrap();
                break;
            }
        }
    }
    writer.finish().unwrap();
}

enum EntryKind {
    File,
    Dir,
    Symlink,
}

struct ZipEntry {
    name: &'static str,
    contents: &'static [u8],
    kind: EntryKind,
    unix_mode: Option<u32>,
}

fn file_entry(name: &'static str, contents: &'static [u8]) -> ZipEntry {
    ZipEntry {
        name,
        contents,
        kind: EntryKind::File,
        unix_mode: None,
    }
}

/// Recursively list a directory tree as sorted relative paths for
/// zero-write assertions.
fn tree(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                out.push(format!("{rel}/"));
                walk(&path, root, out);
            } else {
                out.push(rel);
            }
        }
    }
    walk(root, root, &mut out);
    out.sort();
    out
}

/// Holds the tempdir guard so fixtures stay alive for the whole test.
struct Fixture {
    _guard: tempfile::TempDir,
    archive: PathBuf,
    target: PathBuf,
    root: PathBuf,
}

impl Fixture {
    /// New fixture with `payload/` (for the archive) and an absent `target/`.
    fn new(tag: &str) -> Self {
        let guard = tempfile::tempdir().unwrap();
        let root = guard.path().to_path_buf();
        let payload_dir = root.join("payload");
        fs::create_dir_all(&payload_dir).unwrap();
        Self {
            _guard: guard,
            archive: payload_dir.join(tag),
            target: root.join("target"),
            root,
        }
    }
}

fn assert_extraction_rejected(fx: &Fixture, escape_names: &[PathBuf]) {
    let before = tree(&fx.target);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(msvc_kit::installer::extract_vsix(&fx.archive, &fx.target));
    assert!(
        result.is_err(),
        "expected extraction of {:?} to be rejected",
        fx.archive
    );
    let after = tree(&fx.target);
    assert_eq!(
        before, after,
        "target tree changed for rejected archive {:?}",
        fx.archive
    );
    for escape in escape_names {
        assert!(
            !escape.exists(),
            "escape file {escape:?} must not exist after rejected extraction"
        );
    }
}

fn extract_zip_ok(fx: &Fixture) {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(msvc_kit::installer::extract_vsix(&fx.archive, &fx.target))
        .unwrap();
}

// ---------- traversal and absolute paths (must reject with zero writes) ----

#[test]
fn zip_dotdot_traversal_is_rejected() {
    let fx = Fixture::new("traversal.vsix");
    make_zip(&fx.archive, &[file_entry("../escape.txt", b"evil")]);
    let escape = fx.root.join("escape.txt");
    assert_extraction_rejected(&fx, &[escape]);
}

#[test]
fn zip_nested_dotdot_traversal_is_rejected() {
    let fx = Fixture::new("nested.vsix");
    make_zip(&fx.archive, &[file_entry("a/../../escape.txt", b"evil")]);
    let escape = fx.root.join("escape.txt");
    assert_extraction_rejected(&fx, &[escape]);
}

#[test]
fn zip_backslash_traversal_is_rejected() {
    let fx = Fixture::new("backslash.vsix");
    make_zip(&fx.archive, &[file_entry("..\\escape.txt", b"evil")]);
    let escape = fx.root.join("escape.txt");
    assert_extraction_rejected(&fx, &[escape]);
}

#[test]
fn zip_mixed_backslash_traversal_is_rejected() {
    let fx = Fixture::new("mixed.vsix");
    make_zip(
        &fx.archive,
        &[file_entry("a\\..\\..\\outside.txt", b"evil")],
    );
    let escape = fx.root.join("outside.txt");
    assert_extraction_rejected(&fx, &[escape]);
}

#[test]
fn zip_posix_absolute_path_is_rejected() {
    let fx = Fixture::new("abs.vsix");
    make_zip(&fx.archive, &[file_entry("/absolute/file.txt", b"evil")]);
    assert_extraction_rejected(&fx, &[]);
}

#[test]
fn zip_windows_drive_absolute_paths_are_rejected() {
    for name in ["C:\\absolute\\file.txt", "C:/absolute/file.txt"] {
        let fx = Fixture::new("drive.vsix");
        make_zip(&fx.archive, &[file_entry(name, b"evil")]);
        assert_extraction_rejected(&fx, &[]);
    }
}

#[test]
fn zip_unc_and_verbatim_paths_are_rejected() {
    for name in ["\\\\server\\share\\file.txt", "\\\\?\\C:\\outside.txt"] {
        let fx = Fixture::new("unc.vsix");
        make_zip(&fx.archive, &[file_entry(name, b"evil")]);
        assert_extraction_rejected(&fx, &[]);
    }
}

// ---------- ADS, reserved names, trailing dot/space, dot components -------

#[test]
fn zip_alternate_data_stream_name_is_rejected() {
    let fx = Fixture::new("ads.vsix");
    make_zip(&fx.archive, &[file_entry("file.txt:payload", b"evil")]);
    assert_extraction_rejected(&fx, &[]);
}

#[test]
fn zip_reserved_device_names_are_rejected() {
    let fx = Fixture::new("reserved.vsix");
    make_zip(
        &fx.archive,
        &[
            file_entry("benign.txt", b"ok"),
            file_entry("CON", b"evil"),
            file_entry("NUL.txt", b"evil"),
            file_entry("AUX", b"evil"),
            file_entry("COM1.log", b"evil"),
            file_entry("LPT9", b"evil"),
        ],
    );
    assert_extraction_rejected(&fx, &[]);
}

#[test]
fn zip_trailing_dot_and_space_are_rejected() {
    let fx = Fixture::new("trailing.vsix");
    make_zip(
        &fx.archive,
        &[file_entry("foo.", b"evil"), file_entry("bar ", b"evil")],
    );
    assert_extraction_rejected(&fx, &[]);
}

#[test]
fn zip_dot_component_and_double_separator_are_rejected() {
    let fx = Fixture::new("dot.vsix");
    make_zip(
        &fx.archive,
        &[file_entry("a//b.txt", b"evil"), file_entry(".", b"evil")],
    );
    assert_extraction_rejected(&fx, &[]);
}

// ---------- collisions and structural conflicts ---------------------------

#[test]
fn zip_case_folded_duplicate_paths_with_different_bytes_are_rejected() {
    let fx = Fixture::new("case.vsix");
    make_zip(
        &fx.archive,
        &[file_entry("A.txt", b"one"), file_entry("a.txt", b"two")],
    );
    assert_extraction_rejected(&fx, &[]);
}

#[test]
fn zip_unicode_normalization_duplicate_paths_with_different_bytes_are_rejected() {
    // NFC (U+00E9) and NFD (e + U+0301) spellings of the same name.
    let fx = Fixture::new("nfc.vsix");
    make_zip(
        &fx.archive,
        &[
            file_entry("caf\u{e9}.txt", b"composed"),
            file_entry("cafe\u{301}.txt", b"decomposed"),
        ],
    );
    assert_extraction_rejected(&fx, &[]);
}

#[test]
fn zip_file_directory_conflict_is_rejected() {
    let fx = Fixture::new("conflict.vsix");
    make_zip(
        &fx.archive,
        &[file_entry("a", b"file"), file_entry("a/b.txt", b"nested")],
    );
    assert_extraction_rejected(&fx, &[]);
}

#[test]
fn zip_symlink_entries_are_rejected() {
    let fx = Fixture::new("symlink.vsix");
    make_zip(
        &fx.archive,
        &[
            file_entry("benign.txt", b"ok"),
            ZipEntry {
                name: "link.txt",
                contents: b"../outside.txt",
                kind: EntryKind::Symlink,
                unix_mode: None,
            },
        ],
    );
    let escape = fx.root.join("outside.txt");
    assert_extraction_rejected(&fx, &[escape]);
}

// ---------- benign archives must keep working ------------------------------

#[test]
fn zip_unicode_spaces_and_deep_paths_extract() {
    let fx = Fixture::new("benign.vsix");
    make_zip(
        &fx.archive,
        &[file_entry("中文 目录/深 层/file name.txt", b"content")],
    );
    extract_zip_ok(&fx);
    let out = fx
        .target
        .join("中文 目录")
        .join("深 层")
        .join("file name.txt");
    assert_eq!(fs::read(&out).unwrap(), b"content");
}

#[test]
fn cab_duplicate_entry_names_are_rejected_deterministically() {
    // Byte-identical duplicate names are refused by the zip reader itself; the
    // CAB reader resolves entries by name, so msvc-kit rejects duplicates
    // during planning instead of guessing which entry was meant.
    let fx = Fixture::new("dup.cab");
    fs::create_dir_all(&fx.target).unwrap();
    make_cab(&fx.archive, &[("dup.txt", b"same"), ("dup.txt", b"same")]);
    let before = tree(&fx.target);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(msvc_kit::installer::extract_cab(&fx.archive, &fx.target));
    assert!(
        result.is_err(),
        "duplicate CAB entry names must not extract"
    );
    assert_eq!(before, tree(&fx.target));
}

#[test]
fn zip_contents_prefix_stripping_is_preserved() {
    let fx = Fixture::new("contents.vsix");
    make_zip(&fx.archive, &[file_entry("Contents/VC/Tools/x.txt", b"c")]);
    extract_zip_ok(&fx);
    assert!(fx.target.join("VC").join("Tools").join("x.txt").is_file());
}

#[test]
fn zip_empty_directory_entries_are_created() {
    // Defined policy: explicit directory entries are created even when empty.
    let fx = Fixture::new("emptydir.vsix");
    make_zip(
        &fx.archive,
        &[ZipEntry {
            name: "only_dir/",
            contents: b"",
            kind: EntryKind::Dir,
            unix_mode: None,
        }],
    );
    extract_zip_ok(&fx);
    assert!(fx.target.join("only_dir").is_dir());
    assert_eq!(tree(&fx.target), vec!["only_dir/".to_string()]);
}

// ---------- CAB extractor shares the same guarantees -----------------------

#[test]
fn cab_dotdot_traversal_is_rejected() {
    let fx = Fixture::new("traverse.cab");
    fs::create_dir_all(&fx.target).unwrap();
    make_cab(&fx.archive, &[("..\\cab_escape.txt", b"evil")]);
    let escape = fx.root.join("cab_escape.txt");
    let before = tree(&fx.target);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(msvc_kit::installer::extract_cab(&fx.archive, &fx.target));
    assert!(
        result.is_err(),
        "expected CAB traversal extraction to be rejected"
    );
    assert_eq!(
        before,
        tree(&fx.target),
        "target tree changed for rejected CAB"
    );
    assert!(
        !escape.exists(),
        "CAB traversal escaped the extraction root"
    );
}

#[test]
fn cab_reserved_and_ads_names_are_rejected() {
    let fx = Fixture::new("reserved.cab");
    fs::create_dir_all(&fx.target).unwrap();
    make_cab(
        &fx.archive,
        &[("ok.txt", b"a"), ("CON", b"b"), ("stream.txt:ads", b"c")],
    );
    let before = tree(&fx.target);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(msvc_kit::installer::extract_cab(&fx.archive, &fx.target));
    assert!(
        result.is_err(),
        "expected CAB reserved/ADS extraction to be rejected"
    );
    assert_eq!(before, tree(&fx.target));
}

#[test]
fn cab_benign_extraction_still_works() {
    let fx = Fixture::new("benign.cab");
    fs::create_dir_all(&fx.target).unwrap();
    make_cab(&fx.archive, &[("dir/file.txt", b"payload")]);
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(msvc_kit::installer::extract_cab(&fx.archive, &fx.target))
        .unwrap();
    assert_eq!(
        fs::read(fx.target.join("dir").join("file.txt")).unwrap(),
        b"payload"
    );
}

// ---------- extraction markers carry content identity (v2) -----------------

#[tokio::test]
async fn extraction_markers_record_source_sha256() {
    let fx = Fixture::new("payload.vsix");
    make_zip(&fx.archive, &[file_entry("a.txt", b"one")]);

    msvc_kit::installer::extract_packages_with_progress(
        std::slice::from_ref(&fx.archive),
        &fx.target,
        "test",
    )
    .await
    .unwrap();

    let marker_dir = fx.target.join(".msvc-kit-extracted");
    let markers: Vec<_> = fs::read_dir(&marker_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(markers.len(), 1, "exactly one v2 marker expected");
    let bytes = fs::read(markers[0].path()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value.get("schema").and_then(|v| v.as_str()),
        Some("msvc-kit.extraction-marker.v2"),
        "marker must identify its schema"
    );
    assert!(
        value
            .get("source_sha256")
            .and_then(|v| v.as_str())
            .is_some_and(|sha| sha.len() == 64),
        "marker must carry the source archive SHA256"
    );
}

#[tokio::test]
async fn legacy_ok_markers_are_ignored_and_upgraded() {
    let fx = Fixture::new("legacy.vsix");
    make_zip(&fx.archive, &[file_entry("legacy.txt", b"x")]);

    // Forge a legacy v1 marker exactly like the pre-v2 extractor wrote it.
    let marker_dir = fx.target.join(".msvc-kit-extracted");
    fs::create_dir_all(&marker_dir).unwrap();
    let metadata = fs::metadata(&fx.archive).unwrap();
    let modified = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest as _;
    hasher.update(fx.archive.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(metadata.len().to_le_bytes());
    hasher.update(b"\0");
    hasher.update(modified.to_le_bytes());
    let fingerprint = hex::encode(hasher.finalize());
    let legacy = marker_dir.join(format!("legacy.vsix-{}.done", &fingerprint[..16]));
    fs::write(&legacy, b"ok").unwrap();

    msvc_kit::installer::extract_packages_with_progress(
        std::slice::from_ref(&fx.archive),
        &fx.target,
        "test",
    )
    .await
    .unwrap();

    assert!(
        fx.target.join("legacy.txt").is_file(),
        "legacy marker must not skip extraction"
    );
    assert!(
        legacy.is_file(),
        "legacy marker itself must remain untouched"
    );
    let v2_count = fs::read_dir(&marker_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("legacy.vsix-") && name.contains("-v2-")
        })
        .count();
    assert_eq!(
        v2_count, 1,
        "a v2 marker must be written after re-extraction"
    );
}
