//! Common download functionality shared between MSVC and SDK downloaders

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use futures::{stream, StreamExt};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use tokio::{io::AsyncWriteExt, sync::RwLock, time::sleep};
use tracing::debug;

use super::hash::compute_file_hash;
use super::progress::{BoxedProgressHandler, IndicatifProgressHandler};
use super::traits::BoxedCacheManager;
use super::{DownloadIndex, DownloadOptions, DownloadStatus, IndexEntry, Package, PackagePayload};
use crate::constants::download as dl_const;
use crate::error::{MsvcKitError, Result};

/// Common downloader with shared functionality
pub struct CommonDownloader {
    pub options: DownloadOptions,
    pub client: Client,
    pub progress_handler: Option<BoxedProgressHandler>,
    /// Custom cache manager for manifest / payload caching
    pub cache_manager: Option<BoxedCacheManager>,
}

#[derive(Debug, Clone, Copy)]
enum PayloadOutcome {
    Skipped,
    Downloaded,
}

#[derive(Debug)]
struct PayloadResult {
    path: PathBuf,
    transferred: u64,
    outcome: PayloadOutcome,
}

impl CommonDownloader {
    /// Create a new common downloader with a custom HTTP client
    pub fn with_client(options: DownloadOptions, client: Client) -> Self {
        Self {
            options,
            client,
            progress_handler: None,
            cache_manager: None,
        }
    }

    /// Set a custom progress handler
    pub fn with_progress_handler(mut self, handler: BoxedProgressHandler) -> Self {
        self.progress_handler = Some(handler);
        self
    }

    /// Set a custom cache manager for manifest and payload caching
    pub fn with_cache_manager(mut self, manager: BoxedCacheManager) -> Self {
        self.cache_manager = Some(manager);
        self
    }

    /// Get the manifest cache directory.
    ///
    /// Resolution order: an injected cache manager (`cache_dir/manifests`), the
    /// explicit `manifest_cache_dir` from the download options, and finally the
    /// platform default location.
    pub fn manifest_cache_dir(&self) -> PathBuf {
        if let Some(ref cm) = self.cache_manager {
            return cm.cache_dir().join("manifests");
        }
        if let Some(ref dir) = self.options.manifest_cache_dir {
            return dir.clone();
        }
        super::cache::default_manifest_cache_dir()
    }

    /// Get the manifest cache directory, falling back to the configuration.
    ///
    /// Same resolution order as [`Self::manifest_cache_dir`], except that a
    /// missing `manifest_cache_dir` resolves to
    /// [`MsvcKitConfig::manifest_cache_dir`](crate::MsvcKitConfig::manifest_cache_dir)
    /// instead of the platform default. An unconfigured installation resolves
    /// to the platform default either way, so the default behaviour is
    /// unchanged.
    pub fn configured_manifest_cache_dir(&self) -> PathBuf {
        if self.cache_manager.is_some() || self.options.manifest_cache_dir.is_some() {
            return self.manifest_cache_dir();
        }
        super::configured_manifest_cache_dir()
    }

