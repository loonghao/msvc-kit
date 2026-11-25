//! File-level install transactions (spec 05 §6-8, spec 02 §6).
//!
//! A transaction extracts packages into a per-transaction workspace under
//! `.msvc-kit/transactions/<tx-id>/`, merges the staged package trees with
//! collision detection, and commits file-by-file with a write-ahead journal:
//! every backup/install decision is journaled before it happens and
//! acknowledged after, so recovery can always distinguish "journal says the
//! action ran" from "the action actually ran" by hashing the file system.
//!
//! Invariants enforced here (spec I-001/I-002/I-006):
//! - the shared install root is never replaced or cleared; only files this
//!   transaction owns (created, planned, or adopted from a legacy msvc-kit
//!   root with backup) are touched;
//! - unknown existing files are never overwritten (they become
//!   `UnownedPathConflict` during the pre-pass, before any mutation);
//! - a failed transaction either rolls back to the previous bytes or
//!   persists a `needs_recovery` state that refuses new transactions until
//!   `recover_pending` resolves it (idempotently).

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::contracts::{
    InstalledFileEntry, InstalledManifest, JournalEvent, JournalEventKind, OwnershipEntry,
    OwnershipIndexDocument, TransactionState, INSTALLED_MANIFEST_SCHEMA, OWNERSHIP_INDEX_SCHEMA,
    TRANSACTION_JOURNAL_SCHEMA,
};
use crate::downloader::hash::compute_file_hash;
use crate::error::{MsvcKitError, Result};
use crate::installer::path_safety::{ensure_safe_directory, real_dir_or_file};
use crate::storage::atomic_write;

pub(crate) const TRANSACTIONS_DIR: &str = "transactions";
pub(crate) const INSTALLED_MANIFESTS_DIR: &str = "installed-manifests";
pub(crate) const OWNERSHIP_FILE: &str = "ownership.json";
pub(crate) const JOURNAL_FILE: &str = "journal.jsonl";
pub(crate) const NEEDS_RECOVERY_FILE: &str = "needs-recovery.json";

/// Where a fault injection (or real failure) sits in the transaction
/// lifecycle. Used by tests (spec 08 §10) and by failure diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultPoint {
    AfterWorkspaceCreated,
    AfterJournalHeader,
    AfterPackageStaged,
    AfterMerge,
    BeforeBackup,
    AfterBackup,
    BeforeInstall,
    AfterInstall,
    BeforeOwnershipWrite,
    BeforeInstalledManifestWrite,
    BeforeCommittedMarker,
    AfterCommittedMarker,
    BeforeCleanup,
}

/// Points at which a transaction must fail (after doing everything up to and
/// including the named step). Empty = normal operation.
#[derive(Debug, Clone, Default)]
pub struct FaultHook {
    points: Arc<Mutex<Vec<FaultPoint>>>,
}

impl FaultHook {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn with_points(points: &[FaultPoint]) -> Self {
        Self {
            points: Arc::new(Mutex::new(points.to_vec())),
        }
    }

    fn hit(&self, point: FaultPoint) -> Result<()> {
        let active = self
            .points
            .lock()
            .expect("fault hook mutex poisoned")
            .clone();
        if active.contains(&point) {
            Err(MsvcKitError::Other(format!(
                "injected fault at {point:?}: transaction aborts here"
            )))
        } else {
            Ok(())
        }
    }

    pub fn is_armed(&self, point: FaultPoint) -> bool {
        self.points
            .lock()
            .expect("fault hook mutex poisoned")
            .contains(&point)
    }
}

/// One staged file with its content digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    /// Path relative to the package staging root.
    pub relative: PathBuf,
    pub size: u64,
    pub sha256: String,
}

/// One package extracted into the transaction workspace.
#[derive(Debug, Clone)]
pub struct StagedPackage {
    pub package_id: String,
    /// Files this package staged (empty when the extraction marker proved the
    /// package was already extracted, in which case the root is untouched).
    pub files: Vec<StagedFile>,
    /// True when a valid v2 extraction marker skipped the extraction.
    pub skipped_via_marker: bool,
}

/// One merged file ready to commit, with every package that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedFile {
    /// Path relative to the install root.
    pub relative: PathBuf,
    /// Absolute path in the staging workspace.
    pub staging_path: PathBuf,
    pub size: u64,
    pub sha256: String,
    pub owners: Vec<String>,
}

/// The merge result: deterministic file list with collision detection done.
#[derive(Debug, Clone, Default)]
pub struct MergedTree {
    pub files: Vec<MergedFile>,
}

/// Outcome summary of a committed transaction.
#[derive(Debug, Clone, Default)]
pub struct CommittedInstall {
    pub transaction_id: String,
    pub created: usize,
    pub replaced: usize,
    pub skipped: usize,
    /// Root-relative files this install wrote (created or replaced).
    pub written: Vec<String>,
}

/// What `recover_pending` found and did.
#[derive(Debug, Clone, Default)]
pub struct RecoveryReport {
    pub transactions_examined: usize,
    pub rolled_back: Vec<String>,
    pub cleaned_staging: Vec<String>,
    pub cleaned_backups: Vec<String>,
    pub needs_recovery: Vec<String>,
}

/// A write-ahead journal record as persisted (one JSON object per line).
/// Header lines carry schema/ids; event lines carry the JournalEvent fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalLine {
    #[serde(rename = "type")]
    line_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<JournalEventKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recorded_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_root: Option<String>,
    /// Exact backup file name (within this transaction's backup dir) for the
    /// event's path, when a backup was taken. Written on WillBackup/BackupDone
    /// so a crash between them and during recovery uses the precise identity
    /// instead of a fuzzy suffix match. Missing on old journals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backup_name: Option<String>,
}

pub(crate) fn msvc_kit_dir(root: &Path) -> PathBuf {
    root.join(".msvc-kit")
}

pub(crate) fn transactions_dir(root: &Path) -> PathBuf {
    msvc_kit_dir(root).join(TRANSACTIONS_DIR)
}

pub fn ownership_path(root: &Path) -> PathBuf {
    msvc_kit_dir(root).join(OWNERSHIP_FILE)
}

fn installed_manifest_path(root: &Path, tx_id: &str) -> PathBuf {
    msvc_kit_dir(root)
        .join(INSTALLED_MANIFESTS_DIR)
        .join(format!("{tx_id}.json"))
}

