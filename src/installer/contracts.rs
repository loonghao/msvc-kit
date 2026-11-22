//! Minimal internal data contracts for the install pipeline (spec 03 §3).
//!
//! The install plan document lives in [`super::plan`] and is public (the CLI
//! plans and applies it). These documents are internal for now
//! (`pub(crate)`): they give P03+ a stable serialization target without yet
//! committing the public library API to evolving shapes. They land ahead of
//! their consumers (transaction and ownership code lands in P03), so the
//! unused-item lint is silenced at the module root until then.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub(crate) const INSTALL_RECEIPT_SCHEMA: &str = "msvc-kit.install-receipt.v1";
pub(crate) const OWNERSHIP_INDEX_SCHEMA: &str = "msvc-kit.ownership.v1";
pub(crate) const TRANSACTION_JOURNAL_SCHEMA: &str = "msvc-kit.transaction-journal.v1";
pub(crate) const INSTALLED_MANIFEST_SCHEMA: &str = "msvc-kit.installed-manifest.v1";

/// One file of the final installed tree (spec 03 §3.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstalledFileEntry {
    /// Root-relative path (forward slashes for stable cross-tool records).
    pub path: String,
    pub size: u64,
    pub sha256: String,
    /// Packages that produced this file in the latest install.
    pub owners: Vec<String>,
    /// `file` today; directory existence is not part of integrity.
    pub kind: String,
}

/// The installed manifest document (`msvc-kit.installed-manifest.v1`): the
/// authoritative per-file digest record of what the last install left on
/// disk, used for ownership decisions and later integrity verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstalledManifest {
    pub schema: String,
    /// Install (transaction) identifier that produced this manifest.
    pub install_id: String,
    pub files: Vec<InstalledFileEntry>,
}

/// Per-file ownership record (spec 03 §3.6). Several owners may reference the
/// same path only when they agree on its SHA256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnershipEntry {
    /// Root-relative path, lowercase-canonical for stable indexing.
    pub path: String,
    /// Current SHA256 of the file on disk, when present.
    pub sha256: Option<String>,
    /// Kind: `file` today; directories are recorded only for existence.
    pub kind: String,
    /// Install IDs that currently reference this path.
    pub owners: Vec<String>,
}

/// The ownership index document (`msvc-kit.ownership.v1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnershipIndexDocument {
    pub schema: String,
    pub entries: Vec<OwnershipEntry>,
}

/// Journal transaction state machine (spec 02 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TransactionState {
    Created,
    Planned,
    Acquiring,
    Acquired,
    Extracting,
    Extracted,
    Pruning,
    Validating,
    ReadyToCommit,
    Committing,
    Committed,
    Failed,
    Aborted,
    RollingBack,
    RolledBack,
    NeedsRecovery,
}

/// Journal event kinds (spec 03 §3.7); write-ahead events precede the file
/// operations they describe, completion events follow them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JournalEventKind {
    WillBackup,
    BackupDone,
    WillInstall,
    InstallDone,
    FileRemoved,
    ReceiptCommitted,
    Committed,
    RolledBack,
}

/// One journal record with its sequence number for replay ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JournalEvent {
    pub sequence: u64,
    pub kind: JournalEventKind,
    /// Root-relative path the event concerns.
    pub path: String,
    /// Expected hash before (Will*) or after (Done/Committed) the operation.
    pub hash: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

/// The transaction journal document (`msvc-kit.transaction-journal.v1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TransactionJournalDocument {
    pub schema: String,
    pub transaction_id: String,
    pub plan_id: String,
    /// Normalized target root identity the transaction mutates.
    pub target_root: String,
    /// Staging directory (same volume as `target_root`).
    pub staging_path: String,
    /// Backup directory for replaced owned files.
    pub backup_path: String,
    pub state: TransactionState,
    pub events: Vec<JournalEvent>,
    pub updated_at: DateTime<Utc>,
}

/// Result of one applied install (`msvc-kit.install-receipt.v1`).
///
/// This extends the acquisition provenance in
/// [`crate::toolchain_lock::InstallationReceipt`] with transaction outcome and
/// installed-tree identity; it records what happened, never what was asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallReceiptDocument {
    pub schema: String,
    pub install_id: String,
    pub transaction_id: String,
    pub component: String,
    pub version: String,
    pub arch: String,
    pub host_arch: String,
    /// Source payload identities actually consumed (downloaded or reused).
    pub source_payloads: Vec<SourcePayload>,
    /// Extractor backend identifier actually used (e.g. `msiexec-a`).
    pub extractor_backend: String,
    /// Pruning profile and rule version applied.
    pub pruning: Option<String>,
    /// SHA256 over the installed manifest document.
    pub installed_manifest_digest: Option<String>,
    /// Final commit status of the transaction.
    pub commit_status: String,
    pub completed_at: DateTime<Utc>,
}

/// One consumed payload in an install receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourcePayload {
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// `downloaded` or `reused-cache`.
    pub acquisition: String,
}

#[cfg(test)]
mod golden_tests {
    use super::*;

