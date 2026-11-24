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
        std::fs::write(receipts.join("legacy.json"), b"{}").unwrap();
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