/// Encode a package ID into a safe single directory name (no separators,
/// no traversal, bounded length, content-addressed suffix for uniqueness).
fn safe_package_dir_name(package_id: &str) -> String {
    let sanitized: String = package_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let mut hasher = Sha256::new();
    hasher.update(package_id.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("{sanitized}-{}", &digest[..8])
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Build a backup file name that is *uniquely* tied to one root-relative
/// install path.
///
/// P1 (v3): the old scheme truncated the content digest to 8 hex chars and
/// only escaped path separators, so the two distinct paths `a/b` and `a__b`
/// both produced names ending in `__a__b` that `find_backup` could not tell
/// apart. The full SHA-256 (64 hex chars) of the normalized lowercase key
/// that the ownership index uses makes the name injective: distinct keys
/// always yield distinct file names, and a bare key is tied to exactly one
/// backup file.
fn backup_file_name(key: &str) -> String {
    format!("{}__", sha256_hex(key.as_bytes()))
}

/// Whether a payload file is an MSI package (the only format whose bytes are
/// handed to the Windows Installer).
fn is_msi_payload(payload: &Path) -> bool {
    payload
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("msi"))
        .unwrap_or(false)
}

/// Look up the official SHA256 of a payload.
///
/// The map is keyed by the payload's path as it exists on disk. Paths are
/// compared exactly first and then by file name, so a caller that keyed the
/// map by the manifest payload name still resolves correctly.
fn official_digest_for(payload: &Path, digests: &HashMap<PathBuf, String>) -> Option<String> {
    if let Some(digest) = digests.get(payload) {
        return Some(digest.clone());
    }
    let file_name = payload.file_name()?;
    digests
        .iter()
        .find(|(key, _)| key.file_name() == Some(file_name))
        .map(|(_, digest)| digest.clone())
}

/// Load the ownership index, or an empty one when the root predates it.
fn load_ownership(root: &Path) -> Result<OwnershipIndexDocument> {
    let path = ownership_path(root);
    super::path_safety::check_no_reparse_within(root, &path)?;
    let Some(meta) = real_dir_or_file(&path)? else {
        return Ok(OwnershipIndexDocument {
            schema: OWNERSHIP_INDEX_SCHEMA.to_string(),
            entries: Vec::new(),
        });
    };
    if !meta.is_file() {
        return Err(MsvcKitError::Other(format!(
            "Ownership index path is not a file: {:?}",
            path
        )));
    }
    let document: OwnershipIndexDocument = serde_json::from_slice(&std::fs::read(&path)?)?;
    Ok(document)
}

/// Legacy installation evidence, built only from *verifiable* credentials.
///
/// Historical msvc-kit receipts record download provenance (component,
/// version, payload names/hashes) but no installed-tree paths, so they cannot
/// yield a per-file ownership set. The only compatibility contract this
/// version can honour is the root-level adoption used before the ownership
/// index existed: a root with at least one *valid* legacy receipt or one valid
/// v2 extraction marker is adopted wholesale, with every replaced file backed
/// up in the transaction journal. A non-empty directory or a link to foreign
/// data is NOT such evidence.
#[derive(Debug, Default)]
struct LegacyOwnership {
    valid_evidence: bool,
}

/// Validate a legacy receipt's schema and fields without trusting its file
/// name or path. Both schema versions written by historical msvc-kit releases
/// are accepted; only a fully parsed, consistent record counts.
fn valid_legacy_receipt(path: &Path) -> Result<bool> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyReceipt {
        schema: String,
        component: String,
        version: String,
        #[allow(dead_code)]
        arch: crate::Architecture,
        #[allow(dead_code)]
        host_arch: crate::Architecture,
        #[serde(default)]
        #[allow(dead_code)]
        vs_channel: Option<String>,
        #[allow(dead_code)]
        components: Vec<String>,
        #[allow(dead_code)]
        payloads: Vec<crate::toolchain_lock::LockedPayload>,
    }

    let Some(meta) = real_dir_or_file(path)? else {
        return Ok(false);
    };
    if !meta.is_file() {
        return Ok(false);
    }
    let bytes = std::fs::read(path)?;
    let Ok(receipt) = serde_json::from_slice::<LegacyReceipt>(&bytes) else {
        return Ok(false);
    };
    Ok(matches!(
        receipt.schema.as_str(),
        "msvc-kit.installation-receipt.v1" | "msvc-kit.installation-receipt.v2"
    ) && !receipt.component.is_empty()
        && !receipt.version.is_empty())
}

/// Validate a v2 extraction marker: name shape plus schema and source digest
/// inside the file. v1 markers ("<name>-<16hex>.done") never validate, exactly
/// as in `probe_extraction_marker`.
fn valid_legacy_marker(path: &Path) -> Result<bool> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyMarker {
        schema: String,
        source_sha256: String,
        #[allow(dead_code)]
        source_name: String,
    }

    if !path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains("-v2-") && name.ends_with(".done"))
    {
        return Ok(false);
    }
    let Some(meta) = real_dir_or_file(path)? else {
        return Ok(false);
    };
    if !meta.is_file() {
        return Ok(false);
    }
    let bytes = std::fs::read(path)?;
    let Ok(marker) = serde_json::from_slice::<LegacyMarker>(&bytes) else {
        return Ok(false);
    };
    Ok(marker.schema == super::EXTRACTION_MARKER_SCHEMA && marker.source_sha256.len() == 64)
}

/// Detect legacy msvc-kit management without following links or swallowing
/// errors. Every credential directory is checked component-by-component; a
/// reparse point, an unexpected metadata error, or a malformed credential is
/// never treated as ownership evidence.
fn detect_legacy_ownership(root: &Path) -> Result<LegacyOwnership> {
    super::path_safety::check_no_reparse_within(root, root)?;
    let mut ownership = LegacyOwnership::default();

    let receipts = msvc_kit_dir(root).join("receipts");
    super::path_safety::check_no_reparse_within(root, &receipts)?;
    if let Some(meta) = real_dir_or_file(&receipts)? {
        if !meta.is_dir() {
            return Err(MsvcKitError::Other(format!(
                "Legacy receipts path is not a directory: {:?}",
                receipts
            )));
        }
        for entry in std::fs::read_dir(&receipts)? {
            let entry = entry?;
            let path = entry.path();
            super::path_safety::check_no_reparse_within(root, &path)?;
            if valid_legacy_receipt(&path)? {
                ownership.valid_evidence = true;
                break;
            }
        }
    }

    if ownership.valid_evidence {
        return Ok(ownership);
    }

    let markers = root.join(".msvc-kit-extracted");
    super::path_safety::check_no_reparse_within(root, &markers)?;
    if let Some(meta) = real_dir_or_file(&markers)? {
        if !meta.is_dir() {
            return Err(MsvcKitError::Other(format!(
                "Legacy extraction-marker path is not a directory: {:?}",
                markers
            )));
        }
        for entry in std::fs::read_dir(&markers)? {
            let entry = entry?;
            let path = entry.path();
            super::path_safety::check_no_reparse_within(root, &path)?;
            if valid_legacy_marker(&path)? {
                ownership.valid_evidence = true;
                break;
            }
        }
    }

    Ok(ownership)
}

/// An active transaction. The caller must hold the install root lock.
#[derive(Debug)]
pub struct Transaction {
    root: PathBuf,
    tx_id: String,
    tx_dir: PathBuf,
    packages_dir: PathBuf,
    backup_dir: PathBuf,
    journal_path: PathBuf,
    seq: u64,
    state: TransactionState,
    faults: FaultHook,
    /// Ownership entries added by this transaction (path -> entry), for
    /// rollback bookkeeping.
    ownership_added: HashMap<String, OwnershipEntry>,
}