    /// Download packages with progress display and local index for fast skip
    pub async fn download_packages(
        &self,
        packages: &[Package],
        download_dir: &Path,
        component_name: &str,
    ) -> Result<Vec<PathBuf>> {
        let mut all_payloads: Vec<PackagePayload> = Vec::new();
        for payload in packages.iter().flat_map(|package| &package.payloads) {
            payload_path(download_dir, &payload.file_name)?;
            if let Some(existing) = all_payloads
                .iter()
                .find(|existing| existing.file_name == payload.file_name)
            {
                if existing.url != payload.url
                    || existing.size != payload.size
                    || existing.sha256 != payload.sha256
                {
                    return Err(MsvcKitError::Config(format!(
                        "Conflicting payload identities for {}",
                        payload.file_name
                    )));
                }
            } else {
                all_payloads.push(payload.clone());
            }
        }

        let total_files = all_payloads.len();
        let total_size: u64 = all_payloads.iter().map(|p| p.size).sum();

        // Use custom progress handler or create default
        let progress_handler: BoxedProgressHandler = self
            .progress_handler
            .clone()
            .unwrap_or_else(|| Arc::new(IndicatifProgressHandler::new(total_size)));

        // Lock before opening redb and retain ownership through all downloads/index writes.
        let lock_path = download_dir.join(".download.lock");
        let _cache_lock =
            tokio::task::spawn_blocking(move || crate::storage::lock_file(&lock_path))
                .await
                .map_err(|error| MsvcKitError::Other(error.to_string()))??;
        let index_path = download_dir.join("index.db");
        let index = DownloadIndex::load(&index_path).await?;
        let index = Arc::new(RwLock::new(index));

        progress_handler.on_start(component_name, total_files, total_size);
        progress_handler.on_progress(0);
        let processed = Arc::new(AtomicUsize::new(0));
        let skipped = Arc::new(AtomicUsize::new(0));
        let downloaded = Arc::new(AtomicUsize::new(0));

        let max_concurrency = self.options.parallel_downloads.max(1);
        let mut current_concurrency = max_concurrency;

        let mut downloaded_files = Vec::with_capacity(all_payloads.len());
        let mut index_pos = 0;

        // Track consecutive low-throughput batches for smarter adaptation
        let mut low_throughput_streak = 0usize;

        while index_pos < all_payloads.len() {
            let end = (index_pos + current_concurrency).min(all_payloads.len());
            let batch: Vec<_> = all_payloads[index_pos..end].to_vec();

            let batch_start = Instant::now();
            let mut batch_bytes = 0u64;

            let results = stream::iter(batch.into_iter().map(|payload| {
                let progress = progress_handler.clone();
                let verify_hashes = self.options.verify_hashes;
                let index = index.clone();
                let client = self.client.clone();
                let download_dir = download_dir.to_path_buf();
                async move {
                    download_single_payload_with_handler(
                        &client,
                        &payload,
                        &download_dir,
                        &index,
                        &progress,
                        verify_hashes,
                    )
                    .await
                }
            }))
            .buffer_unordered(current_concurrency)
            .collect::<Vec<_>>()
            .await;

            for res in results {
                match res {
                    Ok(r) => {
                        processed.fetch_add(1, Ordering::Relaxed);

                        match r.outcome {
                            PayloadOutcome::Skipped => {
                                skipped.fetch_add(1, Ordering::Relaxed);
                            }
                            PayloadOutcome::Downloaded => {
                                downloaded.fetch_add(1, Ordering::Relaxed);
                            }
                        }

                        downloaded_files.push(r.path);
                        batch_bytes += r.transferred;
                    }
                    Err(e) => {
                        progress_handler.on_error(&e.to_string());
                        return Err(e);
                    }
                }
            }

            // Update summary message
            let p = processed.load(Ordering::Relaxed);
            let s = skipped.load(Ordering::Relaxed);
            let d = downloaded.load(Ordering::Relaxed);
            progress_handler.on_message(&format!(
                "{}/{} files | dl {} | skip {} | conc {}",
                p, total_files, d, s, current_concurrency
            ));

            let batch_duration = batch_start.elapsed().as_secs_f64().max(0.001);
            let throughput_mbps = (batch_bytes as f64 / batch_duration) / 1_000_000.0;

            // Smarter adaptive heuristic using constants
            if throughput_mbps < dl_const::LOW_THROUGHPUT_MBPS {
                low_throughput_streak += 1;
                if low_throughput_streak >= dl_const::LOW_THROUGHPUT_STREAK_THRESHOLD
                    && current_concurrency > dl_const::MIN_CONCURRENCY
                {
                    current_concurrency -= 1;
                    low_throughput_streak = 0;
                }
            } else if throughput_mbps > dl_const::HIGH_THROUGHPUT_MBPS {
                low_throughput_streak = 0;
                if current_concurrency < max_concurrency {
                    current_concurrency += 1;
                }
            } else {
                low_throughput_streak = low_throughput_streak.saturating_sub(1);
            }

            debug!(
                "Batch {}-{} throughput {:.1} MB/s, next concurrency {} (max {})",
                index_pos, end, throughput_mbps, current_concurrency, max_concurrency
            );

            index_pos = end;
        }

        progress_handler.on_complete(
            downloaded.load(Ordering::Relaxed),
            skipped.load(Ordering::Relaxed),
        );

        Ok(downloaded_files)
    }
}

