//! Common download functionality shared between MSVC and SDK downloaders

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use futures::{stream, StreamExt};
use reqwest::{Client, StatusCode};
use tokio::{io::AsyncWriteExt, sync::Mutex, time::sleep};
use tracing::debug;

use super::hash::compute_file_hash;
use super::progress::{BoxedProgressHandler, IndicatifProgressHandler};
use super::{DownloadIndex, DownloadOptions, DownloadStatus, Package, PackagePayload};
use crate::constants::download as dl_const;
use crate::error::{MsvcKitError, Result};

/// Common downloader with shared functionality
pub struct CommonDownloader {
    pub options: DownloadOptions,
    pub client: Client,
    pub progress_handler: Option<BoxedProgressHandler>,
}

#[derive(Debug, Clone, Copy)]
enum PayloadOutcome {
    Skipped,
    Downloaded,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DownloadFileResult;

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
        }
    }

    /// Set a custom progress handler
    pub fn with_progress_handler(mut self, handler: BoxedProgressHandler) -> Self {
        self.progress_handler = Some(handler);
        self
    }

    /// Download packages with progress display and local index for fast skip
    pub async fn download_packages(
        &self,
        packages: &[Package],
        download_dir: &Path,
        component_name: &str,
    ) -> Result<Vec<PathBuf>> {
        let all_payloads: Vec<PackagePayload> =
            packages.iter().flat_map(|p| p.payloads.clone()).collect();

        let total_files = all_payloads.len();
        let total_size: u64 = all_payloads.iter().map(|p| p.size).sum();

        // Use custom progress handler or create default
        let progress_handler: BoxedProgressHandler = self
            .progress_handler
            .clone()
            .unwrap_or_else(|| Arc::new(IndicatifProgressHandler::new(total_size)));

        let index_path = download_dir.join("index.db");
        let index = DownloadIndex::load(&index_path).await?;
        let index = Arc::new(Mutex::new(index));

        // Calculate completed files from index
        let (completed_bytes, completed_count) = self
            .calculate_initial_progress(&all_payloads, download_dir, &index)
            .await?;

        tracing::info!(
            "Index pre-scan: completed={} ({}), remaining={}, total_files={}, total_size={}",
            completed_count,
            humansize::format_size(completed_bytes, humansize::BINARY),
            total_files.saturating_sub(completed_count),
            total_files,
            humansize::format_size(total_size, humansize::BINARY)
        );

        // Initialize progress
        progress_handler.on_start(component_name, total_files, total_size);
        progress_handler.on_progress(completed_bytes);

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

    /// Calculate initial progress from already downloaded files
    async fn calculate_initial_progress(
        &self,
        payloads: &[PackagePayload],
        download_dir: &Path,
        index: &Arc<Mutex<DownloadIndex>>,
    ) -> Result<(u64, usize)> {
        let mut completed_bytes = 0u64;
        let mut completed_count = 0usize;
        let mut debug_logged = 0usize;

        for payload in payloads {
            let cached = {
                let idx = index.lock().await;
                idx.get_entry(&payload.file_name).await?
            };
            let path = download_dir.join(&payload.file_name);

            // Check index for completed files (fast path - trust index with computed_hash)
            if let Some(ref entry) = cached {
                if entry.status == DownloadStatus::Completed {
                    if let Some(ref computed) = entry.computed_hash {
                        let expected = payload.sha256.as_deref();
                        if self.options.verify_hashes {
                            if let Some(exp) = expected {
                                if !computed.eq_ignore_ascii_case(exp) && debug_logged < 10 {
                                    tracing::debug!(
                                        "Indexed hash != manifest, will re-download: file={} computed={} expected={}",
                                        payload.file_name,
                                        computed,
                                        exp
                                    );
                                    debug_logged += 1;
                                }
                            }
                        }

                        let check_path = if tokio::fs::metadata(&path).await.is_ok() {
                            &path
                        } else {
                            &entry.local_path
                        };
                        if tokio::fs::metadata(check_path).await.is_ok() {
                            completed_bytes += payload.size;
                            completed_count += 1;
                            continue;
                        } else if debug_logged < 10 {
                            tracing::debug!(
                                "Indexed file missing on disk, will redownload: file={} path={:?} alt_path={:?}",
                                payload.file_name,
                                path,
                                entry.local_path
                            );
                            debug_logged += 1;
                        }
                    } else if entry.hash_verified || !self.options.verify_hashes {
                        // hash_verified: trust index entry with verified hash
                        // !verify_hashes: skip hash check, trust size match
                        let check_path = if tokio::fs::metadata(&path).await.is_ok() {
                            &path
                        } else {
                            &entry.local_path
                        };
                        if let Ok(meta) = tokio::fs::metadata(check_path).await {
                            if meta.len() == payload.size {
                                completed_bytes += payload.size;
                                completed_count += 1;
                                continue;
                            }
                        }
                    }
                }
            }

            // Check file on disk (may exist without index)
            if let Ok(meta) = tokio::fs::metadata(&path).await {
                if meta.len() == payload.size {
                    completed_bytes += payload.size;
                    completed_count += 1;
                } else if debug_logged < 10 {
                    tracing::debug!(
                        "File exists without matching index size, will redownload: file={} path={:?} actual={} expect={}",
                        payload.file_name,
                        path,
                        meta.len(),
                        payload.size
                    );
                    debug_logged += 1;
                }
            }
        }

        if debug_logged >= 10 {
            tracing::debug!("Logged first 10 mismatch/missing cases; more may exist");
        }

        Ok((completed_bytes, completed_count))
    }
}

