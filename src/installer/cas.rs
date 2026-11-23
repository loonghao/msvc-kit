//! Content-addressed blob cache (spec 06 §2).
//!
//! Blobs are keyed by `sha256:<lowercase-64-hex>` and stored as
//! `blobs/sha256/<2-hex>/<full-sha256>` under the cache root. Publication is
//! atomic (streamed to `partial/<digest>.part`, fsynced, then renamed into
//! place), so a concurrent reader never observes a half-written blob and two
//! publishers of the same content converge on one file. Corruption found
//! during verification quarantines the blob instead of serving it (I-007),
//! and a mark-and-sweep GC with dry-run support never deletes pinned or
//! referenced blobs.
//!
//! The store coexists with the per-version download layout: existing
//! downloads are imported in place (re-runnable), and the downloader keeps
//! writing there for compatibility; the CAS is the deduplication and
//! integrity layer on top.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::downloader::hash::compute_file_hash;
use crate::error::{MsvcKitError, Result};
use crate::storage::atomic_write;

pub(crate) const BLOBS_DIR: &str = "blobs/sha256";
const PARTIAL_DIR: &str = "partial";
const QUARANTINE_DIR: &str = "quarantine";
const INDEX_FILE: &str = "cache.redb";
const PINS_FILE: &str = "pins.json";
const SOURCE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("blob_sources");

/// One source location a blob was imported from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobSourceRef {
    /// Absolute path of the source file at import time.
    pub path: PathBuf,
    pub size: u64,
    /// When the reference was recorded (RFC 3339).
    pub recorded_at: String,
}

/// A pinned blob: referenced by an active install, plan or lock. Pinned
/// blobs are never swept (spec 06 §2.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlobPins {
    /// Blobs referenced by an active transaction/plan/lock, by digest.
    pub pinned: std::collections::BTreeSet<String>,
    /// Minimum age (seconds) a blob must have before GC may sweep it.
    pub min_age_seconds: u64,
}

/// The content-addressed blob store.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
    db: Arc<Database>,
}