/// Resolve safe payload paths under the cache root.
fn payload_path(download_dir: &Path, name: &str) -> Result<PathBuf> {
    let relative = Path::new(name);
    if name.is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(MsvcKitError::InstallPath(format!(
            "Invalid payload file name: {name}"
        )));
    }
    Ok(download_dir.join(relative))
}

fn same_source(entry: &IndexEntry, payload: &PackagePayload) -> bool {
    entry.url == payload.url
        && entry.size == payload.size
        && match (&entry.sha256, &payload.sha256) {
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            (None, None) => true,
            _ => false,
        }
}

/// Cached bytes are evidence only after checking current source identity and disk contents.
async fn validated_cache_hash(
    payload: &PackagePayload,
    path: &Path,
    entry: Option<&IndexEntry>,
    verify_hashes: bool,
) -> Result<Option<String>> {
    if let Some(entry) = entry {
        if entry.status != DownloadStatus::Completed || !same_source(entry, payload) {
            return Ok(None);
        }
    } else if !verify_hashes || payload.sha256.is_none() {
        // With no index identity and no expected digest, size alone cannot identify a payload.
        return Ok(None);
    }
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file()
        || (metadata.len() != payload.size && (!verify_hashes || payload.sha256.is_none()))
    {
        return Ok(None);
    }
    if verify_hashes {
        let actual = compute_file_hash(path).await?;
        let expected = payload.sha256.as_ref();
        return Ok(expected
            .filter(|expected| actual.eq_ignore_ascii_case(expected))
            .map(|_| actual));
    }
    // Verification disabled still requires an indexed source identity and disk size.
    Ok(Some(
        entry
            .and_then(|entry| entry.computed_hash.clone())
            .unwrap_or_default(),
    ))
}

