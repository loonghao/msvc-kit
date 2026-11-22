//! Minimal data contracts for the install pipeline (spec 03 §3).
//!
//! These documents are internal for now (`pub(crate)`): they give P02+ a
//! stable serialization target without yet committing the public library API
//! to evolving shapes. P01 delivers the schemas and their golden tests ahead
//! of their consumers (plan/transaction/ownership code lands in P02/P03), so
//! the unused-item lint is silenced at the module root until then.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub(crate) const INSTALL_PLAN_SCHEMA: &str = "msvc-kit.install-plan.v1";
pub(crate) const INSTALL_RECEIPT_SCHEMA: &str = "msvc-kit.install-receipt.v1";
pub(crate) const OWNERSHIP_INDEX_SCHEMA: &str = "msvc-kit.ownership.v1";
pub(crate) const TRANSACTION_JOURNAL_SCHEMA: &str = "msvc-kit.transaction-journal.v1";

/// The install action kinds an [`InstallPlanDocument`] can express (spec 03
/// §3.3). Unknown kinds fail deserialization rather than being ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlanActionKind {
    DownloadPayload,
    ReuseCachedPayload,
    ReuseInstalledComponent,
    ExtractPackage,
    RetainSourceMedia,
    PrunePath,
    CreateDirectory,
    WriteMetadata,
    BackupOwnedFile,
    CommitFile,
    SkipOptionalPackage,
    Conflict,
}

/// One planned step with its dependency references and estimates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanAction {
    /// Stable identifier, unique inside the plan.
    pub id: String,
    pub kind: PlanActionKind,
    /// IDs of actions that must complete before this one starts.
    pub depends_on: Vec<String>,
    /// Package that owns the action, when applicable.
    pub package_id: Option<String>,
    /// Root-relative path the action concerns, when applicable.
    pub path: Option<String>,
    /// Expected SHA256 of the payload/file, when known.
    pub expected_sha256: Option<String>,
    /// Estimated bytes this action consumes or reclaims, when known.
    pub estimated_bytes: Option<u64>,
    pub reason: String,
    /// Risk note surfaced to plan reviewers.
    pub risk: Option<String>,
}

/// Normalized request summary recorded inside a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanRequest {
    pub profile: Option<String>,
    pub provider: String,
    pub host_arch: String,
    pub target_arches: Vec<String>,
}

/// Resolved selection summary recorded inside a plan (spec 04 §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolvedSummary {
    pub msvc_version: String,
    pub sdk_version: String,
    pub channel: Option<String>,
    pub manifest_digest: Option<String>,
    pub packages: Vec<String>,
}

/// Facts an apply run revalidates before executing a plan (spec 03 §3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanObservations {
    /// Channel manifest digest the resolution depended on.
    pub manifest_digest: Option<String>,
    /// Root-relative paths already owned by a managed install, with digests.
    pub owned_paths_digest: Option<String>,
    /// Normalized target root the plan was computed against.
    pub target_root: String,
    /// Free-space snapshot; apply re-checks anyway.
    pub free_space_bytes: Option<u64>,
}

/// Byte estimates with explicit unknowns (spec 04 §7). `None` means
/// "cannot estimate"; zero is only ever a measured fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiskUsageEstimate {
    pub download_bytes: u64,
    pub reused_cache_bytes: u64,
    pub staging_bytes: Option<u64>,
    pub backup_bytes: Option<u64>,
    pub peak_extra_bytes: Option<u64>,
    pub final_delta_bytes: Option<i64>,
    /// `exact` / `bounded` / `unknown`.
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrivilegeRequirement {
    pub requires_elevation: bool,
    pub reason: String,
}

