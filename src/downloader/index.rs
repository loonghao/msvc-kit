use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use tokio::task;

use crate::error::{MsvcKitError, Result};

const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("download_index");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DownloadStatus {
    Completed,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub file_name: String,
    pub url: String,
    pub size: u64,
    /// Expected SHA256 from manifest (may be None)
    pub sha256: Option<String>,
    /// Computed SHA256 after download (always set for completed files)
    #[serde(default)]
    pub computed_hash: Option<String>,
    pub local_path: PathBuf,
    pub status: DownloadStatus,
    #[serde(default)]
    pub bytes_downloaded: u64,
    #[serde(default)]
    pub hash_verified: bool,
    pub updated_at: DateTime<Utc>,
}

/// redb-based download index (single-file, crash-safe)
pub struct DownloadIndex {
    db: Arc<Database>,
    /// Path to the database file (used for debugging and diagnostics)
    path: PathBuf,
}

impl DownloadIndex {
    /// Get the path to the database file
    pub fn db_path(&self) -> &Path {
        &self.path
    }
}

impl DownloadIndex {
    /// Load or create index database at the given path (uses .db extension)
    pub async fn load(path: &Path) -> Result<Self> {
        let db_path = if path.extension().is_some() {
            path.to_path_buf()
        } else {
            path.with_extension("db")
        };

        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // redb prefers forward slashes; ensure consistent path string
        let db_path_str = db_path.to_string_lossy().replace('\\', "/");
        let db_path_clone = db_path.clone();

        let db_exists = db_path_clone.exists();
        let db: Database = task::spawn_blocking(move || -> Result<Database> {
            let builder = Database::builder();
            if db_exists {
                // Try opening existing DB first
                match builder.open(db_path_str.as_str()) {
                    Ok(db) => {
                        tracing::info!("Index DB opened: {:?}", db_path_clone);
                        Ok(db)
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Index DB open failed, backing up and recreating: {:?}, err={}",
                            db_path_clone,
                            e
                        );

                        // If corrupted, back it up and recreate
                        let mut backup = db_path_clone.clone();
                        backup.set_extension("db.bak");
                        std::fs::rename(&db_path_clone, &backup)
                            .map_err(|ioe| MsvcKitError::Database(ioe.to_string()))?;
                        builder
                            .create(db_path_str.as_str())
                            .map_err(|db_err| MsvcKitError::Database(db_err.to_string()))
                    }
                }
            } else {
                tracing::info!("Index DB creating: {:?}", db_path_clone);
                builder
                    .create(db_path_str.as_str())
                    .map_err(|db_err| MsvcKitError::Database(db_err.to_string()))
            }
        })
        .await
        .map_err(|je| MsvcKitError::Database(je.to_string()))??;

        // Ensure table exists
        let db_arc = Arc::new(db);
        let db_clone = db_arc.clone();
        let _ = task::spawn_blocking(move || -> Result<()> {
            let tx = db_clone
                .begin_write()
                .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            {
                let _ = tx
                    .open_table(TABLE)
                    .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            }
            tx.commit()
                .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|je| MsvcKitError::Database(je.to_string()))?;

        // Debug: count existing entries
        let db_clone = db_arc.clone();
        let _ = task::spawn_blocking(move || -> Result<()> {
            let tx = db_clone
                .begin_read()
                .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            if let Ok(table) = tx.open_table(TABLE) {
                let count = table
                    .len()
                    .map_err(|e| MsvcKitError::Database(e.to_string()))?;
                let mut with_hash = 0u64;
                let mut without_hash = 0u64;
                for item in table
                    .iter()
                    .map_err(|e| MsvcKitError::Database(e.to_string()))?
                {
                    let (_, val) = item.map_err(|e| MsvcKitError::Database(e.to_string()))?;
                    let entry: IndexEntry =
                        bincode::serde::decode_from_slice(val.value(), bincode::config::standard())
                            .map_err(|e| MsvcKitError::Database(e.to_string()))?
                            .0;
                    if entry.computed_hash.is_some() {
                        with_hash += 1;
                    } else {
                        without_hash += 1;
                    }
                }
                tracing::info!(
                    "Index DB ready: total={}, with_hash={}, without_hash={}",
                    count,
                    with_hash,
                    without_hash
                );
            } else {
                tracing::info!("Index table missing, will be created on first write");
            }

            Ok(())
        })
        .await
        .map_err(|je| MsvcKitError::Database(je.to_string()))?;

        Ok(Self {
            db: db_arc,
            path: db_path,
        })
    }

