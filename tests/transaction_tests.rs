//! Install transaction fault-injection and recovery tests (spec 08 §10,
//! spec 05 §8).
//!
//! Every fault point is exercised against a real temp root: the transaction
//! aborts at the injected point, `recover_pending` resolves the journal, and
//! the assertions check the invariant that matters for that point — the root
//! either never changed or was rolled back to the original bytes, unowned
//! files are untouched, and a transaction that passed its Committed marker is
//! kept while only its backup directory is cleaned.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use msvc_kit::installer::transaction::{self, CommittedInstall, FaultHook, FaultPoint};
use sha2::{Digest, Sha256};

const PKG: &str = "Microsoft.VC.14.44.Tools.HostX64.TargetX64";
const REL_FILE: &str = "VC/Tools/plan/file.txt";

fn sha(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    for (name, contents) in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file(name, options).unwrap();
        use std::io::Write as _;
        zip.write_all(contents).unwrap();
    }
    zip.finish().unwrap();
}

struct Fixture {
    _guard: tempfile::TempDir,
    root: PathBuf,
    payload: PathBuf,
}

/// A real historical receipt (schema `msvc-kit.installation-receipt.v1`:
/// component/version/arch/host_arch/components/payloads). Used as legacy
/// *root-level* adoption evidence; it carries no installed paths.
fn legacy_receipt_json() -> Vec<u8> {
    serde_json::json!({
        "schema": "msvc-kit.installation-receipt.v1",
        "component": "msvc",
        "version": "14.44.35207",
        "arch": "x64",
        "host_arch": "x64",
        "vs_channel": null,
        "components": [],
        "payloads": []
    })
    .to_string()
    .into_bytes()
}

impl Fixture {
    fn new(entries: &[(&str, &[u8])]) -> Self {
        let guard = tempfile::tempdir().unwrap();
        let root = guard.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        let payload = guard.path().join("payload.vsix");
        make_zip(&payload, entries);
        Self {
            _guard: guard,
            root,
            payload,
        }
    }

    /// Legacy evidence: receipts directory + an existing (different) file so
    /// the commit takes the backup/replace path.
    fn make_legacy_with_conflicting_file(&self, rel: &str, contents: &[u8]) {
        let receipts = self.root.join(".msvc-kit").join("receipts");
        std::fs::create_dir_all(&receipts).unwrap();
        std::fs::write(
            receipts.join("msvc-14.44.35207-x64-x64.json"),
            legacy_receipt_json(),
        )
        .unwrap();
        let target = self.root.join(rel);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
}

/// Run a commit that dies at the injected fault WITHOUT rolling back,
/// simulating a process crash: recovery must resolve the journal afterwards.
async fn run_commit(
    root: &Path,
    payload: &Path,
    faults: FaultHook,
) -> std::result::Result<CommittedInstall, msvc_kit::MsvcKitError> {
    let mut tx = transaction::Transaction::begin(root, "test-plan", faults)?;
    let staged = tx
        .stage_package(PKG, &[payload.to_path_buf()], &HashMap::new(), |_| true)
        .await?;
    let merged = tx.merge(std::slice::from_ref(&staged))?;
    match tx.commit(&merged).await {
        Ok(outcome) => Err(msvc_kit::MsvcKitError::Other(format!(
            "armed fault did not fire; commit succeeded: {:?}",
            outcome
        ))),
        Err(error) => Err(error),
    }
}

fn root_tree(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                // Normalize .msvc-kit volatile files out of the comparison.
                if rel.starts_with(".msvc-kit/transactions/")
                    || rel == ".msvc-kit/installation.lock"
                {
                    continue;
                }
                out.push((rel, sha(&bytes)));
            }
        }
    }
    walk(root, root, &mut out);
    out.sort();
    out
}

#[tokio::test]
async fn commit_fresh_root_writes_files_ownership_and_manifest() {
    let fx = Fixture::new(&[(REL_FILE, b"content-a")]);
    let outcome = transaction::recover_pending(&fx.root).await.unwrap();
    assert_eq!(outcome.transactions_examined, 0);

    let mut tx = transaction::Transaction::begin(&fx.root, "plan-1", FaultHook::none()).unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    assert_eq!(staged.files.len(), 1);
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    let outcome = tx.commit(&merged).await.unwrap();

    assert_eq!(outcome.created, 1);
    assert_eq!(outcome.replaced, 0);
    assert_eq!(outcome.skipped, 0);
    assert_eq!(outcome.written, vec![REL_FILE.to_string()]);

    let installed = fx.root.join(REL_FILE);
    assert_eq!(std::fs::read(&installed).unwrap(), b"content-a");

    // Ownership index records the file with its digest and owner.
    let ownership: serde_json::Value =
        serde_json::from_slice(&std::fs::read(transaction::ownership_path(&fx.root)).unwrap())
            .unwrap();
    assert_eq!(
        ownership["entries"][0]["path"].as_str(),
        Some(REL_FILE.to_lowercase().as_str())
    );
    assert_eq!(ownership["entries"][0]["owners"][0].as_str(), Some(PKG));

    // Installed manifest for this install id exists and matches.
    let tx_id = outcome.transaction_id.clone();
    let manifest_path = fx
        .root
        .join(".msvc-kit")
        .join("installed-manifests")
        .join(format!("{}.json", tx_id));
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["files"][0]["path"].as_str(), Some(REL_FILE));

    // Journal carries the Committed event; backup dir is cleaned.
    assert!(transaction::journal_has_committed(
        &fx.root
            .join(".msvc-kit")
            .join("transactions")
            .join(tx_id.as_str())
            .join("journal.jsonl")
    ));
    assert!(!fx
        .root
        .join(".msvc-kit")
        .join("transactions")
        .join(tx_id.as_str())
        .join("backup")
        .exists());
}

#[tokio::test]
async fn reinstall_identical_files_skips_writes_and_adds_owner() {
    let fx = Fixture::new(&[(REL_FILE, b"same")]);
    let mut tx = transaction::Transaction::begin(&fx.root, "plan-1", FaultHook::none()).unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    tx.commit(&merged).await.unwrap();

    // Same content, different package id: identical bytes are skipped and the
    // new owner is appended without rewriting the file.
    let mut tx = transaction::Transaction::begin(&fx.root, "plan-2", FaultHook::none()).unwrap();
    let staged = tx
        .stage_package(
            "other.package",
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    let outcome = tx.commit(&merged).await.unwrap();
    assert_eq!(outcome.created, 0);
    assert_eq!(outcome.replaced, 0);
    assert_eq!(outcome.skipped, 1);

    let ownership: serde_json::Value =
        serde_json::from_slice(&std::fs::read(transaction::ownership_path(&fx.root)).unwrap())
            .unwrap();
    let owners: Vec<&str> = ownership["entries"][0]["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert!(owners.contains(&PKG) && owners.contains(&"other.package"));
    assert_eq!(std::fs::read(fx.root.join(REL_FILE)).unwrap(), b"same");
}

#[tokio::test]
async fn replace_owned_file_backs_up_then_installs() {
    let fx = Fixture::new(&[(REL_FILE, b"content-b")]);
    fx.make_legacy_with_conflicting_file(REL_FILE, b"old-bytes");
    let before = root_tree(&fx.root);

    let mut tx = transaction::Transaction::begin(&fx.root, "plan-1", FaultHook::none()).unwrap();
    let tx_id = tx.transaction_id().to_string();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    let outcome = tx.commit(&merged).await.unwrap();
    assert_eq!(outcome.replaced, 1);
    assert_eq!(std::fs::read(fx.root.join(REL_FILE)).unwrap(), b"content-b");
    let _ = tx_id;

    // The pre-existing bytes were backed up inside the transaction workspace;
    // after a clean commit the backup is gone but the journal recorded both.
    let journal = std::fs::read_to_string(
        fx.root
            .join(".msvc-kit")
            .join("transactions")
            .join(tx_id.as_str())
            .join("journal.jsonl"),
    )
    .unwrap();
    assert!(journal.contains("will_backup"));
    assert!(journal.contains("backup_done"));
    assert!(journal.contains("install_done"));
    assert!(journal.contains("committed"));
    let _ = before;
}

#[tokio::test]
async fn unowned_existing_files_are_never_overwritten() {
    let fx = Fixture::new(&[(REL_FILE, b"planned")]);
    // Foreign file at the target path, no msvc-kit management evidence.
    let target = fx.root.join(REL_FILE);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"user-owned").unwrap();
    let before = root_tree(&fx.root);

    let error = run_commit(&fx.root, &fx.payload, FaultHook::none())
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("refusing to install over unowned files"),
        "{error}"
    );
    assert_eq!(before, root_tree(&fx.root), "root must be untouched");
    assert_eq!(std::fs::read(&target).unwrap(), b"user-owned");
}