/// The canonical install plan document (`msvc-kit.install-plan.v1`).
///
/// `plan_id` is derived from the canonical serialization of the plan without
/// `created_at`; it is verified, not recomputed, on load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallPlanDocument {
    pub schema: String,
    pub plan_id: String,
    /// Recorded wall-clock time; excluded from plan identity.
    pub created_at: DateTime<Utc>,
    pub request: PlanRequest,
    pub resolved: ResolvedSummary,
    pub observations: PlanObservations,
    pub actions: Vec<PlanAction>,
    pub disk_usage: DiskUsageEstimate,
    pub privilege: PrivilegeRequirement,
    pub warnings: Vec<String>,
    pub blocking_conflicts: Vec<String>,
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

    /// Round-trip every document through its golden JSON so field names,
    /// casing and optionality stay stable across refactors.
    #[test]
    fn install_plan_golden_round_trip() {
        let json = r#"{
          "schema": "msvc-kit.install-plan.v1",
          "plan_id": "sha256:abcdef",
          "created_at": "2026-11-22T00:00:00Z",
          "request": {
            "profile": "cpp-desktop",
            "provider": "msvc",
            "host_arch": "x64",
            "target_arches": ["x64"]
          },
          "resolved": {
            "msvc_version": "14.44.34823",
            "sdk_version": "10.0.26100.0",
            "channel": "Visual Studio 2026 (v18)",
            "manifest_digest": "sha256:1234",
            "packages": ["Microsoft.VC.14.44.Tools.HostX64.TargetX64"]
          },
          "observations": {
            "manifest_digest": "sha256:1234",
            "owned_paths_digest": "sha256:5678",
            "target_root": "C:/tools/msvc",
            "free_space_bytes": 107374182400
          },
          "actions": [
            {
              "id": "download-1",
              "kind": "download_payload",
              "depends_on": [],
              "package_id": "Microsoft.VC.14.44.Tools.HostX64.TargetX64",
              "path": null,
              "expected_sha256": "aa",
              "estimated_bytes": 1048576,
              "reason": "not in cache",
              "risk": null
            },
            {
              "id": "extract-1",
              "kind": "extract_package",
              "depends_on": ["download-1"],
              "package_id": null,
              "path": "VC/Tools/MSVC",
              "expected_sha256": null,
              "estimated_bytes": null,
              "reason": "staged extraction",
              "risk": null
            }
          ],
          "disk_usage": {
            "download_bytes": 1048576,
            "reused_cache_bytes": 0,
            "staging_bytes": null,
            "backup_bytes": null,
            "peak_extra_bytes": null,
            "final_delta_bytes": null,
            "confidence": "bounded"
          },
          "privilege": {"requires_elevation": false, "reason": "user-writable target"},
          "warnings": [],
          "blocking_conflicts": []
        }"#;

        let plan: InstallPlanDocument = serde_json::from_str(json).unwrap();
        assert_eq!(plan.schema, INSTALL_PLAN_SCHEMA);
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.actions[0].kind, PlanActionKind::DownloadPayload);
        assert_eq!(plan.actions[1].kind, PlanActionKind::ExtractPackage);
        assert_eq!(plan.actions[1].depends_on, vec!["download-1".to_string()]);
        // created_at must not break round-trip equality of the parsed value.
        let reencoded = serde_json::to_string(&plan).unwrap();
        let reparsed: InstallPlanDocument = serde_json::from_str(&reencoded).unwrap();
        assert_eq!(plan, reparsed);
        assert_eq!(
            serde_json::to_value(&plan).unwrap()["disk_usage"]["staging_bytes"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn install_plan_rejects_unknown_fields_and_bad_kinds() {
        let base = r#"{
          "schema": "msvc-kit.install-plan.v1",
          "plan_id": "sha256:abcdef",
          "created_at": "2026-11-22T00:00:00Z",
          "request": {"profile": null, "provider": "msvc", "host_arch": "x64", "target_arches": ["x64"]},
          "resolved": {"msvc_version": "14.44", "sdk_version": "10.0.26100.0", "channel": null, "manifest_digest": null, "packages": []},
          "observations": {"manifest_digest": null, "owned_paths_digest": null, "target_root": "C:/t", "free_space_bytes": null},
          "actions": [],
          "disk_usage": {"download_bytes": 0, "reused_cache_bytes": 0, "staging_bytes": null, "backup_bytes": null, "peak_extra_bytes": null, "final_delta_bytes": null, "confidence": "unknown"},
          "privilege": {"requires_elevation": false, "reason": "r"},
          "warnings": [],
          "blocking_conflicts": []
        }"#;

        let with_unknown: serde_json::Value =
            serde_json::from_str::<serde_json::Value>(base).unwrap();
        let mut value = with_unknown.clone();
        value["unexpected_field"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<InstallPlanDocument>(value).is_err());

        let mut value = with_unknown.clone();
        value["actions"] = serde_json::json!([{
            "id": "x",
            "kind": "format_disk",
            "depends_on": [],
            "package_id": null,
            "path": null,
            "expected_sha256": null,
            "estimated_bytes": null,
            "reason": "no",
            "risk": null
        }]);
        assert!(serde_json::from_value::<InstallPlanDocument>(value).is_err());

        let mut value = with_unknown;
        value["schema"] = serde_json::Value::String("msvc-kit.install-plan.v9".into());
        assert!(serde_json::from_value::<InstallPlanDocument>(value).is_ok());
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