    pub async fn get_entry(&self, file_name: &str) -> Result<Option<IndexEntry>> {
        let db = self.db.clone();
        let key = file_name.to_string();
        let result = task::spawn_blocking(move || -> Result<Option<IndexEntry>> {
            let tx = db
                .begin_read()
                .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            let table = match tx.open_table(TABLE) {
                Ok(t) => t,
                Err(_) => return Ok(None),
            };
            let maybe_bytes = table
                .get(key.as_str())
                .map_err(|e| MsvcKitError::Database(e.to_string()))?
                .map(|value| value.value().to_vec());
            drop(table);
            drop(tx);
            if let Some(bytes) = maybe_bytes {
                let entry: IndexEntry =
                    bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
                        .map_err(|e| MsvcKitError::Database(e.to_string()))?
                        .0;
                Ok(Some(entry))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|je| MsvcKitError::Database(je.to_string()))?;
        result
    }

    pub async fn upsert_entry(&mut self, entry: &IndexEntry) -> Result<()> {
        let db = self.db.clone();
        let entry = entry.clone();
        let result = task::spawn_blocking(move || -> Result<()> {
            let tx = db
                .begin_write()
                .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            {
                let mut table = tx
                    .open_table(TABLE)
                    .map_err(|e| MsvcKitError::Database(e.to_string()))?;
                let bytes = bincode::serde::encode_to_vec(&entry, bincode::config::standard())
                    .map_err(|e| MsvcKitError::Database(e.to_string()))?;
                table
                    .insert(entry.file_name.as_str(), bytes.as_slice())
                    .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            }
            tx.commit()
                .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|je| MsvcKitError::Database(je.to_string()))?;
        result
    }

    pub async fn remove(&mut self, file_name: &str) -> Result<()> {
        let db = self.db.clone();
        let key = file_name.to_string();
        let result = task::spawn_blocking(move || -> Result<()> {
            let tx = db
                .begin_write()
                .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            {
                if let Ok(mut table) = tx.open_table(TABLE) {
                    let _ = table.remove(key.as_str());
                }
            }
            tx.commit()
                .map_err(|e| MsvcKitError::Database(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|je| MsvcKitError::Database(je.to_string()))?;
        result
    }

    /// Check if entry exists and is identical (fast skip)
    pub async fn is_entry_unchanged(
        &self,
        file_name: &str,
        expected_status: DownloadStatus,
        expected_size: u64,
        expected_hash: &Option<String>,
        expected_path: &Path,
    ) -> Result<bool> {
        if let Some(entry) = self.get_entry(file_name).await? {
            Ok(entry.status == expected_status
                && entry.size == expected_size
                && entry.computed_hash == *expected_hash
                && entry.local_path == expected_path)
        } else {
            Ok(false)
        }
    }

    pub async fn mark_completed(
        &mut self,
        payload: &crate::downloader::PackagePayload,
        local_path: PathBuf,
        computed_hash: Option<String>,
    ) -> Result<()> {
        if self
            .is_entry_unchanged(
                &payload.file_name,
                DownloadStatus::Completed,
                payload.size,
                &computed_hash,
                &local_path,
            )
            .await?
        {
            return Ok(());
        }

        let hash_verified = match (&computed_hash, &payload.sha256) {
            (Some(computed), Some(expected)) => computed.eq_ignore_ascii_case(expected),
            (Some(_), None) => true,
            _ => false,
        };

        let entry = IndexEntry {
            file_name: payload.file_name.clone(),
            url: payload.url.clone(),
            size: payload.size,
            sha256: payload.sha256.clone(),
            computed_hash,
            local_path,
            status: DownloadStatus::Completed,
            bytes_downloaded: payload.size,
            hash_verified,
            updated_at: Utc::now(),
        };
        self.upsert_entry(&entry).await
    }

    /// Deferred version kept for compatibility; performs immediate upsert
    pub fn mark_completed_deferred(
        &mut self,
        payload: &crate::downloader::PackagePayload,
        local_path: PathBuf,
        computed_hash: Option<String>,
    ) -> bool {
        // Fire-and-forget: spawn async task reusing the same DB handle
        let db = self.db.clone();
        let payload = payload.clone();
        tokio::spawn(async move {
            let mut idx = DownloadIndex {
                db,
                path: PathBuf::new(),
            };
            let _ = idx
                .mark_completed(&payload, local_path, computed_hash)
                .await;
        });
        true
    }

    pub async fn mark_partial(
        &mut self,
        payload: &crate::downloader::PackagePayload,
        local_path: PathBuf,
        bytes_downloaded: u64,
    ) -> Result<()> {
        let entry = IndexEntry {
            file_name: payload.file_name.clone(),
            url: payload.url.clone(),
            size: payload.size,
            sha256: payload.sha256.clone(),
            computed_hash: None,
            local_path,
            status: DownloadStatus::Partial,
            bytes_downloaded,
            hash_verified: false,
            updated_at: Utc::now(),
        };
        self.upsert_entry(&entry).await
    }

    pub fn is_dirty(&self) -> bool {
        // redb transactions are durable; no dirty tracking needed
        false
    }
}