#[tokio::test]
async fn two_versions_coexist_in_one_root() {
    let fx = Fixture::new(&[("VC/Tools/MSVC/14.44/file.txt", b"v14.44")]);
    let mut tx = transaction::Transaction::begin(&fx.root, "plan-a", FaultHook::none()).unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    tx.commit(&merged).await.unwrap();

    // Second payload at a disjoint version path.
    let payload_b = fx._guard.path().join("payload-b.vsix");
    make_zip(&payload_b, &[("VC/Tools/MSVC/14.45/file.txt", b"v14.45")]);
    let mut tx = transaction::Transaction::begin(&fx.root, "plan-b", FaultHook::none()).unwrap();
    let staged = tx
        .stage_package("pkg-b", &[payload_b], &HashMap::new(), |_| true)
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    tx.commit(&merged).await.unwrap();

    assert_eq!(
        std::fs::read(fx.root.join("VC/Tools/MSVC/14.44/file.txt")).unwrap(),
        b"v14.44"
    );
    assert_eq!(
        std::fs::read(fx.root.join("VC/Tools/MSVC/14.45/file.txt")).unwrap(),
        b"v14.45"
    );
}

/// The fault matrix: every injection point aborts the transaction and
/// recovery leaves the root in a safe state (pre-commit points: original
/// bytes restored; post-commit points: install kept, backup cleaned).
#[tokio::test]
async fn fault_injection_matrix_recovers_to_safe_states() {
    let pre_commit_points = [
        FaultPoint::AfterWorkspaceCreated,
        FaultPoint::AfterJournalHeader,
        FaultPoint::AfterPackageStaged,
        FaultPoint::AfterMerge,
        FaultPoint::BeforeInstall,
        FaultPoint::AfterInstall,
        FaultPoint::BeforeOwnershipWrite,
        FaultPoint::BeforeInstalledManifestWrite,
        FaultPoint::BeforeCommittedMarker,
    ];
    let post_commit_points = [FaultPoint::AfterCommittedMarker, FaultPoint::BeforeCleanup];

    for point in pre_commit_points {
        let fx = Fixture::new(&[(REL_FILE, b"content")]);
        let result = run_commit(&fx.root, &fx.payload, FaultHook::with_points(&[point])).await;
        assert!(result.is_err(), "{point:?} must fail the commit");

        let report = transaction::recover_pending(&fx.root).await.unwrap();
        assert!(
            report.rolled_back.len() + report.cleaned_staging.len() >= 1,
            "{point:?}: recovery must resolve the transaction ({report:?})"
        );

        // The root must not contain the planned file (rolled back or never
        // written), and a second recovery is a no-op.
        assert!(
            !fx.root.join(REL_FILE).exists(),
            "{point:?}: installed file must be rolled back"
        );
        let report = transaction::recover_pending(&fx.root).await.unwrap();
        assert_eq!(report.transactions_examined, 0, "{point:?} idempotent");
    }

    for point in post_commit_points {
        let fx = Fixture::new(&[(REL_FILE, b"content")]);
        let result = run_commit(&fx.root, &fx.payload, FaultHook::with_points(&[point])).await;
        assert!(result.is_err(), "{point:?} must surface the injected fault");

        let report = transaction::recover_pending(&fx.root).await.unwrap();
        assert!(
            !report.needs_recovery.is_empty() || !report.cleaned_backups.is_empty(),
            "{point:?}: recovery must acknowledge the committed transaction ({report:?})"
        );
        // The install is durable: the file stays, and a second recovery only
        // re-examines the retained committed journal (by design).
        assert_eq!(std::fs::read(fx.root.join(REL_FILE)).unwrap(), b"content");
        let report = transaction::recover_pending(&fx.root).await.unwrap();
        assert!(report.needs_recovery.is_empty(), "{report:?}");
    }
}

/// Backup-path faults need a replace scenario (fresh roots never back up).
#[tokio::test]
async fn backup_fault_points_roll_back_to_original_bytes() {
    for point in [FaultPoint::BeforeBackup, FaultPoint::AfterBackup] {
        let fx = Fixture::new(&[(REL_FILE, b"new-bytes")]);
        fx.make_legacy_with_conflicting_file(REL_FILE, b"original-bytes");

        let result = run_commit(&fx.root, &fx.payload, FaultHook::with_points(&[point])).await;
        assert!(result.is_err(), "{point:?} must fail the commit");

        transaction::recover_pending(&fx.root).await.unwrap();
        assert_eq!(
            std::fs::read(fx.root.join(REL_FILE)).unwrap(),
            b"original-bytes",
            "{point:?}: rollback must restore the original file"
        );
        assert!(
            transaction::recover_pending(&fx.root)
                .await
                .unwrap()
                .transactions_examined
                == 0,
            "{point:?} recovery idempotent"
        );
    }
}

