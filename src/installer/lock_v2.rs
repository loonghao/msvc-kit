//! Toolchain lock v2 and installed-tree verification (spec 06 §3, P07).
//!
//! v2 extends the v1 selection lock with what a rebuild needs beyond
//! selection: pruning provenance and the digest of the installed manifest the
//! producing transaction left behind. v1 files stay readable through the v1
//! reader (unchanged); a v2 file is only ever produced explicitly, and a lock
//! without receipts is `selection-only` - it must never claim source pinning.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::error::{MsvcKitError, Result};
use crate::toolchain_lock::{ToolchainLock, LOCK_V2_SCHEMA};

/// Toolchain lock v2: v1 fields plus install-outcome provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainLockV2 {
    pub schema: String,
    /// The v1-compatible selection core (schema field inside is ignored on
    /// parse and pinned to v1 on write, so v1 readers can still load the
    /// selection after stripping this file's extra keys... they cannot:
    /// v1 is deny_unknown_fields. v1 compat means the v1 READER still reads
    /// v1 FILES; v2 files declare themselves via `schema`).
    #[serde(flatten)]
    pub core: LockedSelectionCore,
    /// Provenance status: `source-pinned` (receipts present) or
    /// `selection-only` (external install, no receipts).
    pub provenance: String,
    /// Pruning provenance from the producing install: removed paths with
    /// rule ids/reasons.
    #[serde(default)]
    pub pruning: Vec<PruningRecord>,
    /// SHA256 over the installed-manifest document written by the producing
    /// transaction, when available.
    pub installed_manifest_digest: Option<String>,
    /// MSI payloads and the CAB source media they require (relative names).
    #[serde(default)]
    pub cab_relations: Vec<CabRelation>,
}

/// v1-compatible selection fields (flattened into v2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedSelectionCore {
    pub msvc_version: String,
    pub sdk_version: String,
    pub arch: crate::version::Architecture,
    pub host_arch: crate::version::Architecture,
    pub fingerprint: String,
    pub receipts: Vec<crate::toolchain_lock::InstallationReceipt>,
}

/// One pruning record (removed path with its rule provenance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PruningRecord {
    /// Path relative to the install root.
    pub path: String,
    pub rule_id: String,
    pub rule_version: u32,
    pub reason: String,
}

/// An MSI payload and the CAB source media it needs beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CabRelation {
    pub msi_file: String,
    pub cab_files: Vec<String>,
}

impl ToolchainLockV2 {
    pub const SCHEMA: &'static str = LOCK_V2_SCHEMA;

    /// Build a v2 lock from a v1 lock plus install provenance.
    pub fn from_v1(
        lock: &ToolchainLock,
        provenance: LockProvenance,
        pruning: Vec<PruningRecord>,
        installed_manifest_digest: Option<String>,
        cab_relations: Vec<CabRelation>,
    ) -> Self {
        Self {
            schema: Self::SCHEMA.to_string(),
            core: LockedSelectionCore {
                msvc_version: lock.msvc_version.clone(),
                sdk_version: lock.sdk_version.clone(),
                arch: lock.arch,
                host_arch: lock.host_arch,
                fingerprint: lock.fingerprint.clone(),
                receipts: lock.receipts.clone(),
            },
            provenance: provenance.as_str().to_string(),
            pruning,
            installed_manifest_digest,
            cab_relations,
        }
    }

    /// Whether the lock claims exact source pinning (receipts present and
    /// verified). `selection-only` locks describe what to select, not where
    /// the bytes come from.
    pub fn is_source_pinned(&self) -> bool {
        self.provenance == LockProvenance::SourcePinned.as_str() && !self.core.receipts.is_empty()
    }