    #[test]
    fn installed_manifest_golden_round_trip() {
        let json = r#"{
          "schema": "msvc-kit.installed-manifest.v1",
          "install_id": "tx-1",
          "files": [
            {
              "path": "VC/Tools/MSVC/14.44/bin/Hostx64/x64/cl.exe",
              "size": 123456,
              "sha256": "abc",
              "owners": ["Microsoft.VC.14.44.Tools.HostX64.TargetX64"],
              "kind": "file"
            }
          ]
        }"#;
        let manifest: InstalledManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema, INSTALLED_MANIFEST_SCHEMA);
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].size, 123456);
        let reencoded = serde_json::to_string(&manifest).unwrap();
        assert_eq!(
            manifest,
            serde_json::from_str::<InstalledManifest>(&reencoded).unwrap()
        );
        let mut value =
            serde_json::from_value::<serde_json::Value>(serde_json::to_value(&manifest).unwrap())
                .unwrap();
        value["extra"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<InstalledManifest>(value).is_err());
    }

    #[test]
    fn ownership_index_golden_round_trip() {
        let json = r#"{
          "schema": "msvc-kit.ownership.v1",
          "entries": [
            {
              "path": "vc/tools/msvc/14.44/bin/hostx64/x64/cl.exe",
              "sha256": "abc",
              "kind": "file",
              "owners": ["install-1", "install-2"]
            },
            {
              "path": "windows kits/10/include/10.0.26100.0/um/windows.h",
              "sha256": null,
              "kind": "file",
              "owners": ["install-1"]
            }
          ]
        }"#;
        let index: OwnershipIndexDocument = serde_json::from_str(json).unwrap();
        assert_eq!(index.entries.len(), 2);
        assert_eq!(index.entries[0].owners.len(), 2);
        assert!(index.entries[1].sha256.is_none());
        let reencoded = serde_json::to_string(&index).unwrap();
        assert_eq!(
            index,
            serde_json::from_str::<OwnershipIndexDocument>(&reencoded).unwrap()
        );
        // Unknown fields are refused.
        let mut value =
            serde_json::from_value::<serde_json::Value>(serde_json::to_value(&index).unwrap())
                .unwrap();
        value["extra"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<OwnershipIndexDocument>(value).is_err());
    }

    #[test]
    fn transaction_journal_golden_round_trip() {
        let json = r#"{
          "schema": "msvc-kit.transaction-journal.v1",
          "transaction_id": "tx-1",
          "plan_id": "sha256:plan",
          "target_root": "C:/tools/msvc",
          "staging_path": "C:/tools/msvc/.msvc-kit/transactions/tx-1/staging",
          "backup_path": "C:/tools/msvc/.msvc-kit/transactions/tx-1/backup",
          "state": "committing",
          "events": [
            {
              "sequence": 1,
              "kind": "will_backup",
              "path": "vc/tools/msvc/14.44/bin/cl.exe",
              "hash": "old",
              "recorded_at": "2026-11-22T00:00:01Z"
            },
            {
              "sequence": 2,
              "kind": "backup_done",
              "path": "vc/tools/msvc/14.44/bin/cl.exe",
              "hash": null,
              "recorded_at": "2026-11-22T00:00:02Z"
            }
          ],
          "updated_at": "2026-11-22T00:00:03Z"
        }"#;
        let journal: TransactionJournalDocument = serde_json::from_str(json).unwrap();
        assert_eq!(journal.state, TransactionState::Committing);
        assert_eq!(journal.events[0].kind, JournalEventKind::WillBackup);
        assert_eq!(journal.events[1].kind, JournalEventKind::BackupDone);
        let reencoded = serde_json::to_string(&journal).unwrap();
        assert_eq!(
            journal,
            serde_json::from_str::<TransactionJournalDocument>(&reencoded).unwrap()
        );
        // Every state name from the spec round-trips.
        for state in [
            TransactionState::Created,
            TransactionState::Planned,
            TransactionState::Acquiring,
            TransactionState::Acquired,
            TransactionState::Extracting,
            TransactionState::Extracted,
            TransactionState::Pruning,
            TransactionState::Validating,
            TransactionState::ReadyToCommit,
            TransactionState::Committing,
            TransactionState::Committed,
            TransactionState::Failed,
            TransactionState::Aborted,
            TransactionState::RollingBack,
            TransactionState::RolledBack,
            TransactionState::NeedsRecovery,
        ] {
            let text = serde_json::to_string(&state).unwrap();
            assert_eq!(
                state,
                serde_json::from_str::<TransactionState>(&text).unwrap()
            );
        }
    }

    #[test]
    fn install_receipt_golden_round_trip() {
        let json = r#"{
          "schema": "msvc-kit.install-receipt.v1",
          "install_id": "install-1",
          "transaction_id": "tx-1",
          "component": "msvc",
          "version": "14.44.34823",
          "arch": "x64",
          "host_arch": "x64",
          "source_payloads": [
            {
              "file_name": "Microsoft.VC.14.44.Tools.HostX64.TargetX64.vsix",
              "url": "https://download.visualstudio.microsoft.com/download/pr/x/x.vsix",
              "sha256": "abc",
              "size": 1048576,
              "acquisition": "downloaded"
            }
          ],
          "extractor_backend": "zip-vsix",
          "pruning": "portable@1",
          "installed_manifest_digest": "sha256:tree",
          "commit_status": "committed",
          "completed_at": "2026-11-22T00:00:04Z"
        }"#;
        let receipt: InstallReceiptDocument = serde_json::from_str(json).unwrap();
        assert_eq!(receipt.schema, INSTALL_RECEIPT_SCHEMA);
        assert_eq!(receipt.source_payloads[0].acquisition, "downloaded");
        assert_eq!(
            receipt.installed_manifest_digest.as_deref(),
            Some("sha256:tree")
        );
        let reencoded = serde_json::to_string(&receipt).unwrap();
        assert_eq!(
            receipt,
            serde_json::from_str::<InstallReceiptDocument>(&reencoded).unwrap()
        );
    }
}