impl BlobStore {
    /// Open (or create) the store under `root` (e.g. the configured cache
    /// directory). Layout: `blobs/sha256/<xx>/`, `partial/`, `quarantine/`,
    /// `cache.redb`, `pins.json`.
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root.join(BLOBS_DIR))?;
        std::fs::create_dir_all(root.join(PARTIAL_DIR))?;
        std::fs::create_dir_all(root.join(QUARANTINE_DIR))?;
        let db_path = root.join(INDEX_FILE);
        let db = if db_path.exists() {
            Database::builder()
                .open(&db_path)
                .map_err(|error| MsvcKitError::Database(error.to_string()))
        } else {
            Database::builder()
                .create(&db_path)
                .map_err(|error| MsvcKitError::Database(error.to_string()))
        }
        .or_else(|_| {
            // A corrupted index never loses blobs: rename it aside and
            // start fresh; imports re-register sources on the next run.
            let backup = root.join(format!("{INDEX_FILE}.bak"));
            let _ = std::fs::rename(&db_path, &backup);
            Database::builder()
                .open(&db_path)
                .map_err(|error| MsvcKitError::Database(error.to_string()))
        })?;
        {
            let tx = db
                .begin_write()
                .map_err(|error| MsvcKitError::Database(error.to_string()))?;
            tx.open_table(SOURCE_TABLE)
                .map_err(|error| MsvcKitError::Database(error.to_string()))?;
            tx.commit()
                .map_err(|error| MsvcKitError::Database(error.to_string()))?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            db: Arc::new(db),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Digest validation for user-supplied keys.
    pub fn validate_digest(digest: &str) -> Result<String> {
        let lower = digest
            .trim()
            .trim_start_matches("sha256:")
            .to_ascii_lowercase();
        if lower.len() != 64 || !lower.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(MsvcKitError::Config(format!(
                "Invalid blob digest {digest:?}; expected 64 hex characters"
            )));
        }
        Ok(lower)
    }

    pub fn blob_path(&self, digest: &str) -> PathBuf {
        let digest = digest.to_ascii_lowercase();
        self.root.join(BLOBS_DIR).join(&digest[..2]).join(&digest)
    }

    /// Whether the blob file exists (not necessarily verified).
    pub fn contains_blob(&self, digest: &str) -> bool {
        self.blob_path(digest).is_file()
    }

    /// Read the source references recorded for a digest.
    pub async fn sources(&self, digest: &str) -> Result<Vec<BlobSourceRef>> {
        let key = Self::validate_digest(digest)?;
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<BlobSourceRef>> {
            let tx = db
                .begin_read()
                .map_err(|error| MsvcKitError::Database(error.to_string()))?;
            let table = tx
                .open_table(SOURCE_TABLE)
                .map_err(|error| MsvcKitError::Database(error.to_string()))?;
            let Some(value) = table
                .get(key.as_str())
                .map_err(|error| MsvcKitError::Database(error.to_string()))?
            else {
                return Ok(Vec::new());
            };
            let refs: Vec<BlobSourceRef> =
                bincode::serde::decode_from_slice(value.value(), bincode::config::standard())
                    .map_err(|error| MsvcKitError::Database(error.to_string()))?
                    .0;
            Ok(refs)
        })
        .await
        .map_err(|error| MsvcKitError::Database(error.to_string()))?
    }

    async fn record_source(&self, digest: &str, source: BlobSourceRef) -> Result<()> {
        let key = Self::validate_digest(digest)?;
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let tx = db
                .begin_write()
                .map_err(|error| MsvcKitError::Database(error.to_string()))?;
            {
                let mut table = tx
                    .open_table(SOURCE_TABLE)
                    .map_err(|error| MsvcKitError::Database(error.to_string()))?;
                let refs: Vec<BlobSourceRef> = match table
                    .get(key.as_str())
                    .map_err(|error| MsvcKitError::Database(error.to_string()))?
                {
                    Some(value) => {
                        bincode::serde::decode_from_slice(
                            value.value(),
                            bincode::config::standard(),
                        )
                        .map_err(|error| MsvcKitError::Database(error.to_string()))?
                        .0
                    }
                    None => Vec::new(),
                };
                if !refs.iter().any(|reference| reference.path == source.path) {
                    let mut updated = refs;
                    updated.push(source);
                    let bytes =
                        bincode::serde::encode_to_vec(&updated, bincode::config::standard())
                            .map_err(|error| MsvcKitError::Database(error.to_string()))?;
                    table
                        .insert(key.as_str(), bytes.as_slice())
                        .map_err(|error| MsvcKitError::Database(error.to_string()))?;
                }
            }
            tx.commit()
                .map_err(|error| MsvcKitError::Database(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| MsvcKitError::Database(error.to_string()))?
    }

    /// Import a file into the store: hash it, and if a blob with that digest
    /// is absent, publish it atomically (stream to a `.part`, fsync, rename
    /// into place). Re-runnable: importing the same file twice reports
    /// `AlreadyPresent` and never rewrites the blob.
    pub async fn import_file(&self, path: &Path) -> Result<ImportOutcome> {
        let digest = compute_file_hash(path).await?;
        let size = tokio::fs::metadata(path).await?.len();
        let blob = self.blob_path(&digest);
        if blob.is_file() {
            self.record_source(
                &digest,
                BlobSourceRef {
                    path: path.to_path_buf(),
                    size,
                    recorded_at: chrono::Utc::now().to_rfc3339(),
                },
            )
            .await?;
            return Ok(ImportOutcome::AlreadyPresent { digest, size });
        }

        // Stream to a .part, fsync, then atomically publish. Concurrent
        // imports converge: same digest means identical bytes, so either
        // publisher's outcome is correct.
        let partial = self.root.join(PARTIAL_DIR).join(format!("{digest}.part"));
        tokio::fs::copy(path, &partial)
            .await
            .map_err(|error| MsvcKitError::Other(format!("copy to partial failed: {error}")))?;
        tokio::fs::OpenOptions::new()
            .write(true)
            .open(&partial)
            .await?
            .sync_all()
            .await
            .map_err(|error| MsvcKitError::Other(format!("partial sync failed: {error}")))?;
        if let Some(parent) = blob.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if tokio::fs::rename(&partial, &blob).await.is_err() {
            let _ = tokio::fs::remove_file(&partial).await;
            if !blob.is_file() {
                return Err(MsvcKitError::Other(format!(
                    "Failed to publish blob {digest}: concurrent publication conflicted"
                )));
            }
        }
        self.record_source(
            &digest,
            BlobSourceRef {
                path: path.to_path_buf(),
                size,
                recorded_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await?;
        Ok(ImportOutcome::Imported { digest, size })
    }

    /// Re-hash a blob and quarantine it when the bytes no longer match the
    /// digest (spec I-007: corrupt content is never served). Returns whether
    /// the blob is intact afterwards.
    pub async fn verify_blob(&self, digest: &str) -> Result<bool> {
        let key = Self::validate_digest(digest)?;
        let blob = self.blob_path(&key);
        if !blob.is_file() {
            return Ok(false);
        }
        let actual = compute_file_hash(&blob).await?;
        if actual.eq_ignore_ascii_case(&key) {
            return Ok(true);
        }
        let quarantine = self.root.join(QUARANTINE_DIR).join(format!(
            "{}-{}",
            &key[..12],
            chrono::Utc::now().timestamp()
        ));
        tokio::fs::rename(&blob, &quarantine).await?;
        tracing::warn!(
            "Blob {} failed verification (found {}); quarantined to {}",
            key,
            actual,
            quarantine.display()
        );
        Ok(false)
    }

    /// Load the pins document, or defaults.
    /// Load the pins document. Fail-closed (G-2): if pins.json exists but
    /// is corrupt, this returns an error rather than silently treating all
    /// pins as absent (which would let GC sweep pinned blobs).
    pub fn pins(&self) -> Result<BlobPins> {
        let path = self.root.join(PINS_FILE);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                MsvcKitError::Database(format!(
                    "pins.json is corrupt ({}): {} - refusing to GC without valid pins",
                    path.display(),
                    error
                ))
            }),
            Err(ref io_error) if io_error.kind() == std::io::ErrorKind::NotFound => {
                Ok(BlobPins::default())
            }
            Err(error) => Err(MsvcKitError::Io(error)),
        }
    }

    /// Persist the pins document.
    pub fn save_pins(&self, pins: &BlobPins) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(pins)?;
        atomic_write(&self.root.join(PINS_FILE), &bytes)
    }

    /// Register a pin for a digest (active install/plan/lock reference).
    pub fn pin(&self, digest: &str) -> Result<()> {
        let key = Self::validate_digest(digest)?;
        let mut pins = self.pins()?;
        pins.pinned.insert(key);
        self.save_pins(&pins)
    }

    /// Enumerate all blob digests present in the store.
    pub fn list_blobs(&self) -> Result<Vec<(String, u64)>> {
        list_blobs_at(&self.root)
    }

    /// Mark-and-sweep garbage collection (spec 06 §2.6). Dry run reports
    /// what would be removed without touching anything. Never follows
    /// symlinks out of the cache root and never sweeps pinned blobs.
    pub async fn gc(&self, dry_run: bool) -> Result<GcReport> {
        let root = self.root.clone();
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || gc_sync(&root, &db, dry_run))
            .await
            .map_err(|error| MsvcKitError::Database(error.to_string()))?
    }
}