    /// Parse + validate from a string (tests and callers with in-memory JSON).
    pub fn load_from_str(text: &str) -> Result<Self> {
        let lock: Self = serde_json::from_str(text)?;
        lock.validate()?;
        Ok(lock)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let lock: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        lock.validate()?;
        Ok(lock)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        crate::storage::atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != Self::SCHEMA {
            return Err(MsvcKitError::Config(format!(
                "Unsupported toolchain lock schema: {}",
                self.schema
            )));
        }
        let provenances: [LockProvenance; 2] =
            [LockProvenance::SourcePinned, LockProvenance::SelectionOnly];
        if !provenances
            .iter()
            .any(|provenance| provenance.as_str() == self.provenance)
        {
            return Err(MsvcKitError::Config(format!(
                "Unknown lock provenance {:?}",
                self.provenance
            )));
        }
        let claims_pinned = self.provenance == LockProvenance::SourcePinned.as_str();
        let has_receipts = !self.core.receipts.is_empty();
        if claims_pinned != has_receipts {
            return Err(MsvcKitError::Config(
                "Lock provenance contradicts its receipts: source-pinned locks need receipts, \
                 selection-only locks must not claim them"
                    .to_string(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for record in &self.pruning {
            if !seen.insert(record.path.clone()) {
                return Err(MsvcKitError::Config(format!(
                    "Duplicate pruning record for {}",
                    record.path
                )));
            }
        }
        Ok(())
    }
}

/// Lock provenance (spec 06 §3.2: source-pinned / selection-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockProvenance {
    /// Receipts present: the exact bytes are pinned.
    SourcePinned,
    /// External install: selection only, no receipts.
    SelectionOnly,
}

impl LockProvenance {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SourcePinned => "source-pinned",
            Self::SelectionOnly => "selection-only",
        }
    }
}

/// One detected difference between the installed manifest and the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledDiff {
    /// File exists on disk with different bytes.
    Modified { path: String },
    /// File in the manifest is missing on disk.
    Missing { path: String },
    /// File on disk (relative path) that no manifest entry owns.
    Extra { path: String },
}

/// Verify the installed tree against an installed-manifest document.
///
/// Files are re-hashed (no size-only shortcuts). Extra files under the root
/// are reported only as informational diffs - verification never deletes
/// anything (I-001).
pub async fn verify_installed_tree(
    root: &Path,
    manifest: &crate::installer::contracts::InstalledManifest,
) -> Result<Vec<InstalledDiff>> {
    let mut diffs = Vec::new();
    let mut manifest_paths: BTreeSet<String> = BTreeSet::new();

    for entry in &manifest.files {
        manifest_paths.insert(entry.path.clone());
        // Manifest paths are stored with '/'; join them with the platform's
        // own separator. Hard-coding '\\' makes every lookup miss on
        // non-Windows hosts (the whole path becomes one filename on Linux).
        let absolute = root.join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !absolute.is_file() {
            diffs.push(InstalledDiff::Missing {
                path: entry.path.clone(),
            });
            continue;
        }
        let actual = crate::downloader::hash::compute_file_hash(&absolute).await?;
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            diffs.push(InstalledDiff::Modified {
                path: entry.path.clone(),
            });
        }
    }

    // Extra files: anything on disk (not under msvc-kit control dirs) that no
    // manifest entry owns.
    let skip_prefixes = [".msvc-kit", ".msvc-kit-extracted", "downloads"];
    walk_extra(root, root, &skip_prefixes, &manifest_paths, &mut diffs)?;
    Ok(diffs)
}

fn walk_extra(
    root: &Path,
    dir: &Path,
    skip_prefixes: &[&str],
    manifest_paths: &BTreeSet<String>,
    diffs: &mut Vec<InstalledDiff>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let relative = match path.strip_prefix(root) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        let rel_text = relative.to_string_lossy().replace('\\', "/");
        if skip_prefixes
            .iter()
            .any(|prefix| rel_text.to_lowercase().starts_with(prefix))
        {
            continue;
        }
        if path.is_dir() {
            walk_extra(root, &path, skip_prefixes, manifest_paths, diffs)?;
        } else if !manifest_paths.contains(&rel_text) {
            diffs.push(InstalledDiff::Extra { path: rel_text });
        }
    }
    Ok(())
}