/// Download or reuse a payload while retaining old final bytes until replacement is validated.
async fn download_single_payload_with_handler(
    client: &Client,
    payload: &PackagePayload,
    download_dir: &Path,
    index: &Arc<RwLock<DownloadIndex>>,
    progress: &BoxedProgressHandler,
    verify_hashes: bool,
) -> Result<PayloadResult> {
    if verify_hashes
        && payload.sha256.as_ref().is_none_or(|digest| {
            digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err(MsvcKitError::Config(format!(
            "Missing or invalid official payload SHA256 for {}",
            payload.file_name
        )));
    }
    let file_path = payload_path(download_dir, &payload.file_name)?;
    let cached = { index.read().await.get_entry(&payload.file_name).await? };
    if let Some(hash) =
        validated_cache_hash(payload, &file_path, cached.as_ref(), verify_hashes).await?
    {
        index
            .write()
            .await
            .mark_completed(payload, file_path.clone(), Some(hash))
            .await?;
        progress.on_progress(payload.size);
        progress.on_file_complete(&payload.file_name, "cached");
        return Ok(PayloadResult {
            path: file_path,
            transferred: 0,
            outcome: PayloadOutcome::Skipped,
        });
    }
    progress.on_file_start(&payload.file_name, payload.size);
    let (hash, actual_size) =
        download_file_with_streaming_hash(client, payload, &file_path, progress, verify_hashes)
            .await?;
    index
        .write()
        .await
        .mark_completed(payload, file_path.clone(), Some(hash))
        .await?;
    progress.on_file_complete(&payload.file_name, "downloaded");
    Ok(PayloadResult {
        path: file_path,
        transferred: actual_size,
        outcome: PayloadOutcome::Downloaded,
    })
}

/// Stream into a private sibling file, verify, then publish atomically.
async fn download_file_with_streaming_hash(
    client: &Client,
    payload: &PackagePayload,
    path: &Path,
    progress: &BoxedProgressHandler,
    verify_hashes: bool,
) -> Result<(String, u64)> {
    let parent = path
        .parent()
        .ok_or_else(|| MsvcKitError::InstallPath("Payload has no cache directory".into()))?;
    tokio::fs::create_dir_all(parent).await?;
    'attempts: for attempt in 0..=dl_const::MAX_RETRIES {
        let response = match client.get(&payload.url).send().await {
            Ok(response) => response,
            Err(error) => {
                if attempt < dl_const::MAX_RETRIES
                    && (error.is_connect() || error.is_timeout() || error.is_body())
                {
                    sleep(Duration::from_secs(1 << attempt)).await;
                    continue;
                }
                return Err(MsvcKitError::DownloadNetwork {
                    file: payload.file_name.clone(),
                    url: payload.url.clone(),
                    source: error,
                });
            }
        };
        if (response.status().is_server_error()
            || response.status() == StatusCode::TOO_MANY_REQUESTS)
            && attempt < dl_const::MAX_RETRIES
        {
            sleep(Duration::from_secs(1 << attempt)).await;
            continue;
        }
        let response =
            response
                .error_for_status()
                .map_err(|source| MsvcKitError::DownloadNetwork {
                    file: payload.file_name.clone(),
                    url: payload.url.clone(),
                    source,
                })?;
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        let mut file = tokio::fs::File::from_std(temporary.reopen()?);
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();
        while let Some(item) = stream.next().await {
            let chunk = match item {
                Ok(chunk) => chunk,
                Err(error) => {
                    drop(file);
                    drop(temporary);
                    if attempt < dl_const::MAX_RETRIES {
                        sleep(Duration::from_secs(1 << attempt)).await;
                        continue 'attempts;
                    }
                    return Err(MsvcKitError::DownloadNetwork {
                        file: payload.file_name.clone(),
                        url: payload.url.clone(),
                        source: error,
                    });
                }
            };
            file.write_all(&chunk).await?;
            hasher.update(&chunk);
            downloaded += chunk.len() as u64;
            progress.on_progress(chunk.len() as u64);
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        let computed_hash = hex::encode(hasher.finalize());
        if verify_hashes {
            if let Some(expected) = &payload.sha256 {
                if !computed_hash.eq_ignore_ascii_case(expected) {
                    return Err(MsvcKitError::HashMismatch {
                        file: payload.file_name.clone(),
                        expected: expected.clone(),
                        actual: computed_hash,
                    });
                }
            }
        }
        if downloaded != payload.size {
            if !verify_hashes || payload.sha256.is_none() {
                return Err(MsvcKitError::Other(format!(
                    "Size mismatch for {}: expected {}, received {}",
                    payload.file_name, payload.size, downloaded
                )));
            }
            // Microsoft publishes some signed VSIX payloads with stale size
            // metadata. A matching authoritative digest identifies the exact
            // bytes; an index's previous digest alone cannot waive this check.
            debug!(
                "Verified {} SHA256 despite manifest size {} differing from actual {}",
                payload.file_name, payload.size, downloaded
            );
        }
        temporary
            .persist(path)
            .map_err(|error| MsvcKitError::Io(error.error))?;
        return Ok((computed_hash, downloaded));
    }
    Err(MsvcKitError::Other(format!(
        "Download failed for {} after {} retries",
        payload.file_name,
        dl_const::MAX_RETRIES
    )))
}