#[tokio::test]
async fn needs_recovery_blocks_new_transactions_until_resolved() {
    let fx = Fixture::new(&[(REL_FILE, b"content")]);
    let tx_root = fx.root.join(".msvc-kit").join("transactions");
    let tx_dir = tx_root.join("stuck-tx");
    std::fs::create_dir_all(&tx_dir).unwrap();
    std::fs::write(tx_dir.join("needs-recovery.json"), br#"{"reason":"test"}"#).unwrap();

    let error = transaction::Transaction::begin(&fx.root, "plan", FaultHook::none()).unwrap_err();
    assert!(error.to_string().contains("needs_recovery"), "{error}");

    let report = transaction::recover_pending(&fx.root).await.unwrap();
    assert_eq!(report.needs_recovery, vec!["stuck-tx".to_string()]);
    assert!(
        tx_dir.join("needs-recovery.json").exists(),
        "recovery must not touch needs-recovery transactions"
    );

    // Removing the marker (a user decision) unblocks new transactions.
    std::fs::remove_file(tx_dir.join("needs-recovery.json")).unwrap();
    transaction::Transaction::begin(&fx.root, "plan", FaultHook::none()).unwrap();
}

#[tokio::test]
async fn explicit_rollback_restores_original_bytes_and_marks_journal() {
    let fx = Fixture::new(&[(REL_FILE, b"new-bytes")]);
    fx.make_legacy_with_conflicting_file(REL_FILE, b"original-bytes");

    // The wiring path: commit fails at an injected point, the caller calls
    // rollback explicitly, and the original bytes come back.
    let mut tx = transaction::Transaction::begin(
        &fx.root,
        "plan",
        FaultHook::with_points(&[FaultPoint::AfterInstall]),
    )
    .unwrap();
    let tx_id = tx.transaction_id().to_string();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    let error = tx.commit(&merged).await.unwrap_err();
    tx.rollback(&error.to_string()).await.unwrap();

    assert_eq!(
        std::fs::read(fx.root.join(REL_FILE)).unwrap(),
        b"original-bytes"
    );
    let journal = std::fs::read_to_string(
        fx.root
            .join(".msvc-kit")
            .join("transactions")
            .join(tx_id.as_str())
            .join("journal.jsonl"),
    )
    .unwrap();
    assert!(journal.contains("rolled_back"), "{journal}");

    // Recovery sees the rolled-back transaction and cleans the workspace.
    let report = transaction::recover_pending(&fx.root).await.unwrap();
    assert_eq!(report.cleaned_staging, vec![tx_id]);
}
// ---------- G-5: Real process crash recovery (spec revision R4) ------------
//
// Spawns a helper child process (same test binary) that runs a transaction
// to the AfterInstall fault point and then calls std::process::abort() —
// terminating immediately without running Drop, simulating a kill at that
// journal boundary. The parent process verifies the journal state matches
// the crash point and calls recovery to restore the original bytes. This is
// a real cross-process test: the child's file handles are closed by the OS.

async fn crash_helper_logic(root: &Path, payload: &Path) -> ! {
    let mut tx = transaction::Transaction::begin(
        root,
        "crash-test",
        transaction::FaultHook::with_points(&[transaction::FaultPoint::AfterInstall]),
    )
    .unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&payload.to_path_buf()),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    // The fault hook returns Err (AfterInstall fires). We do NOT call
    // rollback — the child aborts here, simulating a process kill.
    let _ = tx.commit(&merged).await.unwrap_err();
    std::process::abort();
}

#[tokio::test]
async fn real_process_crash_recovery_restores_original_bytes() {
    let root_env = std::env::var("MSVC_KIT_CRASH_TEST_ROOT");
    let payload_env = std::env::var("MSVC_KIT_CRASH_TEST_PAYLOAD");

    // Child mode: re-entered with the fixture paths from env vars.
    if let (Ok(root), Ok(payload)) = (&root_env, &payload_env) {
        let root = PathBuf::from(root);
        let payload = PathBuf::from(payload);
        crash_helper_logic(&root, &payload).await;
        // crash_helper_logic aborts and never returns.
    }

    // Parent mode: create fixture, spawn child, verify recovery.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let payload = dir.path().join("payload.vsix");
    make_zip(&payload, &[(REL_FILE, b"new-bytes")]);
    // Legacy root with a pre-existing file (for backup/replace path).
    let receipts = root.join(".msvc-kit").join("receipts");
    std::fs::create_dir_all(&receipts).unwrap();
    std::fs::write(receipts.join("legacy.json"), b"{}").unwrap();
    let target = root.join(REL_FILE);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"original-bytes").unwrap();

    // Spawn the crash helper child.
    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new(exe)
        .args([
            "--exact",
            "real_process_crash_recovery_restores_original_bytes",
            "--nocapture",
        ])
        .env("MSVC_KIT_CRASH_TEST_ROOT", root.to_str().unwrap())
        .env("MSVC_KIT_CRASH_TEST_PAYLOAD", payload.to_str().unwrap())
        .status()
        .expect("Failed to spawn crash helper child");
    assert!(
        !status.success(),
        "crash helper child should have aborted, got: {status:?}"
    );

    // The journal is in the "commit started but not completed" state:
    // backup_done + install_done exist (AfterInstall fired after install),
    // but ownership/committed events are missing. Recovery must roll back.
    let report = transaction::recover_pending(&root).await.unwrap();
    assert!(
        report.rolled_back.len() == 1 || report.cleaned_staging.len() == 1,
        "recovery must resolve the crashed transaction: {report:?}"
    );

    // The original bytes must be restored from backup.
    let restored = std::fs::read(&target).unwrap();
    assert_eq!(
        restored, b"original-bytes",
        "crash recovery must restore the original file from backup"
    );

    // Idempotent recovery.
    let report = transaction::recover_pending(&root).await.unwrap();
    assert_eq!(
        report.transactions_examined, 0,
        "recovery must be idempotent"
    );
}

// ============================================================================
// P1 regression tests: install-boundary safety, backup identity, fail-closed
// hashing.
// ============================================================================

/// Commit a single-package transaction to `root` and return the error (which
/// any reparse-point / ownership / hash violation must surface).
async fn commit_one(root: &Path, payload: &Path) -> msvc_kit::MsvcKitError {
    let mut tx = transaction::Transaction::begin(root, "plan", FaultHook::none()).unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&payload.to_path_buf()),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    tx.commit(&merged).await.unwrap_err()
}