impl Transaction {
    /// Begin a new transaction after verifying no unresolved state blocks it.
    ///
    /// Callers must hold the install root lock; `recover_pending` must have
    /// been run (this constructor re-checks for `needs_recovery` markers).
    pub fn begin(root: &Path, plan_id: &str, faults: FaultHook) -> Result<Self> {
        // P1: establish a verified directory chain before *any* read or
        // write. A junction/symlink at `.msvc-kit` or `transactions` would
        // otherwise redirect journal creation outside the install root.
        let ms_kit = ensure_safe_directory(root, Path::new(".msvc-kit"))?;
        let tx_root = ensure_safe_directory(&ms_kit, Path::new(TRANSACTIONS_DIR))?;
        // Refuse to start when any transaction is in needs-recovery. Entries
        // are checked with symlink_metadata and never followed; an entry that
        // is not a real directory is an unsafe workspace and refused.
        for entry in std::fs::read_dir(&tx_root)? {
            let entry = entry?;
            let entry_path = entry.path();
            super::path_safety::check_no_reparse_within(root, &entry_path)?;
            let meta = real_dir_or_file(&entry_path)?.ok_or_else(|| {
                MsvcKitError::Other(format!(
                    "Transaction entry vanished during begin: {:?}",
                    entry_path
                ))
            })?;
            if !meta.is_dir() {
                return Err(MsvcKitError::Other(format!(
                    "Refusing non-directory entry in transactions: {:?}",
                    entry_path
                )));
            }
            let marker = entry_path.join(NEEDS_RECOVERY_FILE);
            super::path_safety::check_no_reparse_within(root, &marker)?;
            if real_dir_or_file(&marker)?.is_some() {
                return Err(MsvcKitError::Other(format!(
                    "Transaction {} is in needs_recovery; run `msvc-kit recover --dir {}` \
                     before installing",
                    entry.file_name().to_string_lossy(),
                    root.display()
                )));
            }
        }
        // A secure random nonce makes collisions impossible even when the
        // same process starts twice within one second with the same plan.
        // `create_dir` (not `create_dir_all`) is the exclusive claim: an
        // existing directory is never reused or overwritten.
        use rand::RngCore as _;
        let mut nonce = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce);
        let tx_id = format!(
            "{}-{}-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%S"),
            &sha256_hex(format!("{plan_id}|{}", std::process::id()).as_bytes())[..12],
            hex::encode(nonce)
        );
        let tx_relative = Path::new(TRANSACTIONS_DIR).join(&tx_id);
        let tx_dir = tx_root.join(&tx_id);
        // Exclusive creation: never reuse or overwrite an existing tx dir.
        super::path_safety::check_no_reparse_within(root, &tx_dir)?;
        std::fs::create_dir(&tx_dir)?;
        let tx_dir = ensure_safe_directory(&ms_kit, &tx_relative)?;
        let packages_dir = ensure_safe_directory(&tx_dir, Path::new("packages"))?;
        let backup_dir = ensure_safe_directory(&tx_dir, Path::new("backup"))?;
        let journal_path = tx_dir.join(JOURNAL_FILE);
        faults.hit(FaultPoint::AfterWorkspaceCreated)?;

        let mut tx = Self {
            root: root.to_path_buf(),
            tx_id: tx_id.clone(),
            tx_dir,
            packages_dir,
            backup_dir,
            journal_path,
            seq: 0,
            state: TransactionState::Created,
            faults,
            ownership_added: HashMap::new(),
        };
        tx.append_line(&JournalLine {
            line_type: "header".to_string(),
            sequence: None,
            kind: None,
            path: None,
            hash: None,
            recorded_at: None,
            schema: Some(TRANSACTION_JOURNAL_SCHEMA.to_string()),
            transaction_id: Some(tx_id),
            plan_id: Some(plan_id.to_string()),
            target_root: Some(root.to_string_lossy().into_owned()),
            backup_name: None,
        })?;
        tx.faults.hit(FaultPoint::AfterJournalHeader)?;
        Ok(tx)
    }

    pub fn transaction_id(&self) -> &str {
        &self.tx_id
    }

    pub fn journal_path(&self) -> &Path {
        &self.journal_path
    }

    /// Re-check the full boundary immediately before a write and create any
    /// missing parent directories one level at a time. This closes the
    /// pre-pass/write TOCTOU gap: a junction planted after classification is
    /// refused here rather than silently written through.
    fn prepare_target(&self, target: &Path) -> Result<()> {
        super::path_safety::check_no_reparse_within(&self.root, target)?;
        if let Some(parent) = target.parent() {
            let relative = parent.strip_prefix(&self.root).map_err(|_| {
                MsvcKitError::Other(format!("Target outside install root: {:?}", target))
            })?;
            if !relative.as_os_str().is_empty() {
                ensure_safe_directory(&self.root, relative)?;
            }
        }
        // Final check after creation: the target itself must still be safe.
        super::path_safety::check_no_reparse_within(&self.root, target)?;
        match real_dir_or_file(target)? {
            Some(meta) if meta.is_dir() => Err(MsvcKitError::Other(format!(
                "Refusing to write file over a directory at {:?}",
                target
            ))),
            _ => Ok(()),
        }
    }

    /// Verify a classification recorded in the pre-pass is still true right
    /// before the mutation. Returns the current hash (None = absent).
    async fn current_hash(
        &self,
        target: &Path,
        expected_prepass: Option<&str>,
    ) -> Result<Option<String>> {
        self.prepare_target(target)?;
        let current = match real_dir_or_file(target)? {
            Some(meta) => {
                if !meta.is_file() {
                    return Err(MsvcKitError::Other(format!(
                        "Target is no longer a regular file: {:?}",
                        target
                    )));
                }
                Some(compute_file_hash(target).await?)
            }
            None => None,
        };
        match (current.as_deref(), expected_prepass) {
            (None, None) => {}
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => {}
            _ => {
                return Err(MsvcKitError::Other(format!(
                    "Target changed between pre-check and write: {:?}",
                    target
                )))
            }
        }
        Ok(current)
    }

    fn append_line(&mut self, line: &JournalLine) -> Result<()> {
        let mut bytes = serde_json::to_vec(line)?;
        bytes.push(b'\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.journal_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if line.line_type == "event" {
            if let (Some(sequence), Some(kind)) = (&line.sequence, &line.kind) {
                self.seq = *sequence;
                self.state = state_for_event(*kind);
            }
        }
        Ok(())
    }

    fn append_event(
        &mut self,
        kind: JournalEventKind,
        path: &str,
        hash: Option<String>,
    ) -> Result<()> {
        self.append_event_with_backup(kind, path, hash, None)
    }

    /// Like [`Self::append_event`] but records the exact backup file name (within
    /// this transaction's backup dir) that a WillBackup/BackupDone event took,
    /// so recovery can restore the precise file even when two distinct paths
    /// sanitize to the same human-readable suffix (P1: `a/b` vs `a__b`).
    fn append_event_with_backup(
        &mut self,
        kind: JournalEventKind,
        path: &str,
        hash: Option<String>,
        backup_name: Option<String>,
    ) -> Result<()> {
        let event = JournalEvent {
            sequence: self.seq + 1,
            kind,
            path: path.to_string(),
            hash,
            backup_name,
            recorded_at: chrono::Utc::now(),
        };
        self.append_line(&JournalLine {
            line_type: "event".to_string(),
            sequence: Some(event.sequence),
            kind: Some(event.kind),
            path: Some(event.path.clone()),
            hash: event.hash.clone(),
            recorded_at: Some(event.recorded_at),
            schema: None,
            transaction_id: None,
            plan_id: None,
            target_root: None,
            backup_name: event.backup_name.clone(),
        })
    }

    /// Extract one package's payloads into its own staging directory.
    ///
    /// A valid v2 extraction marker skips the extraction entirely (the
    /// package was installed before and the root is left untouched).
    ///
    /// `digests` maps a payload path to the *official* SHA256 of that
    /// payload (from the channel manifest / plan). MSI payloads MUST have an
    /// entry: the digest is re-verified immediately before the installer
    /// runs (G-4 / R3), so a payload with no official digest cannot be
    /// installed and is rejected instead of being passed through unverified.
    pub async fn stage_package(
        &mut self,
        package_id: &str,
        payload_files: &[PathBuf],
        digests: &HashMap<PathBuf, String>,
        classify: impl Fn(&Path) -> bool,
    ) -> Result<StagedPackage> {
        let dir_name = safe_package_dir_name(package_id);
        let package_dir = ensure_safe_directory(
            &self.root,
            &Path::new(".msvc-kit")
                .join(TRANSACTIONS_DIR)
                .join(&self.tx_id)
                .join("packages")
                .join(&dir_name),
        )?;

        let marker_dir = self.root.join(".msvc-kit-extracted");
        let mut files = Vec::new();
        let mut skipped_via_marker = false;
        let mut extracted_any = false;

        for payload in payload_files {
            if !classify(payload) {
                continue;
            }
            let expected_sha256 = official_digest_for(payload, digests);
            if is_msi_payload(payload) && expected_sha256.is_none() {
                // G-4 (R3): an MSI without an official digest cannot be
                // verified before the installer runs, so it must not be
                // installed at all.
                return Err(MsvcKitError::Other(format!(
                    "Refusing to install {}: no official SHA256 is known for this MSI payload",
                    payload.display()
                )));
            }
            let probe = super::probe_extraction_marker(&marker_dir, payload).await?;
            if probe.valid {
                skipped_via_marker = true;
                continue;
            }
            super::extract_package_with_progress(
                &payload.clone(),
                &package_dir,
                super::inner_progress_enabled(),
                expected_sha256.as_deref(),
            )
            .await?;
            extracted_any = true;
        }

        // G-3: Build the staged file list with digests. Only newly extracted
        // payloads contribute files; marker-skipped payloads leave the root
        // untouched. In mixed cache state, newly extracted files are staged
        // while marker-skipped ones are not re-installed (they are already
        // in the root from a previous install). This fixes the bug where
        // `skipped_via_marker` was a package-level boolean that dropped ALL
        // staged files when ANY payload had a valid marker.
        if extracted_any {
            collect_files(&package_dir, &package_dir, &mut files)?;
        }

        self.faults.hit(FaultPoint::AfterPackageStaged)?;
        Ok(StagedPackage {
            package_id: package_id.to_string(),
            files,
            skipped_via_marker,
        })
    }

    /// Plan and execute pruning for a staged package (spec 06 section 1.5):
    /// removal happens in the staging tree only, every removal is journaled
    /// as FileRemoved, and the staged file list is updated so the merge (and
    /// therefore the commit) never sees the pruned bytes.
    pub async fn prune_staged(
        &mut self,
        staged: &mut StagedPackage,
        profile: crate::installer::pruning::PruningProfile,
        context: &crate::installer::pruning::PruningContext,
    ) -> Result<Vec<crate::installer::pruning::PruneAction>> {
        use crate::installer::pruning;

        if staged.files.is_empty() {
            return Ok(Vec::new());
        }
        let staged_dir_name = safe_package_dir_name(&staged.package_id);
        let package_dir = self.packages_dir.join(staged_dir_name);
        let staged_for_plan: Vec<(PathBuf, String, u64, String)> = staged
            .files
            .iter()
            .map(|file| {
                (
                    file.relative.clone(),
                    file.sha256.clone(),
                    file.size,
                    staged.package_id.clone(),
                )
            })
            .collect();
        let actions = pruning::plan_pruning(profile, &staged_for_plan, context)?;
        if actions.is_empty() {
            return Ok(actions);
        }
        let mut executed = Vec::new();
        for action in actions {
            pruning::remove_staged_file(&package_dir, &action)?;
            self.append_event(
                JournalEventKind::FileRemoved,
                &format!("{} ({})", action.relative.to_string_lossy(), action.rule_id),
                None,
            )?;
            executed.push(action);
        }
        let pruned: std::collections::HashSet<PathBuf> = executed
            .iter()
            .map(|action| action.relative.clone())
            .collect();
        staged.files.retain(|file| !pruned.contains(&file.relative));
        Ok(executed)
    }

    /// Merge staged packages into one deterministic file list, rejecting
    /// cross-package collisions (same path, different bytes) before anything
    /// is committed.
    pub fn merge(&self, staged: &[StagedPackage]) -> Result<MergedTree> {
        let mut by_path: HashMap<String, MergedFile> = HashMap::new();
        for package in staged {
            for file in &package.files {
                let key = file
                    .relative
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_lowercase();
                let absolute = self
                    .packages_dir
                    .join(safe_package_dir_name(&package.package_id))
                    .join(&file.relative);
                match by_path.get_mut(&key) {
                    Some(existing) => {
                        if existing.sha256 != file.sha256 {
                            return Err(MsvcKitError::Other(format!(
                                "Staged packages conflict on {}: package {:?} produced {} but package {:?} produced {}",
                                key,
                                existing.owners.first().cloned().unwrap_or_default(),
                                existing.sha256,
                                package.package_id,
                                file.sha256
                            )));
                        }
                        if !existing.owners.contains(&package.package_id) {
                            existing.owners.push(package.package_id.clone());
                        }
                    }
                    None => {
                        by_path.insert(
                            key,
                            MergedFile {
                                relative: file.relative.clone(),
                                staging_path: absolute,
                                size: file.size,
                                sha256: file.sha256.clone(),
                                owners: vec![package.package_id.clone()],
                            },
                        );
                    }
                }
            }
        }
        // Deterministic order: sort by lowercase relative path.
        let mut files: Vec<MergedFile> = by_path.into_values().collect();
        files.sort_by(|left, right| {
            left.relative
                .to_string_lossy()
                .to_lowercase()
                .cmp(&right.relative.to_string_lossy().to_lowercase())
        });
        self.faults.hit(FaultPoint::AfterMerge)?;
        Ok(MergedTree { files })
    }

    /// Commit the merged tree to the install root with journal-protected
    /// backup/replace semantics, then write ownership and the installed
    /// manifest.
    pub async fn commit(&mut self, merged: &MergedTree) -> Result<CommittedInstall> {
        // P1: refuse to commit anywhere that would resolve writes through a
        // symlink/junction. The install root itself must not be a reparse
        // point, and every target path is checked so a junction planted under
        // the root (e.g. `install/VC -> D:/outside`) cannot redirect a write
        // outside the install boundary. Checked up front, before any mutation.
        super::path_safety::check_no_reparse_within(&self.root, &self.root)?;

        // ---- Pre-pass: classify every file before touching the root. ----
        let mut ownership = load_ownership(&self.root)?;
        let legacy = detect_legacy_ownership(&self.root)?;
        let legacy_managed = legacy.valid_evidence;
        let ownership_lookup: HashMap<String, OwnershipEntry> = ownership
            .entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.clone()))
            .collect();

        enum Planned {
            Create,
            Skip,
            ReplaceOwned {
                backup_path: PathBuf,
                /// Exact backup file name (within the tx backup dir) for the
                /// replaced file; recorded in the journal so recovery restores
                /// the precise file even when two paths share a suffix (P1).
                backup_file_name: String,
                /// Digest observed in the pre-pass; re-verified before write.
                old_hash: String,
            },
        }
        let mut plan: Vec<(MergedFile, Planned)> = Vec::new();
        let mut created = 0usize;
        let mut replaced = 0usize;
        let mut skipped = 0usize;
        let mut conflicts: Vec<String> = Vec::new();

        for file in &merged.files {
            let key = file
                .relative
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase();
            let target = self.root.join(&file.relative);

            // P1: a symlink/junction anywhere along the target path would
            // redirect the write outside the install boundary.
            super::path_safety::check_no_reparse_within(&self.root, &target)?;

            // P1: classify by metadata that cannot follow links, and fail
            // closed. `Path::is_file`/`Path::exists` swallow metadata errors
            // and *follow* symlinks; neither is acceptable here. If the
            // target is unreadable we must NOT treat it as absent (that would
            // let an unknown existing file be overwritten later).
            let existing_hash = match std::fs::symlink_metadata(&target) {
                Ok(meta) => {
                    if meta.is_dir() || meta.file_type().is_symlink() {
                        // A directory or symlink stands where the file should
                        // be: refuse rather than fight it.
                        conflicts.push(format!(
                            "{}: a non-file object already occupies the target path",
                            key
                        ));
                        continue;
                    }
                    Some(compute_file_hash(&target).await?)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };

            let planned = match existing_hash {
                None => Planned::Create,
                Some(hash) if hash.eq_ignore_ascii_case(&file.sha256) => {
                    // Identical bytes: no write needed.
                    skipped += 1;
                    Planned::Skip
                }
                Some(hash) => {
                    let owned = ownership_lookup
                        .get(&key)
                        .map(|entry| entry.sha256.as_deref() == Some(hash.to_lowercase().as_str()))
                        .unwrap_or(false);
                    if owned || legacy_managed {
                        let backup_name = backup_file_name(&key);
                        let backup_path = self.backup_dir.join(&backup_name);
                        replaced += 1;
                        Planned::ReplaceOwned {
                            backup_path,
                            backup_file_name: backup_name,
                            old_hash: hash.clone(),
                        }
                    } else {
                        conflicts.push(format!(
                            "{key}: existing file is not owned by msvc-kit and its bytes \
                             ({hash}) differ from the planned content ({})",
                            file.sha256
                        ));
                        continue;
                    }
                }
            };
            plan.push((file.clone(), planned));
        }

        if !conflicts.is_empty() {
            return Err(MsvcKitError::Other(format!(
                "Transaction {}: refusing to install over unowned files:\n{}",
                self.tx_id,
                conflicts
                    .iter()
                    .map(|conflict| format!("  - {conflict}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
        }

        // ---- Execute pass: journal-protected mutations. ----
        let mut written: Vec<String> = Vec::new();
        for (file, planned) in &plan {
            let key = file
                .relative
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase();
            let journal_path = file.relative.to_string_lossy().replace('\\', "/");
            let target = self.root.join(&file.relative);
            match planned {
                Planned::Create => {
                    // TOCTOU: re-validate absence and the directory chain
                    // immediately before writing (both before the journal
                    // event and again after the fault hook).
                    self.current_hash(&target, None).await?;
                    self.append_event(
                        JournalEventKind::WillInstall,
                        &journal_path,
                        Some(file.sha256.clone()),
                    )?;
                    self.faults.hit(FaultPoint::BeforeInstall)?;
                    self.current_hash(&target, None).await?;
                    std::fs::copy(&file.staging_path, &target)?;
                    let installed = compute_file_hash(&target).await?;
                    if !installed.eq_ignore_ascii_case(&file.sha256) {
                        return Err(MsvcKitError::HashMismatch {
                            file: key.clone(),
                            expected: file.sha256.clone(),
                            actual: installed,
                        });
                    }
                    self.faults.hit(FaultPoint::AfterInstall)?;
                    self.append_event(
                        JournalEventKind::InstallDone,
                        &journal_path,
                        Some(file.sha256.clone()),
                    )?;
                    created += 1;
                    written.push(journal_path.clone());
                    self.ownership_added.insert(
                        key.clone(),
                        OwnershipEntry {
                            path: key.clone(),
                            sha256: Some(file.sha256.clone()),
                            kind: "file".to_string(),
                            owners: file.owners.clone(),
                        },
                    );
                }
                Planned::Skip => {
                    // Bytes already correct: re-verify before recording ownership.
                    self.current_hash(&target, Some(&file.sha256)).await?;
                    // record ownership only.
                    self.ownership_added.insert(
                        key.clone(),
                        OwnershipEntry {
                            path: key.clone(),
                            sha256: Some(file.sha256.clone()),
                            kind: "file".to_string(),
                            owners: file.owners.clone(),
                        },
                    );
                }
                Planned::ReplaceOwned {
                    backup_path,
                    backup_file_name,
                    old_hash,
                } => {
                    // P1: never back up / write through a reparse point along
                    // the backup path either. Re-check the target's pre-pass
                    // digest immediately before the backup rename.
                    super::path_safety::check_no_reparse_within(&self.root, backup_path)?;
                    self.current_hash(&target, Some(old_hash)).await?;
                    self.append_event_with_backup(
                        JournalEventKind::WillBackup,
                        &journal_path,
                        Some(old_hash.clone()),
                        Some(backup_file_name.clone()),
                    )?;
                    self.faults.hit(FaultPoint::BeforeBackup)?;
                    self.current_hash(&target, Some(old_hash)).await?;
                    if let Some(parent) = backup_path.parent() {
                        ensure_safe_directory(
                            &self.root,
                            parent.strip_prefix(&self.root).map_err(|_| {
                                MsvcKitError::Other(format!(
                                    "Backup outside install root: {:?}",
                                    backup_path
                                ))
                            })?,
                        )?;
                    }
                    super::path_safety::check_no_reparse_within(&self.root, &target)?;
                    std::fs::rename(&target, backup_path)?;
                    self.faults.hit(FaultPoint::AfterBackup)?;
                    self.append_event_with_backup(
                        JournalEventKind::BackupDone,
                        &journal_path,
                        Some(old_hash.clone()),
                        Some(backup_file_name.clone()),
                    )?;
                    self.append_event(
                        JournalEventKind::WillInstall,
                        &journal_path,
                        Some(file.sha256.clone()),
                    )?;
                    self.faults.hit(FaultPoint::BeforeInstall)?;
                    // The rename removed the original: re-confirm absence and
                    // a clean directory chain before installing new bytes.
                    self.current_hash(&target, None).await?;
                    std::fs::copy(&file.staging_path, &target)?;
                    let installed = compute_file_hash(&target).await?;
                    if !installed.eq_ignore_ascii_case(&file.sha256) {
                        return Err(MsvcKitError::HashMismatch {
                            file: key.clone(),
                            expected: file.sha256.clone(),
                            actual: installed,
                        });
                    }
                    self.faults.hit(FaultPoint::AfterInstall)?;
                    self.append_event(
                        JournalEventKind::InstallDone,
                        &journal_path,
                        Some(file.sha256.clone()),
                    )?;
                    written.push(journal_path.clone());
                    self.ownership_added.insert(
                        key.clone(),
                        OwnershipEntry {
                            path: key.clone(),
                            sha256: Some(file.sha256.clone()),
                            kind: "file".to_string(),
                            owners: file.owners.clone(),
                        },
                    );
                }
            }
        }

        // ---- Ownership index merge (atomic), with its own backup for
        // rollback. ----
        self.faults.hit(FaultPoint::BeforeOwnershipWrite)?;
        let ownership_target = ownership_path(&self.root);
        let ownership_backup_name = backup_file_name(OWNERSHIP_FILE);
        let ownership_backup = self.backup_dir.join(&ownership_backup_name);
        // P1: never back up through a reparse point on the metadata path either.
        super::path_safety::check_no_reparse_within(&self.root, &ownership_backup)?;
        super::path_safety::check_no_reparse_within(&self.root, &ownership_target)?;
        ensure_safe_directory(&self.root, Path::new(".msvc-kit"))?;
        // Journal the metadata transition: a None hash means "did not exist
        // before this transaction" so rollback removes the file; a digest
        // means "existed" and rollback restores the backup bytes.
        let prior_ownership_hash = match real_dir_or_file(&ownership_target)? {
            Some(meta) if meta.is_file() => {
                let hash = compute_file_hash(&ownership_target).await?;
                std::fs::copy(&ownership_target, &ownership_backup)?;
                Some(hash)
            }
            Some(_) => {
                return Err(MsvcKitError::Other(format!(
                    "Ownership index is not a regular file: {:?}",
                    ownership_target
                )))
            }
            None => None,
        };
        self.append_event_with_backup(
            JournalEventKind::WillBackup,
            OWNERSHIP_FILE,
            prior_ownership_hash.clone(),
            Some(ownership_backup_name.clone()),
        )?;
        self.append_event_with_backup(
            JournalEventKind::BackupDone,
            OWNERSHIP_FILE,
            prior_ownership_hash,
            Some(ownership_backup_name),
        )?;
        for (key, entry) in &self.ownership_added {
            match ownership
                .entries
                .iter_mut()
                .find(|existing| existing.path == *key)
            {
                Some(existing) => {
                    existing.sha256 = entry.sha256.clone();
                    for owner in &entry.owners {
                        if !existing.owners.contains(owner) {
                            existing.owners.push(owner.clone());
                        }
                    }
                }
                None => ownership.entries.push(entry.clone()),
            }
        }
        ownership
            .entries
            .sort_by(|left, right| left.path.cmp(&right.path));
        let ownership_bytes = serde_json::to_vec_pretty(&ownership)?;
        atomic_write(&ownership_path(&self.root), &ownership_bytes)?;

        // ---- Installed manifest (atomic). ----
        self.faults.hit(FaultPoint::BeforeInstalledManifestWrite)?;
        let manifest = InstalledManifest {
            schema: INSTALLED_MANIFEST_SCHEMA.to_string(),
            install_id: self.tx_id.clone(),
            files: plan
                .iter()
                .filter(|(_, planned)| {
                    matches!(planned, Planned::Create | Planned::ReplaceOwned { .. })
                })
                .map(|(file, _)| InstalledFileEntry {
                    path: file.relative.to_string_lossy().replace('\\', "/"),
                    size: file.size,
                    sha256: file.sha256.clone(),
                    owners: file.owners.clone(),
                    kind: "file".to_string(),
                })
                .collect(),
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        ensure_safe_directory(
            &self.root,
            &Path::new(".msvc-kit").join(INSTALLED_MANIFESTS_DIR),
        )?;
        atomic_write(
            &installed_manifest_path(&self.root, &self.tx_id),
            &manifest_bytes,
        )?;

        // ---- Committed marker. ----
        self.faults.hit(FaultPoint::BeforeCommittedMarker)?;
        self.append_event(JournalEventKind::Committed, "", None)?;
        self.faults.hit(FaultPoint::AfterCommittedMarker)?;

        // ---- Cleanup: backups are no longer needed after a clean commit. ----
        self.faults.hit(FaultPoint::BeforeCleanup)?;
        let _ = std::fs::remove_dir_all(&self.backup_dir);

        Ok(CommittedInstall {
            transaction_id: self.tx_id.clone(),
            created,
            replaced,
            skipped,
            written,
        })
    }

    /// Roll back using the journal (best effort, spec 05 §8.2). Consumes the
    /// transaction; on failure the transaction is marked needs_recovery.
    pub async fn rollback(mut self, reason: &str) -> Result<()> {
        match self.rollback_inner().await {
            Ok(()) => {
                // P2 (v3): the RolledBack journal event is the durable proof
                // that the root was restored. If it cannot be written, the
                // rollback did not complete and must not be reported as a
                // success; keep the transaction in needs_recovery instead.
                if let Err(error) = self.append_event(JournalEventKind::RolledBack, reason, None) {
                    let _ = self.mark_needs_recovery(&error.to_string());
                    return Err(error);
                }
                Ok(())
            }
            Err(error) => {
                if let Err(marker_error) = self.mark_needs_recovery(&error.to_string()) {
                    return Err(MsvcKitError::Other(format!(
                        "{error} (recording needs_recovery also failed: {marker_error})"
                    )));
                }
                Err(error)
            }
        }
    }

    /// P1 (v3): rollback and recovery can run in a later process, long after
    /// the crash, so the install root may have changed in the meantime.
    /// Re-validate the transaction workspace before reading or mutating
    /// anything (mirrors the guard `commit` applies before it writes).
    fn check_workspace_safety(&self) -> Result<()> {
        for path in [&self.tx_dir, &self.journal_path, &self.backup_dir] {
            super::path_safety::check_no_reparse_within(&self.root, path)?;
        }
        Ok(())
    }

    /// P1 (v3): resolve a backup file and refuse it when the resolved path
    /// (or any directory leading to it) is a reparse point.
    fn resolve_backup_checked(&self, key: &str, precise: Option<&str>) -> Result<PathBuf> {
        let backup = resolve_backup(&self.backup_dir, key, precise)?;
        super::path_safety::check_no_reparse_within(&self.root, &backup)?;
        Ok(backup)
    }

    async fn rollback_inner(&mut self) -> Result<()> {
        // P1 (v3): never read the journal or restore files through a reparse
        // point planted under the install root after the crash.
        self.check_workspace_safety()?;

        let events = read_journal_events(&self.journal_path)?;

        // Group events per path (chronological). For each path, the journal
        // tells us whether the original bytes were backed up (WillBackup/
        // BackupDone carrying the old digest) and what digest we installed
        // (WillInstall/InstallDone carrying the new digest).
        #[derive(Default)]
        struct PathPlan {
            old_hash: Option<String>,
            new_hash: Option<String>,
            backup_done: bool,
            /// Precise backup file name (within the backup dir) recorded for
            /// this path, when known (P1: distinguishes `a/b` from `a__b`).
            backup_name: Option<String>,
        }
        let mut per_path: HashMap<String, PathPlan> = HashMap::new();
        for event in &events {
            if event.path.is_empty() {
                continue;
            }
            let plan = per_path.entry(event.path.clone()).or_default();
            match event.kind {
                JournalEventKind::WillBackup => {
                    plan.old_hash = event.hash.clone();
                    if let Some(name) = &event.backup_name {
                        plan.backup_name = Some(name.clone());
                    }
                }
                JournalEventKind::BackupDone => {
                    plan.old_hash = event.hash.clone();
                    plan.backup_done = true;
                    if let Some(name) = &event.backup_name {
                        plan.backup_name = Some(name.clone());
                    }
                }
                JournalEventKind::WillInstall | JournalEventKind::InstallDone => {
                    plan.new_hash = event.hash.clone()
                }
                _ => {}
            }
        }

        for (path, plan) in per_path {
            let is_metadata = path == OWNERSHIP_FILE;
            let target = if is_metadata {
                ownership_path(&self.root)
            } else {
                self.root.join(&path)
            };
            // P1 (v3): every path the journal asks recovery to touch must
            // resolve inside the install root without crossing a reparse
            // point. Checked immediately before the operation, matching the
            // guard commit() applies before it starts writing.
            super::path_safety::check_no_reparse_within(&self.root, &target)?;

            if is_metadata {
                // Ownership index: a None old hash means the index did not
                // exist before this transaction; a digest means the backup
                // holds the previous index bytes.
                match (&plan.old_hash, plan.backup_done) {
                    (None, _) => {
                        // P2 (v3): NotFound is the desired end state; any
                        // other failure means the original "no index" state
                        // was not restored and must fail closed.
                        match std::fs::remove_file(&target) {
                            Ok(()) => {}
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                            Err(error) => return Err(error.into()),
                        }
                    }
                    (Some(_), true) => {
                        let backup =
                            self.resolve_backup_checked(&path, plan.backup_name.as_deref())?;
                        std::fs::copy(&backup, &target)?;
                    }
                    (Some(_), false) => {}
                }
                continue;
            }

            match (&plan.old_hash, &plan.new_hash) {
                // Created: remove it only if it still matches our bytes.
                (None, Some(new_hash)) => {
                    // P2 (v3): a hash read failure is NOT proof that the file
                    // is absent or foreign. Propagate it (and any metadata
                    // error other than NotFound) so the transaction stays in
                    // needs_recovery instead of being recorded as a clean
                    // rollback with an unverified file left behind.
                    let target_missing = match std::fs::symlink_metadata(&target) {
                        Ok(meta) if meta.is_dir() || meta.file_type().is_symlink() => {
                            return Err(rollback_conflict(
                                &path,
                                "the path is no longer a regular file installed by this transaction",
                            ));
                        }
                        Ok(_) => false,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                        Err(error) => return Err(error.into()),
                    };
                    if target_missing {
                        // Already absent: the desired end state.
                        continue;
                    }
                    let actual = compute_file_hash(&target).await?;
                    if !actual.eq_ignore_ascii_case(new_hash) {
                        // Someone modified the file after the failure. It is
                        // no longer ours to delete, and the original "absent"
                        // state cannot be restored without destroying their
                        // bytes: manual resolution required.
                        return Err(rollback_conflict(
                            &path,
                            &format!(
                                "the installed file was modified after the transaction failed \
                                 (expected {new_hash}, found {actual}); refusing to delete bytes \
                                 this transaction does not own"
                            ),
                        ));
                    }
                    match std::fs::remove_file(&target) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                // Replaced: restore the backup. If the target no longer
                // matches our installed digest, a third party wrote to the
                // path after the failure; refuse to guess and let the caller
                // mark the transaction needs_recovery.
                (Some(_), Some(new_hash)) => {
                    let backup = self.resolve_backup_checked(&path, plan.backup_name.as_deref())?;
                    let target_missing = match std::fs::symlink_metadata(&target) {
                        Ok(meta) if meta.is_dir() || meta.file_type().is_symlink() => {
                            return Err(rollback_conflict(
                                &path,
                                "the target path is no longer a regular file",
                            ));
                        }
                        Ok(_) => false,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                        Err(error) => return Err(error.into()),
                    };
                    if target_missing {
                        std::fs::rename(&backup, &target)?;
                    } else {
                        let actual = compute_file_hash(&target).await?;
                        if !actual.eq_ignore_ascii_case(new_hash) {
                            return Err(rollback_conflict(
                                &path,
                                &format!(
                                    "the file no longer matches the installed digest {new_hash} \
                                     and the original bytes are in the transaction backup"
                                ),
                            ));
                        }
                        std::fs::copy(&backup, &target)?;
                        std::fs::remove_file(&backup)?;
                    }
                }
                // Backup intent without an install event: the rename either
                // never completed (target still holds the original bytes) or
                // completed with its BackupDone event lost (target is absent
                // and the backup must be restored).
                (Some(old_hash), None) => {
                    let target_missing = match std::fs::symlink_metadata(&target) {
                        Ok(meta) if meta.is_dir() || meta.file_type().is_symlink() => {
                            return Err(rollback_conflict(
                                &path,
                                "the target path is no longer a regular file",
                            ));
                        }
                        Ok(_) => false,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                        Err(error) => return Err(error.into()),
                    };
                    if target_missing {
                        let backup =
                            self.resolve_backup_checked(&path, plan.backup_name.as_deref())?;
                        std::fs::rename(&backup, &target)?;
                    } else {
                        let actual = compute_file_hash(&target).await?;
                        if !actual.eq_ignore_ascii_case(old_hash) {
                            return Err(rollback_conflict(
                                &path,
                                &format!(
                                    "the target no longer holds the original digest {old_hash} \
                                     and the backup was not installed"
                                ),
                            ));
                        }
                    }
                }
                (None, None) => {}
            }
        }

        // The installed manifest of a rolled-back transaction is void.
        let manifest = installed_manifest_path(&self.root, &self.tx_id);
        super::path_safety::check_no_reparse_within(&self.root, &manifest)?;
        match std::fs::remove_file(&manifest) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    fn mark_needs_recovery(&mut self, reason: &str) -> Result<()> {
        let marker_path = self.tx_dir.join(NEEDS_RECOVERY_FILE);
        // P1 (v3): never write the recovery marker through a reparse point
        // planted in the transaction workspace.
        super::path_safety::check_no_reparse_within(&self.root, &marker_path)?;
        let marker = serde_json::json!({
            "schema": TRANSACTION_JOURNAL_SCHEMA,
            "transaction_id": self.tx_id,
            "state": "needs_recovery",
            "reason": reason,
            "recorded_at": chrono::Utc::now(),
        });
        atomic_write(&marker_path, serde_json::to_vec_pretty(&marker)?.as_slice())
    }
}

/// Error for a rollback that cannot restore the pre-transaction state without
/// guessing: the path was modified by a third party or no longer exists in a
/// shape the journal can account for.
fn rollback_conflict(path: &str, detail: &str) -> MsvcKitError {
    MsvcKitError::Other(format!(
        "Rollback conflict for {path}: {detail}; manual resolution required"
    ))
}

/// Whether the journal already carries a Committed event (the point of no
/// return for rollback decisions).
pub fn journal_has_committed(journal: &Path) -> bool {
    read_journal_events(journal)
        .map(|events| {
            events
                .iter()
                .any(|event| event.kind == JournalEventKind::Committed)
        })
        .unwrap_or(false)
}

fn state_for_event(kind: JournalEventKind) -> TransactionState {
    match kind {
        JournalEventKind::WillBackup | JournalEventKind::BackupDone => TransactionState::Committing,
        JournalEventKind::WillInstall
        | JournalEventKind::InstallDone
        | JournalEventKind::FileRemoved => TransactionState::Committing,
        JournalEventKind::ReceiptCommitted => TransactionState::Committing,
        JournalEventKind::Committed => TransactionState::Committed,
        JournalEventKind::RolledBack => TransactionState::RolledBack,
    }
}

fn read_journal_events(journal: &Path) -> Result<Vec<JournalEvent>> {
    let mut events = Vec::new();
    let content = std::fs::read_to_string(journal)?;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed: JournalLine = serde_json::from_str(line)?;
        if parsed.line_type == "event" {
            if let (Some(sequence), Some(kind), Some(path), Some(recorded_at)) = (
                parsed.sequence,
                parsed.kind,
                parsed.path.clone(),
                parsed.recorded_at,
            ) {
                events.push(JournalEvent {
                    sequence,
                    kind,
                    path,
                    hash: parsed.hash,
                    backup_name: parsed.backup_name,
                    recorded_at,
                });
            }
        }
    }
    Ok(events)
}

/// Resolve the backup file for `key` (a root-relative install path) within a
/// transaction's backup dir.
///
/// When the journal recorded an exact backup name (`precise`), that name is
/// used directly — two distinct paths that sanitize to the same suffix (P1:
/// `a/b` vs `a__b`) therefore always recover the correct file. When no name
/// was recorded (a pre-fix journal from an in-flight transaction), fall back
/// to the legacy suffix scan so recovery stays possible.
fn resolve_backup(backup_dir: &Path, key: &str, precise: Option<&str>) -> Result<PathBuf> {
    if let Some(name) = precise {
        let candidate = backup_dir.join(name);
        if real_dir_or_file(&candidate)?.is_some() {
            return Ok(candidate);
        }
    }
    find_backup(backup_dir, key)
}

/// Legacy backup lookup: suffix-match `key` against backup file names. Kept
/// for journals written before the precise backup-name field existed; it is
/// ambiguous when two paths collapse onto one suffix, which is why new
/// journals carry an exact name and callers use [`resolve_backup`].
fn find_backup(backup_dir: &Path, key: &str) -> Result<PathBuf> {
    let suffix = format!(
        "__{}",
        key.replace("/", "__")
            .replace(std::path::MAIN_SEPARATOR_STR, "__")
    );
    let mut best: Option<PathBuf> = None;
    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(&suffix) {
            best = Some(entry.path());
        }
    }
    best.ok_or_else(|| {
        MsvcKitError::Other(format!(
            "No backup found for {key:?} in {}",
            backup_dir.display()
        ))
    })
}

fn collect_files(base: &Path, dir: &Path, out: &mut Vec<StagedFile>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(base)
                .map_err(|error| MsvcKitError::Other(error.to_string()))?
                .to_path_buf();
            let size = entry.metadata()?.len();
            let sha256 = compute_file_hash_sync(&path)?;
            out.push(StagedFile {
                relative,
                size,
                sha256,
            });
        }
    }
    Ok(())
}

fn compute_file_hash_sync(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Resolve any unfinished transactions under `root` (idempotent).
///
/// - Transactions with no committed/rolled-back terminal event and no
///   executed install/backup actions: staging is safe to clean.
/// - Transactions with executed actions: rolled back (restore backups /
///   remove installed files that still match our digests).
/// - Committed transactions: only the backup directory is cleaned.
/// - Needs-recovery transactions: reported, never touched (until their
///   marker is removed by an explicit user decision).
pub async fn recover_pending(root: &Path) -> Result<RecoveryReport> {
    let mut report = RecoveryReport::default();
    let tx_root = transactions_dir(root);
    let Some(tx_root_meta) = real_dir_or_file(&tx_root)? else {
        return Ok(report);
    };
    if !tx_root_meta.is_dir() {
        return Err(MsvcKitError::Other(format!(
            "Transactions path is not a directory: {:?}",
            tx_root
        )));
    }
    // P1 (v3): the transaction workspace is part of the install boundary;
    // refuse to enumerate or clean it when a reparse point (a junction
    // planted after the crash) redirects it elsewhere.
    super::path_safety::check_no_reparse_within(root, &tx_root)?;
    for entry in std::fs::read_dir(&tx_root)? {
        let entry = entry?;
        let tx_dir = entry.path();
        let tx_id = entry.file_name().to_string_lossy().into_owned();
        super::path_safety::check_no_reparse_within(root, &tx_dir)?;
        let Some(tx_meta) = real_dir_or_file(&tx_dir)? else {
            return Err(MsvcKitError::Other(format!(
                "Transaction entry vanished during recovery: {:?}",
                tx_dir
            )));
        };
        if !tx_meta.is_dir() {
            return Err(MsvcKitError::Other(format!(
                "Refusing non-directory transaction entry: {:?}",
                tx_dir
            )));
        }
        report.transactions_examined += 1;
        let journal = tx_dir.join(JOURNAL_FILE);
        super::path_safety::check_no_reparse_within(root, &journal)?;
        let marker = tx_dir.join(NEEDS_RECOVERY_FILE);
        super::path_safety::check_no_reparse_within(root, &marker)?;
        if real_dir_or_file(&marker)?.is_some() {
            report.needs_recovery.push(tx_id);
            continue;
        }
        if real_dir_or_file(&journal)?.is_none() {
            // No journal: nothing was decided; safe to clean staging.
            let _ = std::fs::remove_dir_all(&tx_dir);
            report.cleaned_staging.push(tx_id);
            continue;
        }
        let events = read_journal_events(&journal)?;
        let committed = events
            .iter()
            .any(|event| event.kind == JournalEventKind::Committed);
        let rolled_back = events
            .iter()
            .any(|event| event.kind == JournalEventKind::RolledBack);
        let executed = events.iter().any(|event| {
            matches!(
                event.kind,
                JournalEventKind::WillInstall
                    | JournalEventKind::InstallDone
                    | JournalEventKind::WillBackup
                    | JournalEventKind::BackupDone
            )
        });

        if committed {
            // Only the backup directory may be cleaned.
            let backup = tx_dir.join("backup");
            // P1 (v3): cleaning a committed transaction must not traverse a
            // reparse point either.
            super::path_safety::check_no_reparse_within(root, &backup)?;
            if real_dir_or_file(&backup)?.is_some_and(|meta| meta.is_dir()) {
                let _ = std::fs::remove_dir_all(&backup);
            }
            report.cleaned_backups.push(tx_id);
            continue;
        }
        if rolled_back {
            // Already rolled back: clean staging.
            let _ = std::fs::remove_dir_all(&tx_dir);
            report.cleaned_staging.push(tx_id);
            continue;
        }
        if !executed {
            // Nothing reached the root: staging-only cleanup.
            let _ = std::fs::remove_dir_all(&tx_dir);
            report.cleaned_staging.push(tx_id);
            continue;
        }
        // Actions reached the root: roll back via the journal.
        let mut tx = Transaction {
            root: root.to_path_buf(),
            tx_id: tx_id.clone(),
            tx_dir: tx_dir.clone(),
            packages_dir: tx_dir.join("packages"),
            backup_dir: tx_dir.join("backup"),
            journal_path: journal,
            seq: events.last().map(|event| event.sequence).unwrap_or(0),
            state: TransactionState::Committing,
            faults: FaultHook::none(),
            ownership_added: HashMap::new(),
        };
        match tx.rollback_inner().await {
            Ok(()) => {
                // P2 (v3): only report a clean rollback once the journal
                // records it; otherwise leave the transaction for the next
                // recovery run (with its data intact).
                match tx.append_event(JournalEventKind::RolledBack, "recovery", None) {
                    Ok(()) => {
                        let _ = std::fs::remove_dir_all(&tx_dir);
                        report.rolled_back.push(tx_id);
                    }
                    Err(error) => {
                        let _ = tx.mark_needs_recovery(&error.to_string());
                        report.needs_recovery.push(tx_id);
                    }
                }
            }
            Err(error) => {
                let _ = tx.mark_needs_recovery(&error.to_string());
                report.needs_recovery.push(tx_id);
            }
        }
    }
    Ok(report)
}
