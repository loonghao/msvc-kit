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

/// Load the ownership index, or an empty one when the root predates it.
fn load_ownership(root: &Path) -> Result<OwnershipIndexDocument> {
    let path = ownership_path(root);
    if !path.exists() {
        return Ok(OwnershipIndexDocument {
            schema: OWNERSHIP_INDEX_SCHEMA.to_string(),
            entries: Vec::new(),
        });
    }
    let document: OwnershipIndexDocument = serde_json::from_slice(&std::fs::read(&path)?)?;
    Ok(document)
}

/// Whether the root carries evidence that msvc-kit managed it before the
/// ownership index existed (legacy adoption path).
fn root_has_legacy_management(root: &Path) -> bool {
    let receipts = msvc_kit_dir(root).join("receipts");
    let markers = root.join(".msvc-kit-extracted");
    (receipts.is_dir()
        && std::fs::read_dir(&receipts).is_ok_and(|mut entries| entries.next().is_some()))
        || (markers.is_dir()
            && std::fs::read_dir(&markers).is_ok_and(|mut entries| entries.next().is_some()))
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
        let tx_root = transactions_dir(root);
        std::fs::create_dir_all(&tx_root)?;
        // Refuse to start when any transaction is in needs-recovery.
        for entry in std::fs::read_dir(&tx_root)? {
            let entry = entry?;
            if entry.path().join(NEEDS_RECOVERY_FILE).exists() {
                return Err(MsvcKitError::Other(format!(
                    "Transaction {} is in needs_recovery; run `msvc-kit recover --dir {}` \
                     before installing",
                    entry.file_name().to_string_lossy(),
                    root.display()
                )));
            }
        }
        let tx_id = format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%S"),
            &sha256_hex(format!("{plan_id}|{}", std::process::id()).as_bytes())[..12]
        );
        let tx_dir = tx_root.join(&tx_id);
        let packages_dir = tx_dir.join("packages");
        let backup_dir = tx_dir.join("backup");
        let journal_path = tx_dir.join(JOURNAL_FILE);
        std::fs::create_dir_all(&packages_dir)?;
        std::fs::create_dir_all(&backup_dir)?;
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
        let event = JournalEvent {
            sequence: self.seq + 1,
            kind,
            path: path.to_string(),
            hash,
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
        })
    }

    /// Extract one package's payloads into its own staging directory.
    ///
    /// A valid v2 extraction marker skips the extraction entirely (the
    /// package was installed before and the root is left untouched).
    pub async fn stage_package(
        &mut self,
        package_id: &str,
        payload_files: &[PathBuf],
        classify: impl Fn(&Path) -> bool,
    ) -> Result<StagedPackage> {
        let dir_name = safe_package_dir_name(package_id);
        let package_dir = self.packages_dir.join(&dir_name);
        std::fs::create_dir_all(&package_dir)?;

        let marker_dir = self.root.join(".msvc-kit-extracted");
        let mut files = Vec::new();
        let mut skipped_via_marker = false;

        for payload in payload_files {
            if !classify(payload) {
                continue;
            }
            let probe = super::probe_extraction_marker(&marker_dir, payload).await?;
            if probe.valid {
                skipped_via_marker = true;
                continue;
            }
            super::extract_package(&payload.clone(), &package_dir).await?;
        }

        // Build the staged file list with digests.
        if !skipped_via_marker {
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
        // ---- Pre-pass: classify every file before touching the root. ----
        let mut ownership = load_ownership(&self.root)?;
        let legacy_managed = root_has_legacy_management(&self.root);
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
            let existing_hash = if target.is_file() {
                compute_file_hash(&target).await.ok()
            } else if target.exists() {
                // A directory (or something unreadable) stands where the file
                // should be: refuse rather than fight it.
                conflicts.push(format!(
                    "{}: a non-file object already occupies the target path",
                    key
                ));
                continue;
            } else {
                None
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
                        let backup_path = self.backup_dir.join(format!(
                            "{}__{}",
                            &sha256_hex(key.as_bytes())[..8],
                            file.relative.to_string_lossy().replace(['/', '\\'], "__")
                        ));
                        replaced += 1;
                        Planned::ReplaceOwned {
                            backup_path,
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
                    self.append_event(
                        JournalEventKind::WillInstall,
                        &journal_path,
                        Some(file.sha256.clone()),
                    )?;
                    self.faults.hit(FaultPoint::BeforeInstall)?;
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
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
                    // Bytes already correct: record ownership only.
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
                    old_hash,
                } => {
                    self.append_event(
                        JournalEventKind::WillBackup,
                        &journal_path,
                        Some(old_hash.clone()),
                    )?;
                    self.faults.hit(FaultPoint::BeforeBackup)?;
                    if let Some(parent) = backup_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::rename(&target, backup_path)?;
                    self.faults.hit(FaultPoint::AfterBackup)?;
                    self.append_event(
                        JournalEventKind::BackupDone,
                        &journal_path,
                        Some(old_hash.clone()),
                    )?;
                    self.append_event(
                        JournalEventKind::WillInstall,
                        &journal_path,
                        Some(file.sha256.clone()),
                    )?;
                    self.faults.hit(FaultPoint::BeforeInstall)?;
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
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
        let ownership_backup = self.backup_dir.join(format!(
            "{}_ownership.json",
            &sha256_hex(b"ownership.json")[..8]
        ));
        // Journal the metadata transition: a None hash means "did not exist
        // before this transaction" so rollback removes the file; a digest
        // means "existed" and rollback restores the backup bytes.
        let prior_ownership_hash = if ownership_target.exists() {
            let hash = compute_file_hash(&ownership_target).await?;
            std::fs::copy(&ownership_target, &ownership_backup)?;
            Some(hash)
        } else {
            None
        };
        self.append_event(
            JournalEventKind::WillBackup,
            OWNERSHIP_FILE,
            prior_ownership_hash.clone(),
        )?;
        self.append_event(
            JournalEventKind::BackupDone,
            OWNERSHIP_FILE,
            prior_ownership_hash,
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
        std::fs::create_dir_all(msvc_kit_dir(&self.root).join(INSTALLED_MANIFESTS_DIR))?;
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
                let _ = self.append_event(JournalEventKind::RolledBack, reason, None);
                Ok(())
            }
            Err(error) => {
                self.mark_needs_recovery(&error.to_string())?;
                Err(error)
            }
        }
    }

    async fn rollback_inner(&mut self) -> Result<()> {
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
        }
        let mut per_path: HashMap<String, PathPlan> = HashMap::new();
        for event in &events {
            if event.path.is_empty() {
                continue;
            }
            let plan = per_path.entry(event.path.clone()).or_default();
            match event.kind {
                JournalEventKind::WillBackup => plan.old_hash = event.hash.clone(),
                JournalEventKind::BackupDone => {
                    plan.old_hash = event.hash.clone();
                    plan.backup_done = true;
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

            if is_metadata {
                // Ownership index: a None old hash means the index did not
                // exist before this transaction; a digest means the backup
                // holds the previous index bytes.
                match (&plan.old_hash, plan.backup_done) {
                    (None, _) => {
                        let _ = std::fs::remove_file(&target);
                    }
                    (Some(_), true) => {
                        if let Ok(backup) = find_backup(&self.backup_dir, &path) {
                            std::fs::copy(&backup, &target)?;
                        }
                    }
                    (Some(_), false) => {}
                }
                continue;
            }

            match (&plan.old_hash, &plan.new_hash) {
                // Created: remove it only if it still matches our bytes
                // (someone may have modified the file after the failure).
                (None, Some(new_hash)) => {
                    if let Ok(actual) = compute_file_hash(&target).await {
                        if actual.eq_ignore_ascii_case(new_hash) {
                            std::fs::remove_file(&target)?;
                        }
                    }
                }
                // Replaced: restore the backup. If the target no longer
                // matches our installed digest either, a third party wrote to
                // the path after the failure; refuse to guess and let the
                // caller mark the transaction needs_recovery.
                (Some(_), Some(new_hash)) => {
                    let backup = find_backup(&self.backup_dir, &path)?;
                    let target_state = if target.exists() {
                        compute_file_hash(&target).await.ok()
                    } else {
                        None
                    };
                    let matches_installed = target_state
                        .as_ref()
                        .is_some_and(|actual| actual.eq_ignore_ascii_case(new_hash));
                    let target_missing = !target.exists();
                    if !matches_installed && !target_missing {
                        return Err(MsvcKitError::Other(format!(
                            "Rollback conflict for {path}: the file no longer matches the                              installed digest {new_hash} and the original bytes are in the                              transaction backup; manual resolution required"
                        )));
                    }
                    if target_missing {
                        std::fs::rename(&backup, &target)?;
                    } else {
                        std::fs::copy(&backup, &target)?;
                        std::fs::remove_file(&backup)?;
                    }
                }
                // Backup intent without an install: the target still holds
                // the original bytes (the rename never completed) unless the
                // rename completed but the event was lost - in which case the
                // backup file exists and must be restored.
                (Some(_), None) => {
                    if !target.exists() {
                        let backup = find_backup(&self.backup_dir, &path)?;
                        std::fs::rename(&backup, &target)?;
                    }
                }
                (None, None) => {}
            }
        }

        // The installed manifest of a rolled-back transaction is void.
        let _ = std::fs::remove_file(installed_manifest_path(&self.root, &self.tx_id));
        Ok(())
    }

    fn mark_needs_recovery(&mut self, reason: &str) -> Result<()> {
        let marker = serde_json::json!({
            "schema": TRANSACTION_JOURNAL_SCHEMA,
            "transaction_id": self.tx_id,
            "state": "needs_recovery",
            "reason": reason,
            "recorded_at": chrono::Utc::now(),
        });
        atomic_write(
            &self.tx_dir.join(NEEDS_RECOVERY_FILE),
            serde_json::to_vec_pretty(&marker)?.as_slice(),
        )
    }
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
                    recorded_at,
                });
            }
        }
    }
    Ok(events)
}

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
    if !tx_root.is_dir() {
        return Ok(report);
    }
    for entry in std::fs::read_dir(&tx_root)? {
        let entry = entry?;
        let tx_dir = entry.path();
        if !tx_dir.is_dir() {
            continue;
        }
        report.transactions_examined += 1;
        let tx_id = entry.file_name().to_string_lossy().into_owned();
        let journal = tx_dir.join(JOURNAL_FILE);

        if tx_dir.join(NEEDS_RECOVERY_FILE).exists() {
            report.needs_recovery.push(tx_id);
            continue;
        }
        if !journal.exists() {
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
            if backup.is_dir() {
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
                let _ = tx.append_event(JournalEventKind::RolledBack, "recovery", None);
                let _ = std::fs::remove_dir_all(&tx_dir);
                report.rolled_back.push(tx_id);
            }
            Err(error) => {
                let _ = tx.mark_needs_recovery(&error.to_string());
                report.needs_recovery.push(tx_id);
            }
        }
    }
    Ok(report)
}