/// P1: a junction/symlink planted under the install root must not redirect a
/// commit write outside the install boundary. Place a directory symlink at
/// `root/VC` pointing outside, then try to install `VC/escape.txt`.
#[cfg(windows)]
#[tokio::test]
async fn commit_rejects_junction_under_install_root() {
    use std::os::windows::fs as winfs;

    let fx = Fixture::new(&[("VC/escape.txt", b"escaped")]);
    // Outside dir that the junction targets.
    let outside = fx._guard.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let junction = fx.root.join("VC");
    std::fs::create_dir_all(&junction).unwrap();
    // Replace the real dir with a junction to the outside dir. If creation is
    // unsupported (no Developer Mode / admin), skip so the test stays green.
    std::fs::remove_dir(&junction).unwrap();
    if winfs::symlink_dir(&outside, &junction).is_err() {
        eprintln!("SKIP: cannot create directory symlink (privilege required)");
        return;
    }

    let error = commit_one(&fx.root, &fx.payload).await;
    // The write must be refused because the target path traverses the junction.
    assert!(
        error.to_string().contains("reparse point"),
        "commit must refuse to write through a junction under the root, got: {error}"
    );
    // Nothing may have leaked into the outside directory.
    let leaked = outside.join("escape.txt");
    assert!(
        !leaked.exists(),
        "commit wrote outside the install boundary through the junction"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn commit_rejects_symlink_under_install_root() {
    use std::os::unix::fs as unixfs;

    let fx = Fixture::new(&[("VC/escape.txt", b"escaped")]);
    let outside = fx._guard.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let link = fx.root.join("VC");
    std::fs::create_dir_all(&link).unwrap();
    std::fs::remove_dir(&link).unwrap();
    unixfs::symlink(&outside, &link).unwrap();

    let error = commit_one(&fx.root, &fx.payload).await;
    assert!(
        error.to_string().contains("reparse point"),
        "commit must refuse to write through a symlink under the root, got: {error}"
    );
    assert!(!outside.join("escape.txt").exists());
}

/// P1: two distinct paths that sanitize to the same backup suffix (`a/b` and
/// `a__b`, both ending in `__a__b`) must be restored independently on
/// rollback — the journal's precise backup identity must win over suffix
/// matching, so the files' contents are never crossed.
#[tokio::test]
async fn rollback_restores_colliding_backup_names_independently() {
    let fx = Fixture::new(&[("a/b", b"new-b"), ("a__b", b"new-double")]);
    fx.make_legacy_with_conflicting_file("a/b", b"orig-b");
    fx.make_legacy_with_conflicting_file("a__b", b"orig-double");

    // Commit a crash at BeforeOwnershipWrite -> caller triggers rollback; both
    // files were replaced (backed up) and must come back to their own
    // originals.
    let mut tx = transaction::Transaction::begin(
        &fx.root,
        "plan",
        FaultHook::with_points(&[FaultPoint::BeforeOwnershipWrite]),
    )
    .unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    let error = tx.commit(&merged).await.unwrap_err();
    tx.rollback(&error.to_string()).await.unwrap();

    assert_eq!(
        std::fs::read(fx.root.join("a/b")).unwrap(),
        b"orig-b",
        "a/b must be restored from its own backup, not a__b's"
    );
    assert_eq!(
        std::fs::read(fx.root.join("a__b")).unwrap(),
        b"orig-double",
        "a__b must be restored from its own backup, not a/b's"
    );
}

/// P1: an unreadable existing file must cause the transaction to fail, not be
/// silently treated as "does not exist" and overwritten. Only runnable where
/// permission bits are honored (Unix CI runs as a non-root user).
#[cfg(unix)]
#[tokio::test]
async fn unreadable_existing_file_fails_commit_instead_of_overwriting() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::new(&[(REL_FILE, b"planned-content")]);
    let target = fx.root.join(REL_FILE);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"secret-existing").unwrap();
    let mut perms = std::fs::metadata(&target).unwrap().permissions();
    perms.set_mode(0o000);
    std::fs::set_permissions(&target, perms).unwrap();

    // Guard: if we're running as root the chmod won't deny reads; skip to
    // avoid a false failure. (GitHub Actions Linux runners run as non-root.)
    if std::fs::read(&target).is_ok() {
        eprintln!("SKIP: running as root, read denial not effective");
        return;
    }

    let error = commit_one(&fx.root, &fx.payload).await;
    // Fail-closed: an unreadable pre-existing file must NOT be classified as
    // "absent" and silently overwritten. Previously `.ok()` turned the hash
    // read error into `Planned::Create`, letting the commit succeed. The fix
    // propagates the I/O error, so the commit must fail here.
    assert!(
        error.to_string().contains("Permission denied")
            || error.to_string().to_lowercase().contains("permission"),
        "unreadable existing file must fail the commit with an I/O error, got: {error}"
    );

    // The pre-existing file must be left untouched (no silent overwrite).
    std::fs::set_permissions(&target, PermissionsExt::from_mode(0o600)).unwrap();
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"secret-existing",
        "the unreadable existing file must not have been overwritten"
    );
}

/// P1 (G-4 / R3): an MSI payload without an official SHA256 must be refused
/// by the transaction staging step, not silently extracted. The digest is
/// what the extraction layer re-verifies right before the Windows Installer
/// runs; without it there is nothing to verify against.
#[tokio::test]
async fn msi_payload_without_official_digest_is_rejected() {
    let guard = tempfile::tempdir().unwrap();
    let root = guard.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    // A file that is named like an MSI but is not a real package: the
    // rejection must happen before any extraction is attempted.
    let msi = guard.path().join("payload.msi");
    std::fs::write(&msi, b"not a real msi").unwrap();

    let mut tx = transaction::Transaction::begin(&root, "plan", FaultHook::none()).unwrap();
    let error = tx
        .stage_package(PKG, std::slice::from_ref(&msi), &HashMap::new(), |_| true)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("no official SHA256"),
        "an MSI without an official digest must be rejected, got: {error}"
    );
    // No payload may have been extracted into the transaction's `packages`
    // directory (the journal file the transaction itself wrote is expected).
    let package_dir = root
        .join(".msvc-kit")
        .join("transactions")
        .join(tx.transaction_id())
        .join("packages");
    let staged_files: Vec<_> = walk_files(&package_dir);
    assert!(
        staged_files.is_empty(),
        "no payload may be staged without an official digest, found: {staged_files:?}"
    );
}

/// Collect every file under `dir` (missing dir = no files).
fn walk_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return out;
    }
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else {
            out.push(path.to_string_lossy().into_owned());
        }
    }
    out
}

/// P1 (G-4 / R3): an MSI payload WITH an official digest that does not match
/// the file on disk must abort the extraction instead of reaching the
/// installer.
#[tokio::test]
async fn msi_payload_with_wrong_digest_is_rejected() {
    let guard = tempfile::tempdir().unwrap();
    let root = guard.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let msi = guard.path().join("payload.msi");
    std::fs::write(&msi, b"not a real msi").unwrap();

    let mut digests = HashMap::new();
    digests.insert(msi.clone(), "0".repeat(64));

    let mut tx = transaction::Transaction::begin(&root, "plan", FaultHook::none()).unwrap();
    let error = tx
        .stage_package(PKG, std::slice::from_ref(&msi), &digests, |_| true)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("hash") || error.to_string().contains("Hash"),
        "a payload that fails the official digest check must abort, got: {error}"
    );
}

// ============================================================================
// v3 review fixes: recovery-path reparse guard and fail-closed rollback.
// ============================================================================

/// Create a directory reparse point at `link` targeting `target`. Windows
/// uses a junction (`mklink /J`, no elevation needed); Unix uses a symlink.
/// Returns false when the platform refuses to create it.
fn make_directory_link(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (link, target);
        false
    }
}

/// The single transaction directory created by the fixture's first commit.
fn single_transaction_dir(root: &Path) -> PathBuf {
    let tx_root = root.join(".msvc-kit").join("transactions");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&tx_root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one transaction");
    entries.remove(0)
}

/// Begin a transaction that fails at `point` and is NOT rolled back, leaving
/// the journal and the live handle for an explicit `rollback` call.
async fn begin_and_fail_commit(
    root: &Path,
    payload: &Path,
    point: FaultPoint,
) -> (transaction::Transaction, String, msvc_kit::MsvcKitError) {
    let mut tx =
        transaction::Transaction::begin(root, "plan", FaultHook::with_points(&[point])).unwrap();
    let tx_id = tx.transaction_id().to_string();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&payload.to_path_buf()),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    let error = tx.commit(&merged).await.unwrap_err();
    (tx, tx_id, error)
}

fn read_journal(root: &Path, tx_id: &str) -> String {
    std::fs::read_to_string(
        root.join(".msvc-kit")
            .join("transactions")
            .join(tx_id)
            .join("journal.jsonl"),
    )
    .unwrap()
}