/// Download a single payload file with progress handler
async fn download_single_payload_with_handler(
    client: &Client,
    payload: &PackagePayload,
    download_dir: &Path,
    index: &Arc<Mutex<DownloadIndex>>,
    progress: &BoxedProgressHandler,
    verify_hashes: bool,
) -> Result<PayloadResult> {
    let file_path = download_dir.join(&payload.file_name);

    // Fast path: check index for completed file with computed hash
    let cached = {
        let idx = index.lock().await;
        idx.get_entry(&payload.file_name).await?
    };

    if let Some(ref entry) = cached {
        if entry.status == DownloadStatus::Completed {
            let check_path = if tokio::fs::metadata(&file_path).await.is_ok() {
                file_path.clone()
            } else {
                entry.local_path.clone()
            };

            if tokio::fs::metadata(&check_path).await.is_ok() {
                if let Some(ref computed) = entry.computed_hash {
                    if verify_hashes {
                        if let Some(expected) = payload.sha256.as_deref() {
                            if !computed.eq_ignore_ascii_case(expected) {
                                tracing::warn!(
                                    "Cached hash mismatch for {}, re-downloading",
                                    payload.file_name
                                );
                                {
                                    let mut idx = index.lock().await;
                                    let _ = idx.remove(&payload.file_name).await;
                                }
                                let _ = tokio::fs::remove_file(&check_path).await;
                            } else {
                                tracing::debug!(
                                    "Skipping {} (indexed hash, verified)",
                                    payload.file_name
                                );
                                progress.on_file_complete(&payload.file_name, "cached");
                                return Ok(PayloadResult {
                                    path: check_path,
                                    transferred: 0,
                                    outcome: PayloadOutcome::Skipped,
                                });
                            }
                        } else {
                            tracing::debug!(
                                "Skipping {} (indexed hash, no expected)",
                                payload.file_name
                            );
                            progress.on_file_complete(&payload.file_name, "cached");
                            return Ok(PayloadResult {
                                path: check_path,
                                transferred: 0,
                                outcome: PayloadOutcome::Skipped,
                            });
                        }
                    } else {
                        tracing::debug!(
                            "Skipping {} (indexed hash, verify off)",
                            payload.file_name
                        );
                        progress.on_file_complete(&payload.file_name, "cached");
                        return Ok(PayloadResult {
                            path: check_path,
                            transferred: 0,
                            outcome: PayloadOutcome::Skipped,
                        });
                    }
                }
            }
        }
    }

    // Check file on disk (without valid index entry)
    if let Ok(meta) = tokio::fs::metadata(&file_path).await {
        let existing_size = meta.len();

        // File is complete (size matches)
        // Note: size match alone is best-effort, not cryptographically strong
        if existing_size == payload.size {
            let computed_hash = compute_file_hash(&file_path).await?;

            if verify_hashes {
                if let Some(expected_hash) = &payload.sha256 {
                    if !computed_hash.eq_ignore_ascii_case(expected_hash) {
                        tracing::warn!("Hash mismatch for {}, re-downloading", payload.file_name);
                        let _ = tokio::fs::remove_file(&file_path).await;
                    } else {
                        {
                            let mut idx = index.lock().await;
                            idx.mark_completed(payload, file_path.clone(), Some(computed_hash))
                                .await?;
                        }
                        tracing::debug!("Skipping {} (hash computed & matched)", payload.file_name);
                        progress.on_file_complete(&payload.file_name, "size match");
                        return Ok(PayloadResult {
                            path: file_path,
                            transferred: 0,
                            outcome: PayloadOutcome::Skipped,
                        });
                    }
                } else {
                    {
                        let mut idx = index.lock().await;
                        idx.mark_completed(payload, file_path.clone(), Some(computed_hash))
                            .await?;
                    }
                    tracing::debug!(
                        "Skipping {} (hash computed, no expected)",
                        payload.file_name
                    );
                    progress.on_file_complete(&payload.file_name, "size match");
                    return Ok(PayloadResult {
                        path: file_path,
                        transferred: 0,
                        outcome: PayloadOutcome::Skipped,
                    });
                }
            } else {
                {
                    let mut idx = index.lock().await;
                    idx.mark_completed(payload, file_path.clone(), Some(computed_hash))
                        .await?;
                }
                tracing::debug!("Skipping {} (size matched, hash stored)", payload.file_name);
                progress.on_file_complete(&payload.file_name, "size match");
                return Ok(PayloadResult {
                    path: file_path,
                    transferred: 0,
                    outcome: PayloadOutcome::Skipped,
                });
            }
        }

        // File exists but incomplete - delete and restart
        if existing_size > 0 {
            let _ = tokio::fs::remove_file(&file_path).await;
            let mut idx = index.lock().await;
            let _ = idx.remove(&payload.file_name).await;
        }
    }

    // Download the file (full download, no resume)
    debug!("Downloading: {}", payload.file_name);
    progress.on_file_start(&payload.file_name, payload.size);
    download_file_with_handler(client, payload, &file_path, progress).await?;

    // Compute hash after download and store it
    let computed_hash = compute_file_hash(&file_path).await?;

    if verify_hashes {
        if let Some(expected_hash) = &payload.sha256 {
            if !computed_hash.eq_ignore_ascii_case(expected_hash) {
                return Err(MsvcKitError::HashMismatch {
                    file: payload.file_name.clone(),
                    expected: expected_hash.clone(),
                    actual: computed_hash,
                });
            }
        }
    }

    // Store completed with computed hash
    {
        let mut idx = index.lock().await;
        idx.mark_completed(payload, file_path.clone(), Some(computed_hash))
            .await?;
    }

    progress.on_file_complete(&payload.file_name, "downloaded");

    Ok(PayloadResult {
        path: file_path,
        transferred: payload.size,
        outcome: PayloadOutcome::Downloaded,
    })
}