/// Load the installed manifest for an install id (or the latest one when
/// `install_id` is None).
pub fn load_installed_manifest(
    root: &Path,
    install_id: Option<&str>,
) -> Result<crate::installer::contracts::InstalledManifest> {
    let dir = root.join(".msvc-kit").join("installed-manifests");
    let path = match install_id {
        Some(id) => dir.join(format!("{id}.json")),
        None => {
            let mut latest: Option<(SystemTime, PathBuf)> = None;
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let modified = entry.metadata()?.modified()?;
                if latest.as_ref().is_none_or(|(time, _)| modified > *time) {
                    latest = Some((modified, entry.path()));
                }
            }
            latest.map(|(_, path)| path).ok_or_else(|| {
                MsvcKitError::Config(format!("No installed manifests under {}", dir.display()))
            })?
        }
    };
    let manifest: crate::installer::contracts::InstalledManifest =
        serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(manifest)
}

/// Detect installed-tree mutation (missing/modified/extra files) against the
/// latest (or given) installed manifest.
pub async fn detect_installed_mutation(
    root: &Path,
    install_id: Option<&str>,
) -> Result<Vec<InstalledDiff>> {
    let manifest = load_installed_manifest(root, install_id)?;
    verify_installed_tree(root, &manifest).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installer::contracts::{InstalledFileEntry, InstalledManifest};
    use crate::toolchain_lock::InstallationReceipt;

    fn v1_lock_json() -> String {
        serde_json::json!({
            "schema": "msvc-kit.toolchain-lock.v1",
            "msvc_version": "14.44.34823",
            "sdk_version": "10.0.26100.0",
            "arch": "x64",
            "host_arch": "x64",
            "fingerprint": crate::env::toolchain_fingerprint(
                Some("14.44.34823"),
                Some("10.0.26100.0"),
                crate::version::Architecture::X64,
                crate::version::Architecture::X64,
            ),
            "receipts": []
        })
        .to_string()
    }

    fn v2_from_v1() -> ToolchainLockV2 {
        let lock: ToolchainLock = serde_json::from_str(&v1_lock_json()).unwrap();
        ToolchainLockV2::from_v1(
            &lock,
            LockProvenance::SelectionOnly,
            vec![PruningRecord {
                path: "VC/Tools/MSVC/14.44.34823/bin/HostX64/x64/vctip.exe".to_string(),
                rule_id: "msvc.telemetry.vctip".to_string(),
                rule_version: 1,
                reason: "telemetry".to_string(),
            }],
            Some("sha256:manifest".to_string()),
            vec![CabRelation {
                msi_file: "Windows SDK Headers-x86.msi".to_string(),
                cab_files: vec!["source.cab".to_string()],
            }],
        )
    }

    #[test]
    fn v1_lock_file_is_still_readable_by_the_v1_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.v1.json");
        std::fs::write(&path, v1_lock_json()).unwrap();
        let lock = ToolchainLock::load(&path).unwrap();
        assert_eq!(lock.msvc_version, "14.44.34823");
        assert!(lock.receipts.is_empty());
    }

    #[test]
    fn v2_round_trips_and_carries_provenance() {
        let v2 = v2_from_v1();
        let text = serde_json::to_string_pretty(&v2).unwrap();
        let parsed = ToolchainLockV2::load_from_str(&text).unwrap();
        assert_eq!(parsed.schema, "msvc-kit.toolchain-lock.v2");
        assert!(
            !parsed.is_source_pinned(),
            "selection-only without receipts"
        );
        assert_eq!(parsed.pruning.len(), 1);
        assert_eq!(parsed.cab_relations.len(), 1);
        assert_eq!(
            parsed.installed_manifest_digest.as_deref(),
            Some("sha256:manifest")
        );
    }

    #[test]
    fn source_pinned_requires_receipts() {
        let mut v2 = v2_from_v1();
        v2.provenance = LockProvenance::SourcePinned.as_str().to_string();
        // receipts empty + source-pinned => contradiction rejected
        assert!(ToolchainLockV2::load_from_str(&serde_json::to_string(&v2).unwrap()).is_err());

        // With a receipt the provenance is consistent.
        v2.core.receipts.push(InstallationReceipt {
            schema: crate::toolchain_lock::InstallationReceipt::SCHEMA.to_string(),
            component: "msvc".to_string(),
            version: "14.44.34823".to_string(),
            arch: crate::version::Architecture::X64,
            host_arch: crate::version::Architecture::X64,
            vs_channel: None,
            components: vec![],
            payloads: vec![crate::toolchain_lock::LockedPayload {
                file_name: "a.vsix".to_string(),
                url: "https://example.com/a.vsix".to_string(),
                size: 1,
                sha256: "ab".repeat(32),
            }],
        });
        let parsed = ToolchainLockV2::load_from_str(&serde_json::to_string(&v2).unwrap()).unwrap();
        assert!(parsed.is_source_pinned());
    }

    #[test]
    fn unknown_schema_and_provenance_are_rejected() {
        let mut v2 = v2_from_v1();
        v2.schema = "msvc-kit.toolchain-lock.v9".to_string();
        assert!(ToolchainLockV2::load_from_str(&serde_json::to_string(&v2).unwrap()).is_err());
        let mut v2 = v2_from_v1();
        v2.provenance = "pinned-by-vibes".to_string();
        assert!(ToolchainLockV2::load_from_str(&serde_json::to_string(&v2).unwrap()).is_err());
    }

    #[tokio::test]
    async fn installed_tree_mutation_is_detected() {
        let root = tempfile::tempdir().unwrap();
        let good_rel = "VC/Tools/x/cl.exe";
        let good_abs = root.path().join(good_rel);
        std::fs::create_dir_all(good_abs.parent().unwrap()).unwrap();
        std::fs::write(&good_abs, b"original").unwrap();
        let good_sha = crate::downloader::hash::compute_file_hash(&good_abs)
            .await
            .unwrap();

        let manifest = InstalledManifest {
            schema: "msvc-kit.installed-manifest.v1".to_string(),
            install_id: "tx-1".to_string(),
            files: vec![InstalledFileEntry {
                path: good_rel.to_string(),
                size: 8,
                sha256: good_sha,
                owners: vec!["pkg".to_string()],
                kind: "file".to_string(),
            }],
        };

        // Pristine: no diffs.
        assert!(verify_installed_tree(root.path(), &manifest)
            .await
            .unwrap()
            .is_empty());

        // Modified file detected.
        std::fs::write(&good_abs, b"tampered").unwrap();
        let diffs = verify_installed_tree(root.path(), &manifest).await.unwrap();
        assert!(diffs.contains(&InstalledDiff::Modified {
            path: good_rel.to_string()
        }));

        // Missing file detected.
        std::fs::remove_file(&good_abs).unwrap();
        let diffs = verify_installed_tree(root.path(), &manifest).await.unwrap();
        assert!(diffs.contains(&InstalledDiff::Missing {
            path: good_rel.to_string()
        }));

        // Extra unmanaged file detected (but msvc-kit control dirs skipped).
        std::fs::create_dir_all(root.path().join(".msvc-kit")).unwrap();
        std::fs::write(root.path().join(".msvc-kit").join("internal.json"), b"{}").unwrap();
        std::fs::write(root.path().join("user-note.txt"), b"mine").unwrap();
        let diffs = verify_installed_tree(root.path(), &manifest).await.unwrap();
        assert!(
            diffs.contains(&InstalledDiff::Extra {
                path: "user-note.txt".to_string()
            }),
            "{diffs:?}"
        );
        assert!(!diffs.iter().any(
            |diff| matches!(diff, InstalledDiff::Extra { path } if path.starts_with(".msvc-kit"))
        ));
    }

    #[tokio::test]
    async fn detect_installed_mutation_loads_latest_manifest() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(".msvc-kit").join("installed-manifests");
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = InstalledManifest {
            schema: "msvc-kit.installed-manifest.v1".to_string(),
            install_id: "tx-2".to_string(),
            files: vec![InstalledFileEntry {
                path: "missing.txt".to_string(),
                size: 0,
                sha256: "ab".repeat(32),
                owners: vec![],
                kind: "file".to_string(),
            }],
        };
        std::fs::write(
            dir.join("tx-2.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let diffs = detect_installed_mutation(root.path(), None).await.unwrap();
        assert!(
            matches!(diffs.first(), Some(InstalledDiff::Missing { path }) if path == "missing.txt")
        );
    }
}