/// P1 (v3): recovery runs after the crash and must re-check reparse points.
/// A junction/symlink planted under the install root in the meantime must not
/// redirect the restore outside the install boundary.
#[tokio::test]
async fn recovery_refuses_reparse_point_planted_after_crash() {
    let fx = Fixture::new(&[(REL_FILE, b"new-bytes")]);
    fx.make_legacy_with_conflicting_file(REL_FILE, b"original-bytes");

    // Crash-equivalent: commit stops before the ownership write, no rollback.
    let result = run_commit(
        &fx.root,
        &fx.payload,
        FaultHook::with_points(&[FaultPoint::BeforeOwnershipWrite]),
    )
    .await;
    assert!(result.is_err());
    let tx_dir = single_transaction_dir(&fx.root);
    let tx_id = tx_dir.file_name().unwrap().to_string_lossy().into_owned();

    // Replace the installed tree with a directory reparse point aimed outside
    // the root. The outside tree already holds a copy of the installed bytes,
    // so the pre-fix code would "restore" over it through the link.
    let outside = fx._guard.path().join("outside");
    let leaked = outside.join("Tools").join("plan").join("file.txt");
    std::fs::create_dir_all(leaked.parent().unwrap()).unwrap();
    std::fs::write(&leaked, b"new-bytes").unwrap();
    let vc = fx.root.join("VC");
    std::fs::remove_dir_all(&vc).unwrap();
    if !make_directory_link(&vc, &outside) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    let report = transaction::recover_pending(&fx.root).await.unwrap();
    assert!(report.rolled_back.is_empty(), "{report:?}");
    assert_eq!(report.needs_recovery, vec![tx_id.clone()], "{report:?}");
    assert_eq!(
        std::fs::read(&leaked).unwrap(),
        b"new-bytes",
        "recovery must not write through the reparse point"
    );
    assert!(tx_dir.join("needs-recovery.json").exists());
    assert!(
        !read_journal(&fx.root, &tx_id).contains("rolled_back"),
        "a refused rollback must not be journaled as successful"
    );
}

/// P2 (v3): a created file whose hash cannot be read must leave the
/// transaction in needs_recovery; it may not be recorded as rolled back.
#[cfg(unix)]
#[tokio::test]
async fn rollback_fails_closed_when_created_file_cannot_be_hashed() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::new(&[(REL_FILE, b"ours")]);
    let (tx, tx_id, _) =
        begin_and_fail_commit(&fx.root, &fx.payload, FaultPoint::BeforeOwnershipWrite).await;
    let target = fx.root.join(REL_FILE);
    assert_eq!(std::fs::read(&target).unwrap(), b"ours");

    let mut perms = std::fs::metadata(&target).unwrap().permissions();
    perms.set_mode(0o000);
    std::fs::set_permissions(&target, perms).unwrap();
    if std::fs::read(&target).is_ok() {
        // Running as root: the permission denial is not effective.
        std::fs::set_permissions(&target, PermissionsExt::from_mode(0o600)).unwrap();
        eprintln!("SKIP: running as root, read denial not effective");
        return;
    }

    let error = tx.rollback("injected failure").await.unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("permission")
            || error.to_string().to_lowercase().contains("denied"),
        "the hash read failure must surface, got: {error}"
    );

    std::fs::set_permissions(&target, PermissionsExt::from_mode(0o600)).unwrap();
    assert!(
        std::fs::read(&target).is_ok(),
        "an unreadable file must not be deleted on an unverified rollback"
    );
    let tx_dir = fx.root.join(".msvc-kit").join("transactions").join(&tx_id);
    assert!(tx_dir.join("needs-recovery.json").exists());
    assert!(
        !read_journal(&fx.root, &tx_id).contains("rolled_back"),
        "a failed rollback must not be journaled as successful"
    );
}

/// P2 (v3), Windows counterpart: hold the installed file open with no
/// sharing, so the rollback cannot read (or even stat) it. The transaction
/// must go to needs_recovery instead of being reported as rolled back.
#[cfg(windows)]
#[tokio::test]
async fn rollback_fails_closed_when_created_file_is_locked() {
    use std::os::windows::fs::OpenOptionsExt;

    let fx = Fixture::new(&[(REL_FILE, b"ours")]);
    let (tx, tx_id, _) =
        begin_and_fail_commit(&fx.root, &fx.payload, FaultPoint::BeforeOwnershipWrite).await;
    let target = fx.root.join(REL_FILE);
    assert_eq!(std::fs::read(&target).unwrap(), b"ours");

    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&target)
        .unwrap();

    let error = tx.rollback("injected failure").await.unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("another process")
            || error.to_string().to_lowercase().contains("sharing")
            || error.to_string().contains("IO error"),
        "the locked file must surface an I/O failure, got: {error}"
    );

    drop(locked);
    assert!(
        std::fs::read(&target).is_ok(),
        "an unverifiable file must not be deleted on an unverified rollback"
    );
    let tx_dir = fx.root.join(".msvc-kit").join("transactions").join(&tx_id);
    assert!(tx_dir.join("needs-recovery.json").exists());
    assert!(
        !read_journal(&fx.root, &tx_id).contains("rolled_back"),
        "a failed rollback must not be journaled as successful"
    );
}

/// P2 (v3): a created file modified by a third party after the failure is no
/// longer ours to delete; the rollback must go to needs_recovery instead of
/// deleting or silently ignoring it.
#[tokio::test]
async fn rollback_refuses_to_delete_created_file_modified_after_failure() {
    let fx = Fixture::new(&[(REL_FILE, b"ours")]);
    let (tx, tx_id, _) =
        begin_and_fail_commit(&fx.root, &fx.payload, FaultPoint::BeforeOwnershipWrite).await;
    let target = fx.root.join(REL_FILE);
    std::fs::write(&target, b"third-party").unwrap();

    let error = tx.rollback("injected failure").await.unwrap_err();
    assert!(error.to_string().contains("Rollback conflict"), "{error}");
    assert_eq!(std::fs::read(&target).unwrap(), b"third-party");
    let tx_dir = fx.root.join(".msvc-kit").join("transactions").join(&tx_id);
    assert!(tx_dir.join("needs-recovery.json").exists());
    assert!(!read_journal(&fx.root, &tx_id).contains("rolled_back"));
}

/// P2 (v3): when the ownership index did not exist before the transaction,
/// rollback must remove the index this transaction wrote; a removal failure
/// (not NotFound) is not a clean rollback.
#[tokio::test]
async fn rollback_fails_closed_when_ownership_index_cannot_be_removed() {
    let fx = Fixture::new(&[(REL_FILE, b"ours")]);
    let (tx, tx_id, _) = begin_and_fail_commit(
        &fx.root,
        &fx.payload,
        FaultPoint::BeforeInstalledManifestWrite,
    )
    .await;
    let index = transaction::ownership_path(&fx.root);
    assert!(
        index.is_file(),
        "the fault point follows the ownership-index write"
    );
    // Replace the index with a directory so remove_file must fail.
    std::fs::remove_file(&index).unwrap();
    std::fs::create_dir_all(&index).unwrap();

    let error = tx.rollback("injected failure").await.unwrap_err();
    assert!(error.to_string().contains("IO error"), "{error}");
    let tx_dir = fx.root.join(".msvc-kit").join("transactions").join(&tx_id);
    assert!(tx_dir.join("needs-recovery.json").exists());
    assert!(!read_journal(&fx.root, &tx_id).contains("rolled_back"));
}