/// Result of an import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    Imported { digest: String, size: u64 },
    AlreadyPresent { digest: String, size: u64 },
}

/// GC summary (spec 06 §2.6).
#[derive(Debug, Clone, Default)]
pub struct GcReport {
    /// (digest, size) removed — or that would be removed in dry-run.
    pub swept: Vec<(String, u64)>,
    /// Blobs kept because they are younger than the retention window.
    pub retained_young: Vec<(String, u64)>,
    pub needs_recovery: Vec<String>,
    pub errors: Vec<String>,
}

/// Synchronous mark-and-sweep body (runs on the blocking pool).
fn gc_sync(root: &Path, db: &Database, dry_run: bool) -> Result<GcReport> {
    let pins: BlobPins = std::fs::read(root.join(PINS_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    let mut report = GcReport::default();

    // Mark: blobs with at least one source reference whose file still exists
    // are referenced; explicit pins add on top.
    let mut referenced: std::collections::HashSet<String> = pins.pinned.iter().cloned().collect();
    {
        let tx = db
            .begin_read()
            .map_err(|error| MsvcKitError::Database(error.to_string()))?;
        let table = tx
            .open_table(SOURCE_TABLE)
            .map_err(|error| MsvcKitError::Database(error.to_string()))?;
        for row in table
            .iter()
            .map_err(|error| MsvcKitError::Database(error.to_string()))?
        {
            let (key, value) = row.map_err(|error| MsvcKitError::Database(error.to_string()))?;
            let digest = key.value().to_string();
            let refs: Vec<BlobSourceRef> =
                bincode::serde::decode_from_slice(value.value(), bincode::config::standard())
                    .map_err(|error| MsvcKitError::Database(error.to_string()))?
                    .0;
            if refs
                .iter()
                .any(|reference| Path::new(&reference.path).is_file())
            {
                referenced.insert(digest);
            }
        }
    }

    for (digest, size) in list_blobs_at(root)? {
        let blob = blob_path_at(root, &digest);
        if referenced.contains(&digest) {
            continue;
        }
        let age_ok = std::fs::metadata(&blob)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|age| now - age.as_secs() as i64 >= pins.min_age_seconds as i64)
            .unwrap_or(false);
        if !age_ok {
            report.retained_young.push((digest, size));
            continue;
        }
        if dry_run {
            report.swept.push((digest, size));
            continue;
        }
        match std::fs::remove_file(&blob) {
            Ok(()) => {
                // Drop the index entry after the file is gone: a crash
                // between the two leaves an entry for a missing blob, which
                // reconciliation tolerates.
                let tx = db
                    .begin_write()
                    .map_err(|error| MsvcKitError::Database(error.to_string()))?;
                {
                    let mut table = tx
                        .open_table(SOURCE_TABLE)
                        .map_err(|error| MsvcKitError::Database(error.to_string()))?;
                    table
                        .remove(digest.as_str())
                        .map_err(|error| MsvcKitError::Database(error.to_string()))?;
                }
                tx.commit()
                    .map_err(|error| MsvcKitError::Database(error.to_string()))?;
                report.swept.push((digest, size));
            }
            Err(error) => report
                .errors
                .push(format!("Failed to remove {digest}: {error}")),
        }
    }
    Ok(report)
}

fn blob_path_at(root: &Path, digest: &str) -> PathBuf {
    root.join(BLOBS_DIR).join(&digest[..2]).join(digest)
}

fn list_blobs_at(root: &Path) -> Result<Vec<(String, u64)>> {
    let mut out = Vec::new();
    let blobs = root.join(BLOBS_DIR);
    for shard in std::fs::read_dir(&blobs)? {
        let shard = shard?;
        if !shard.path().is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(shard.path())? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            out.push((name.to_ascii_lowercase(), size));
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn store() -> (tempfile::TempDir, BlobStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        (dir, store)
    }

    fn write_source(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn sha_of_file(path: &Path) -> String {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(path).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        hex::encode(hasher.finalize())
    }

    #[tokio::test]
    async fn same_content_imported_twice_yields_one_blob() {
        let (dir, store) = store();
        let a = write_source(dir.path(), "a.vsix", b"payload");
        let b = write_source(dir.path(), "b.vsix", b"payload");

        let first = store.import_file(&a).await.unwrap();
        let second = store.import_file(&b).await.unwrap();

        let ImportOutcome::Imported { digest, .. } = first else {
            panic!("first import must import");
        };
        assert!(matches!(second, ImportOutcome::AlreadyPresent { .. }));
        assert!(store.contains_blob(&digest));
        assert_eq!(store.list_blobs().unwrap().len(), 1, "one blob per content");

        let refs = store.sources(&digest).await.unwrap();
        assert_eq!(refs.len(), 2, "both source files recorded");
    }

    #[tokio::test]
    async fn verify_quarantines_corrupted_blobs() {
        let (dir, store) = store();
        let source = write_source(dir.path(), "payload.vsix", b"good bytes");
        let outcome = store.import_file(&source).await.unwrap();
        let ImportOutcome::Imported { digest, .. } = outcome else {
            panic!("expected import");
        };

        std::fs::write(store.blob_path(&digest), b"corrupted!!").unwrap();
        assert!(!store.verify_blob(&digest).await.unwrap());
        assert!(!store.contains_blob(&digest), "corrupt blob removed");
        assert!(
            store
                .root()
                .join(QUARANTINE_DIR)
                .read_dir()
                .unwrap()
                .count()
                >= 1,
            "quarantine holds the corrupted bytes"
        );
    }

    #[tokio::test]
    async fn gc_dry_run_changes_nothing_and_respects_pins() {
        let (dir, store) = store();
        let source = write_source(dir.path(), "payload.vsix", b"referenced");
        let other = write_source(dir.path(), "orphan.vsix", b"orphan");
        let referenced = store.import_file(&source).await.unwrap();
        store.import_file(&other).await.unwrap();
        let ImportOutcome::Imported {
            digest: ref_digest, ..
        } = referenced
        else {
            panic!();
        };
        let orphan_digest = sha_of_file(&other);

        // Simulate the orphan's source file being cleaned up: without a live
        // source the blob becomes a GC candidate.
        std::fs::remove_file(&other).unwrap();

        store.pin(&ref_digest).unwrap();

        let report = store.gc(true).await.unwrap();
        let swept: std::collections::HashSet<&str> = report
            .swept
            .iter()
            .map(|(digest, _)| digest.as_str())
            .collect();
        assert!(swept.contains(orphan_digest.as_str()));
        assert!(
            !swept.contains(ref_digest.as_str()),
            "pinned blob never swept"
        );

        // Dry run changed nothing.
        assert!(store.contains_blob(ref_digest.as_str()));
        assert!(store.contains_blob(orphan_digest.as_str()));

        // Real sweep removes the orphan but keeps the pinned blob.
        let report = store.gc(false).await.unwrap();
        assert!(report
            .swept
            .iter()
            .any(|(digest, _)| digest.as_str() == orphan_digest));
        assert!(store.contains_blob(ref_digest.as_str()));
        assert!(!store.contains_blob(orphan_digest.as_str()));

        // Re-running is idempotent.
        let report = store.gc(false).await.unwrap();
        assert!(report.swept.is_empty());
    }

    #[tokio::test]
    async fn gc_never_sweeps_younger_than_retention() {
        let (dir, store) = store();
        let source = write_source(dir.path(), "fresh.vsix", b"fresh");
        store.import_file(&source).await.unwrap();
        store
            .save_pins(&BlobPins {
                pinned: Default::default(),
                min_age_seconds: 3600,
            })
            .unwrap();
        // The source file is gone, so the blob is unreferenced but young.
        std::fs::remove_file(&source).unwrap();
        let report = store.gc(false).await.unwrap();
        assert!(report.swept.is_empty(), "{report:?}");
        assert_eq!(report.retained_young.len(), 1);
        assert_eq!(store.list_blobs().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn migration_is_re_runnable_without_data_loss() {
        let (dir, store) = store();
        let source = write_source(dir.path(), "payload.vsix", b"same bytes");
        let first = store.import_file(&source).await.unwrap();
        let second = store.import_file(&source).await.unwrap();
        assert!(matches!(first, ImportOutcome::Imported { .. }));
        assert!(matches!(second, ImportOutcome::AlreadyPresent { .. }));
        let ImportOutcome::Imported { digest, .. } = first else {
            panic!()
        };
        assert!(store.verify_blob(&digest).await.unwrap());
    }

    #[tokio::test]
    async fn invalid_digests_are_rejected() {
        let _store = store();
        assert!(BlobStore::validate_digest("nope").is_err());
        assert!(BlobStore::validate_digest("sha256:zz").is_err());
        let valid = "a".repeat(64);
        assert_eq!(
            BlobStore::validate_digest(&format!("sha256:{valid}")).unwrap(),
            valid
        );
    }
}