/// Download a single file with progress handler (no resume)
async fn download_file_with_handler(
    client: &Client,
    payload: &PackagePayload,
    path: &Path,
    progress: &BoxedProgressHandler,
) -> Result<DownloadFileResult> {
    for attempt in 0..=dl_const::MAX_RETRIES {
        let response = match client.get(&payload.url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                if attempt < dl_const::MAX_RETRIES
                    && (e.is_connect() || e.is_timeout() || e.is_body())
                {
                    let backoff = Duration::from_secs(1 << attempt);
                    tracing::warn!(
                        "Retrying {} (request error: {}, attempt {}, backoff {:?})",
                        payload.file_name,
                        e,
                        attempt + 1,
                        backoff
                    );
                    sleep(backoff).await;
                    continue;
                }
                return Err(MsvcKitError::DownloadNetwork {
                    file: payload.file_name.clone(),
                    url: payload.url.clone(),
                    source: e,
                });
            }
        };

        if (response.status().is_server_error()
            || response.status() == StatusCode::TOO_MANY_REQUESTS)
            && attempt < dl_const::MAX_RETRIES
        {
            let status = response.status();
            let backoff = Duration::from_secs(1 << attempt);
            tracing::warn!(
                "Retrying {} (status {}, attempt {}, backoff {:?})",
                payload.file_name,
                status,
                attempt + 1,
                backoff
            );
            sleep(backoff).await;
            continue;
        }

        if !response.status().is_success() {
            return Err(MsvcKitError::DownloadNetwork {
                file: payload.file_name.clone(),
                url: payload.url.clone(),
                source: response.error_for_status().unwrap_err(),
            });
        }

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file = tokio::fs::File::create(path).await?;

        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    file.write_all(&chunk).await?;
                    progress.on_progress(chunk.len() as u64);
                }
                Err(e) => {
                    // Body streaming error - retry
                    let _ = tokio::fs::remove_file(path).await;

                    if attempt < dl_const::MAX_RETRIES {
                        let backoff = Duration::from_secs(1 << attempt);
                        tracing::warn!(
                            "Retrying {} (body read error: {}, attempt {}, backoff {:?})",
                            payload.file_name,
                            e,
                            attempt + 1,
                            backoff
                        );
                        sleep(backoff).await;
                        continue;
                    }

                    return Err(MsvcKitError::DownloadNetwork {
                        file: payload.file_name.clone(),
                        url: payload.url.clone(),
                        source: e,
                    });
                }
            }
        }

        file.flush().await?;
        return Ok(DownloadFileResult);
    }

    Err(MsvcKitError::Other(format!(
        "Download failed for {} after {} retries",
        payload.file_name,
        dl_const::MAX_RETRIES
    )))
}