/// P2 (v3): a journal that says the previous ownership index was backed up
/// must not silently skip the restore when that backup file is gone.
#[tokio::test]
async fn rollback_fails_closed_when_ownership_backup_is_missing() {
    let fx = Fixture::new(&[(REL_FILE, b"same")]);

    // First install creates the ownership index.
    let mut tx = transaction::Transaction::begin(&fx.root, "first", FaultHook::none()).unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    tx.commit(&merged).await.unwrap();
    assert!(transaction::ownership_path(&fx.root).exists());

    // Second install: the files are identical (skipped) but the ownership
    // index is backed up; fail after that write, then delete its backup.
    let (tx, tx_id, _) = begin_and_fail_commit(
        &fx.root,
        &fx.payload,
        FaultPoint::BeforeInstalledManifestWrite,
    )
    .await;
    let backup_dir = fx
        .root
        .join(".msvc-kit")
        .join("transactions")
        .join(&tx_id)
        .join("backup");
    let backups: Vec<PathBuf> = std::fs::read_dir(&backup_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1, "expected only the ownership backup");
    std::fs::remove_file(&backups[0]).unwrap();

    let error = tx.rollback("injected failure").await.unwrap_err();
    assert!(error.to_string().contains("No backup found"), "{error}");
    let tx_dir = fx.root.join(".msvc-kit").join("transactions").join(&tx_id);
    assert!(tx_dir.join("needs-recovery.json").exists());
    assert!(!read_journal(&fx.root, &tx_id).contains("rolled_back"));
}

// ============================================================================
// P1-A/P1-B: filesystem-boundary hardening for legacy detection and tx setup.
// ============================================================================

/// Test 1: a receipts junction pointing at a real, valid receipt outside the
/// root must NOT be accepted as legacy ownership evidence.
#[tokio::test]
async fn legacy_detection_rejects_receipts_junction() {
    let fx = Fixture::new(&[(REL_FILE, b"planned")]);
    let target = fx.root.join(REL_FILE);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"user-owned").unwrap();

    let outside = fx._guard.path().join("outside-receipts");
    let receipts = outside.join("receipts");
    std::fs::create_dir_all(&receipts).unwrap();
    std::fs::write(
        receipts.join("msvc-14.44.35207-x64-x64.json"),
        legacy_receipt_json(),
    )
    .unwrap();

    let ms_kit = fx.root.join(".msvc-kit");
    std::fs::create_dir_all(&ms_kit).unwrap();
    let link = ms_kit.join("receipts");
    if !make_directory_link(&link, &receipts) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    // Commit through a helper that reports begin() errors too: legacy
    // detection runs in the pre-pass after begin succeeds.
    let error = match transaction::Transaction::begin(&fx.root, "plan", FaultHook::none()) {
        Err(error) => error,
        Ok(mut tx) => {
            let staged = tx
                .stage_package(
                    PKG,
                    std::slice::from_ref(&fx.payload),
                    &HashMap::new(),
                    |_| true,
                )
                .await
                .unwrap();
            let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
            tx.commit(&merged).await.unwrap_err()
        }
    };
    assert!(
        error.to_string().contains("unowned files") || error.to_string().contains("reparse point"),
        "a junction must not grant legacy ownership: {error}"
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"user-owned");
    assert_eq!(
        std::fs::read(receipts.join("msvc-14.44.35207-x64-x64.json")).unwrap(),
        legacy_receipt_json(),
        "external receipt must stay byte-identical"
    );
}

/// Test 2: an extracted-marker symlink/junction with a valid marker outside
/// the root must not grant ownership either.
#[tokio::test]
async fn legacy_detection_rejects_extracted_marker_symlink() {
    let fx = Fixture::new(&[(REL_FILE, b"planned")]);
    let target = fx.root.join(REL_FILE);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"user-owned").unwrap();

    let outside = fx._guard.path().join("outside-markers");
    std::fs::create_dir_all(&outside).unwrap();
    let marker_name = "payload.vsix-v2-0123456789abcdef.done";
    let marker = serde_json::json!({
        "schema": "msvc-kit.extraction-marker.v2",
        "source_sha256": "a".repeat(64),
        "source_name": "payload.vsix"
    });
    std::fs::write(outside.join(marker_name), marker.to_string()).unwrap();

    let link = fx.root.join(".msvc-kit-extracted");
    if !make_directory_link(&link, &outside) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    let error = match transaction::Transaction::begin(&fx.root, "plan", FaultHook::none()) {
        Err(error) => error,
        Ok(mut tx) => {
            let staged = tx
                .stage_package(
                    PKG,
                    std::slice::from_ref(&fx.payload),
                    &HashMap::new(),
                    |_| true,
                )
                .await
                .unwrap();
            let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
            tx.commit(&merged).await.unwrap_err()
        }
    };
    assert!(
        error.to_string().contains("unowned files") || error.to_string().contains("reparse point"),
        "a marker link must not grant legacy ownership: {error}"
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"user-owned");
    assert_eq!(
        std::fs::read(outside.join(marker_name)).unwrap(),
        marker.to_string().as_bytes(),
        "external marker must stay byte-identical"
    );
}

/// Test 3: an arbitrary non-empty receipts directory with an invalid file
/// must never authorize overwriting a user-owned file.
#[tokio::test]
async fn invalid_receipt_does_not_authorize_overwrite() {
    let fx = Fixture::new(&[(REL_FILE, b"planned")]);
    let target = fx.root.join(REL_FILE);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"user-owned").unwrap();
    let original_hash = sha(b"user-owned");

    let receipts = fx.root.join(".msvc-kit").join("receipts");
    std::fs::create_dir_all(&receipts).unwrap();
    std::fs::write(receipts.join("unknown.txt"), b"not a receipt").unwrap();

    let error = commit_one(&fx.root, &fx.payload).await;
    assert!(
        error.to_string().contains("unowned files"),
        "invalid receipts must not authorize an overwrite: {error}"
    );
    assert_eq!(sha(&std::fs::read(&target).unwrap()), original_hash);
    assert_eq!(std::fs::read(&target).unwrap(), b"user-owned");
}

/// Test 4: a genuine historical receipt still allows legitimate updates of
/// files msvc-kit managed, while protecting other unowned files.
#[tokio::test]
async fn genuine_legacy_receipt_still_allows_managed_replace() {
    let fx = Fixture::new(&[(REL_FILE, b"new-bytes")]);
    fx.make_legacy_with_conflicting_file(REL_FILE, b"old-bytes");
    let other = fx.root.join("VC/user-owned.dll");
    std::fs::create_dir_all(other.parent().unwrap()).unwrap();
    std::fs::write(&other, b"foreign").unwrap();

    let mut tx = transaction::Transaction::begin(&fx.root, "plan", FaultHook::none()).unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();
    let outcome = tx.commit(&merged).await.unwrap();

    assert_eq!(outcome.replaced, 1);
    assert_eq!(std::fs::read(fx.root.join(REL_FILE)).unwrap(), b"new-bytes");
    assert_eq!(std::fs::read(&other).unwrap(), b"foreign");
}

/// P1-B Test 1: `transactions` itself is a junction; begin must refuse and
/// leave the outside tree untouched.
#[tokio::test]
async fn transaction_begin_rejects_transactions_junction() {
    let fx = Fixture::new(&[]);
    let outside = fx._guard.path().join("outside-transactions");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"keep").unwrap();

    let ms_kit = fx.root.join(".msvc-kit");
    std::fs::create_dir_all(&ms_kit).unwrap();
    if !make_directory_link(&ms_kit.join("transactions"), &outside) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    let error = transaction::Transaction::begin(&fx.root, "plan", FaultHook::none()).unwrap_err();
    assert!(
        error.to_string().contains("reparse point"),
        "begin must refuse a transactions junction: {error}"
    );
    assert_eq!(
        std::fs::read(outside.join("sentinel.txt")).unwrap(),
        b"keep"
    );
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        1,
        "no transaction data may be created outside the root"
    );
}

/// P1-B Test 2: `.msvc-kit` itself is a junction; begin must refuse.
#[tokio::test]
async fn transaction_begin_rejects_msvc_kit_junction() {
    let fx = Fixture::new(&[]);
    let outside = fx._guard.path().join("outside-msvc-kit");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"keep").unwrap();

    if !make_directory_link(&fx.root.join(".msvc-kit"), &outside) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    let error = transaction::Transaction::begin(&fx.root, "plan", FaultHook::none()).unwrap_err();
    assert!(
        error.to_string().contains("reparse point"),
        "begin must refuse a .msvc-kit junction: {error}"
    );
    assert_eq!(
        std::fs::read(outside.join("sentinel.txt")).unwrap(),
        b"keep"
    );
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
}

/// P1-B Test 3: a junction entry inside `transactions` must be refused by
/// begin; it must never be scanned as a transaction or cleaned.
#[tokio::test]
async fn transaction_begin_rejects_junction_entry_in_transactions() {
    let fx = Fixture::new(&[]);
    let outside = fx._guard.path().join("outside-tx");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"keep").unwrap();

    let tx_root = fx.root.join(".msvc-kit").join("transactions");
    std::fs::create_dir_all(&tx_root).unwrap();
    if !make_directory_link(&tx_root.join("existing-junction"), &outside) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    let error = transaction::Transaction::begin(&fx.root, "plan", FaultHook::none()).unwrap_err();
    assert!(
        error.to_string().contains("reparse point"),
        "begin must refuse a junction transaction entry: {error}"
    );
    assert_eq!(
        std::fs::read(outside.join("sentinel.txt")).unwrap(),
        b"keep"
    );
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
}

/// P1-B Test 4: two transactions begun in the same process with the same plan
/// must have distinct IDs, journals and backup dirs.
#[tokio::test]
async fn transactions_have_unique_ids_for_same_plan() {
    let fx = Fixture::new(&[]);
    let first = transaction::Transaction::begin(&fx.root, "same-plan", FaultHook::none()).unwrap();
    let second = transaction::Transaction::begin(&fx.root, "same-plan", FaultHook::none()).unwrap();
    assert_ne!(first.transaction_id(), second.transaction_id());
    assert_ne!(first.journal_path(), second.journal_path());
    assert!(first.journal_path().is_file());
    assert!(second.journal_path().is_file());
}

/// P1-C: a junction planted *before the final pre-write re-check* must be
/// refused at the write; no bytes may leak outside the install root.
///
/// This is a deterministic TOCTOU race injection: `FaultHook::with_action`
/// swaps the target's parent directory (`root/VC`) for a reparse point aimed
/// at an outside directory after the first pre-write re-check and before the
/// second one. No sleeps or timing are involved — the hook runs synchronously
/// at the fault point, so the race is reproduced every run (or skipped when
/// the platform cannot create a reparse point).
///
/// Scope: this proves the *re-check* catches a swap that happens before it.
/// It does **not** prove the final check-to-use window is closed — see
/// `toctou_create_final_window_is_documented_residual_risk`.
#[tokio::test]
async fn commit_rechecks_target_before_write() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let fx = Fixture::new(&[(REL_FILE, b"content")]);
    // Outside tree the junction will target; holds a sentinel so we can prove
    // no bytes leaked through the link.
    let outside = fx._guard.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"keep").unwrap();

    // The hook swaps `root/VC` for a junction to `outside` at the instant the
    // transaction is about to copy the staged file. It runs after the first
    // pre-write re-check (which created `VC/Tools/plan`) and before the second
    // re-check immediately preceding `std::fs::copy`.
    let root = fx.root.clone();
    let outside_for_action = outside.clone();
    let created = Arc::new(AtomicBool::new(false));
    let created_for_action = Arc::clone(&created);
    let hooks = FaultHook::with_action(FaultPoint::BeforeInstall, move || {
        let vc = root.join("VC");
        let _ = std::fs::remove_dir_all(&vc);
        let ok = make_directory_link(&vc, &outside_for_action);
        created_for_action.store(ok, Ordering::SeqCst);
    });

    let mut tx = transaction::Transaction::begin(&fx.root, "plan", hooks).unwrap();
    let tx_id = tx.transaction_id().to_string();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();

    let commit_result = tx.commit(&merged).await;
    if !created.load(Ordering::SeqCst) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    // The commit must refuse to write through the junction.
    let error = commit_result.unwrap_err();
    assert!(
        error.to_string().contains("reparse point"),
        "commit must refuse a parent swapped to a reparse point before the write, got: {error}"
    );

    // No bytes leaked into the outside tree.
    let leaked = outside.join("Tools").join("plan").join("file.txt");
    assert!(
        !leaked.exists(),
        "commit wrote outside the install boundary"
    );
    assert_eq!(
        std::fs::read(outside.join("sentinel.txt")).unwrap(),
        b"keep",
        "outside sentinel must be untouched"
    );

    // The transaction must NOT be recorded as committed or cleanly rolled
    // back: the rollback path also refuses to traverse the reparse point, so
    // it is parked in needs_recovery for a human decision. This proves the
    // failure was not silently absorbed.
    assert!(!transaction::journal_has_committed(
        &fx.root
            .join(".msvc-kit")
            .join("transactions")
            .join(&tx_id)
            .join("journal.jsonl")
    ));
    let report = transaction::recover_pending(&fx.root).await.unwrap();
    assert_eq!(report.needs_recovery, vec![tx_id.clone()], "{report:?}");
    assert!(
        report.rolled_back.is_empty() && report.cleaned_staging.is_empty(),
        "a refused rollback must not be reported as success: {report:?}"
    );
}

/// P1-C (replace path): a junction planted before the final pre-backup
/// re-check must be refused, so neither the backup nor the install traverses
/// the reparse point. The outside tree (and the original file it mirrors)
/// stays intact.
///
/// Scope: like the create-path test above, this covers a swap that lands
/// *before* the re-check. The final check-to-use window is pinned separately
/// by `toctou_replace_final_window_is_documented_residual_risk`.
#[tokio::test]
async fn commit_rechecks_replace_target_before_backup() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let fx = Fixture::new(&[(REL_FILE, b"new-bytes")]);
    fx.make_legacy_with_conflicting_file(REL_FILE, b"old-bytes");
    // Outside tree the junction targets; it already mirrors the old file so a
    // backup/install through the link would touch it. A sentinel proves no
    // unrelated bytes leaked.
    let outside = fx._guard.path().join("outside-replace");
    let outside_file = outside.join("Tools").join("plan").join("file.txt");
    std::fs::create_dir_all(outside_file.parent().unwrap()).unwrap();
    std::fs::write(&outside_file, b"old-bytes").unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"keep").unwrap();

    // Swap `root/VC` for a junction at BeforeBackup: after the target re-check
    // and before the backup rename. The following re-check (and the
    // backup-path reparse guard) must refuse.
    let root = fx.root.clone();
    let outside_for_action = outside.clone();
    let created = Arc::new(AtomicBool::new(false));
    let created_for_action = Arc::clone(&created);
    let hooks = FaultHook::with_action(FaultPoint::BeforeBackup, move || {
        let vc = root.join("VC");
        let _ = std::fs::remove_dir_all(&vc);
        let ok = make_directory_link(&vc, &outside_for_action);
        created_for_action.store(ok, Ordering::SeqCst);
    });

    let mut tx = transaction::Transaction::begin(&fx.root, "plan", hooks).unwrap();
    let tx_id = tx.transaction_id().to_string();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();

    let commit_result = tx.commit(&merged).await;
    if !created.load(Ordering::SeqCst) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    let error = commit_result.unwrap_err();
    assert!(
        error.to_string().contains("reparse point"),
        "commit must refuse a parent swapped to a reparse point before the backup, got: {error}"
    );

    // Neither the backup nor the install may have traversed the link: the
    // outside copy of the old file and the sentinel are byte-identical.
    assert_eq!(
        std::fs::read(&outside_file).unwrap(),
        b"old-bytes",
        "backup/install must not write through the reparse point"
    );
    assert_eq!(
        std::fs::read(outside.join("sentinel.txt")).unwrap(),
        b"keep"
    );

    // Not committed, and recovery must park it (needs_recovery), not roll it
    // back: the rollback path also refuses the reparse point.
    assert!(!transaction::journal_has_committed(
        &fx.root
            .join(".msvc-kit")
            .join("transactions")
            .join(&tx_id)
            .join("journal.jsonl")
    ));
    let report = transaction::recover_pending(&fx.root).await.unwrap();
    assert_eq!(report.needs_recovery, vec![tx_id.clone()], "{report:?}");
    assert!(
        report.rolled_back.is_empty() && report.cleaned_staging.is_empty(),
        "a refused rollback must not be reported as success: {report:?}"
    );
}

/// Residual TOCTOU window — create path: the junction is planted *after* the
/// final `current_hash`/`prepare_target` check has passed and immediately
/// before `std::fs::copy`. There is no further path validation in between in
/// the current implementation, so the copy follows the swapped path. This
/// test pins that reality rather than pretending the check closes the race.
///
/// Closing it requires handle-relative operations (a directory handle plus
/// `openat`/`NtCreateFile`-style calls), which the current `std::fs` code does
/// not use. See the "Residual TOCTOU risk" section in the transaction module.
#[tokio::test]
async fn toctou_create_final_window_is_documented_residual_risk() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let fx = Fixture::new(&[(REL_FILE, b"content")]);
    let outside = fx._guard.path().join("outside-create-final");
    // The copy must have a destination inside the swapped-in tree; otherwise
    // it would fail for "directory missing" and prove nothing about the race.
    std::fs::create_dir_all(outside.join("Tools").join("plan")).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"keep").unwrap();

    let root = fx.root.clone();
    let outside_for_action = outside.clone();
    let swapped = Arc::new(AtomicBool::new(false));
    let swapped_for_action = Arc::clone(&swapped);
    let hooks = FaultHook::with_action(FaultPoint::AfterInstallCheck, move || {
        let vc = root.join("VC");
        let _ = std::fs::remove_dir_all(&vc);
        let ok = make_directory_link(&vc, &outside_for_action);
        swapped_for_action.store(ok, Ordering::SeqCst);
    });

    let mut tx = transaction::Transaction::begin(&fx.root, "plan", hooks).unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();

    let outcome = tx.commit(&merged).await;
    if !swapped.load(Ordering::SeqCst) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    // The final check-to-use window is real: the write succeeds *through* the
    // link planted after the last validation. This is the residual risk; the
    // test deliberately asserts it instead of claiming the window is closed.
    outcome.expect("copy after the final check must follow the swapped path");
    assert_eq!(
        std::fs::read(outside.join("Tools").join("plan").join("file.txt")).unwrap(),
        b"content",
        "the file was written through the junction planted after the final check"
    );
    assert_eq!(
        std::fs::read(outside.join("sentinel.txt")).unwrap(),
        b"keep"
    );
}

/// Residual TOCTOU window — replace path: the junction is planted *after*
/// the final `current_hash` + backup-path reparse guard and immediately
/// before `std::fs::rename`. The backup rename follows the link and moves a
/// file from outside the root into the transaction backup directory; the
/// following install copy is refused by its own fresh re-check, so the commit
/// fails, but the boundary violation already happened. Same conclusion as
/// above: repeated path checks narrow but do not eliminate the check-to-use
/// race.
#[tokio::test]
async fn toctou_replace_final_window_is_documented_residual_risk() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let fx = Fixture::new(&[(REL_FILE, b"new-bytes")]);
    fx.make_legacy_with_conflicting_file(REL_FILE, b"old-bytes");
    let outside = fx._guard.path().join("outside-replace-final");
    let outside_file = outside.join("Tools").join("plan").join("file.txt");
    std::fs::create_dir_all(outside_file.parent().unwrap()).unwrap();
    std::fs::write(&outside_file, b"old-bytes").unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"keep").unwrap();

    let root = fx.root.clone();
    let outside_for_action = outside.clone();
    let swapped = Arc::new(AtomicBool::new(false));
    let swapped_for_action = Arc::clone(&swapped);
    let hooks = FaultHook::with_action(FaultPoint::AfterBackupCheck, move || {
        let vc = root.join("VC");
        let _ = std::fs::remove_dir_all(&vc);
        let ok = make_directory_link(&vc, &outside_for_action);
        swapped_for_action.store(ok, Ordering::SeqCst);
    });

    let mut tx = transaction::Transaction::begin(&fx.root, "plan", hooks).unwrap();
    let staged = tx
        .stage_package(
            PKG,
            std::slice::from_ref(&fx.payload),
            &HashMap::new(),
            |_| true,
        )
        .await
        .unwrap();
    let merged = tx.merge(std::slice::from_ref(&staged)).unwrap();

    let outcome = tx.commit(&merged).await;
    if !swapped.load(Ordering::SeqCst) {
        eprintln!("SKIP: cannot create a directory reparse point");
        return;
    }

    // The backup rename followed the link planted after the final validation:
    // the outside file was moved into this transaction's backup directory. The
    // install copy that follows is then refused by its own fresh re-check, so
    // the commit fails — but the boundary violation already happened. That is
    // exactly the residual risk this test pins.
    assert!(
        outcome.is_err(),
        "the later install re-check should stop the commit after the rename: {outcome:?}"
    );
    let error = outcome.unwrap_err();
    assert!(
        error.to_string().contains("reparse point"),
        "expected the install re-check to refuse the swapped path, got: {error}"
    );
    assert!(
        !outside_file.exists(),
        "the backup rename moved the outside file through the junction"
    );
    let backup_dir = single_transaction_dir(&fx.root).join("backup");
    let backups: Vec<PathBuf> = std::fs::read_dir(&backup_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1, "expected one backup file: {backups:?}");
    assert_eq!(
        std::fs::read(&backups[0]).unwrap(),
        b"old-bytes",
        "the outside file's bytes were moved into the backup directory"
    );
    assert_eq!(
        std::fs::read(outside.join("sentinel.txt")).unwrap(),
        b"keep"
    );
}
