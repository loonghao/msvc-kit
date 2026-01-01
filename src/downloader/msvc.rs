//! MSVC compiler download functionality

use std::path::PathBuf;

use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::{DownloadOptions, VsManifest, PackagePayload};
use crate::error::{MsvcKitError, Result};
use crate::installer::InstallInfo;
use crate::version::Architecture;

/// MSVC component IDs for different architectures
const MSVC_COMPONENTS: &[&str] = &[
    "Microsoft.VC.{version}.Tools.Host{host}.Target{target}",
    "Microsoft.VC.{version}.CRT.Headers",
    "Microsoft.VC.{version}.CRT.Source",
    "Microsoft.VC.{version}.CRT.{target}.Desktop",
    "Microsoft.VC.{version}.CRT.{target}.Store",
    "Microsoft.VC.{version}.ASAN.Headers",
    "Microsoft.VC.{version}.ASAN.{target}",
    "Microsoft.VC.{version}.ATL.{target}",
    "Microsoft.VC.{version}.ATL.Headers",
    "Microsoft.VC.{version}.MFC.{target}",
    "Microsoft.VC.{version}.MFC.Headers",
];

/// Core MSVC components (minimum required)
const MSVC_CORE_COMPONENTS: &[&str] = &[
    "Microsoft.VC.{version}.Tools.Host{host}.Target{target}",
    "Microsoft.VC.{version}.CRT.Headers",
    "Microsoft.VC.{version}.CRT.{target}.Desktop",
];

/// MSVC downloader
pub struct MsvcDownloader {
    options: DownloadOptions,
    client: reqwest::Client,
}

impl MsvcDownloader {
    /// Create a new MSVC downloader
    pub fn new(options: DownloadOptions) -> Self {
        Self {
            options,
            client: reqwest::Client::builder()
                .user_agent("msvc-kit/0.1.0")
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Download MSVC components
    pub async fn download(&self) -> Result<InstallInfo> {
        tracing::info!("Fetching Visual Studio manifest...");
        let manifest = VsManifest::fetch().await?;

        // Determine version to download
        let version = self.options.msvc_version.clone()
            .or_else(|| manifest.get_latest_msvc_version())
            .ok_or_else(|| MsvcKitError::VersionNotFound("No MSVC version found".to_string()))?;

        tracing::info!("Downloading MSVC version: {}", version);

        // Get the version prefix (e.g., "14.40" from "14.40.33807")
        let version_prefix = version.split('.').take(2).collect::<Vec<_>>().join(".");

        // Find packages to download
        let packages = self.find_required_packages(&manifest, &version_prefix)?;

        if packages.is_empty() {
            return Err(MsvcKitError::ComponentNotFound(
                format!("No MSVC packages found for version {}", version)
            ));
        }

        // Create download directory
        let download_dir = self.options.target_dir.join("downloads").join("msvc");
        tokio::fs::create_dir_all(&download_dir).await?;

        // Download all packages
        let downloaded_files = self.download_packages(&packages, &download_dir).await?;

        // Create install info
        let install_path = self.options.target_dir.join("VC").join("Tools").join("MSVC").join(&version);

        Ok(InstallInfo {
            component_type: "msvc".to_string(),
            version: version.clone(),
            install_path,
            downloaded_files,
            arch: self.options.arch,
        })
    }

    /// Find required packages from the manifest
    fn find_required_packages(&self, manifest: &VsManifest, version_prefix: &str) -> Result<Vec<PackagePayload>> {
        let host = self.options.host_arch.unwrap_or(Architecture::host());
        let target = self.options.arch;

        let mut payloads = Vec::new();

        // Build component patterns
        for component_pattern in MSVC_CORE_COMPONENTS {
            let component_id = component_pattern
                .replace("{version}", version_prefix)
                .replace("{host}", &host.to_string().to_uppercase())
                .replace("{target}", &target.to_string());

            // Find matching packages in manifest
            for item in &manifest.channel_items {
                if item.id.contains(&component_id) || 
                   item.id.to_lowercase().contains(&component_id.to_lowercase()) {
                    for payload in &item.payloads {
                        payloads.push(PackagePayload {
                            file_name: payload.file_name.clone(),
                            url: payload.url.clone(),
                            size: payload.size.unwrap_or(0),
                            sha256: payload.sha256.clone(),
                        });
                    }
                }
            }
        }

        // Also search for general MSVC packages
        for item in &manifest.channel_items {
            if item.id.starts_with("Microsoft.VC.") && 
               item.id.contains(version_prefix) &&
               (item.id.contains("Tools") || item.id.contains("CRT")) {
                for payload in &item.payloads {
                    // Avoid duplicates
                    if !payloads.iter().any(|p| p.url == payload.url) {
                        payloads.push(PackagePayload {
                            file_name: payload.file_name.clone(),
                            url: payload.url.clone(),
                            size: payload.size.unwrap_or(0),
                            sha256: payload.sha256.clone(),
                        });
                    }
                }
            }
        }

        Ok(payloads)
    }

    /// Download packages with progress display
    async fn download_packages(
        &self,
        packages: &[PackagePayload],
        download_dir: &PathBuf,
    ) -> Result<Vec<PathBuf>> {
        let multi_progress = MultiProgress::new();
        let total_size: u64 = packages.iter().map(|p| p.size).sum();

        let overall_pb = multi_progress.add(ProgressBar::new(total_size));
        overall_pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        overall_pb.set_message("Downloading MSVC components...");

        let mut downloaded_files = Vec::new();
        let mut downloaded_size = 0u64;

        for package in packages {
            let file_path = download_dir.join(&package.file_name);

            // Skip if already downloaded and hash matches
            if file_path.exists() {
                if let Some(expected_hash) = &package.sha256 {
                    if self.verify_file_hash(&file_path, expected_hash).await? {
                        tracing::info!("Skipping {} (already downloaded)", package.file_name);
                        downloaded_size += package.size;
                        overall_pb.set_position(downloaded_size);
                        downloaded_files.push(file_path);
                        continue;
                    }
                }
            }

            // Download the file
            tracing::info!("Downloading: {}", package.file_name);
            self.download_file(&package.url, &file_path, package.size, &overall_pb, downloaded_size).await?;

            // Verify hash if available
            if self.options.verify_hashes {
                if let Some(expected_hash) = &package.sha256 {
                    if !self.verify_file_hash(&file_path, expected_hash).await? {
                        return Err(MsvcKitError::HashMismatch {
                            file: package.file_name.clone(),
                            expected: expected_hash.clone(),
                            actual: "mismatch".to_string(),
                        });
                    }
                }
            }

            downloaded_size += package.size;
            downloaded_files.push(file_path);
        }

        overall_pb.finish_with_message("Download complete!");
        Ok(downloaded_files)
    }

    /// Download a single file with progress tracking
    async fn download_file(
        &self,
        url: &str,
        path: &PathBuf,
        size: u64,
        overall_pb: &ProgressBar,
        base_position: u64,
    ) -> Result<()> {
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(MsvcKitError::Network(
                response.error_for_status().unwrap_err()
            ));
        }

        let mut file = tokio::fs::File::create(path).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            overall_pb.set_position(base_position + downloaded.min(size));
        }

        file.flush().await?;
        Ok(())
    }

    /// Verify file hash
    async fn verify_file_hash(&self, path: &PathBuf, expected: &str) -> Result<bool> {
        let content = tokio::fs::read(path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let result = hasher.finalize();
        let actual = hex::encode(result);
        Ok(actual.eq_ignore_ascii_case(expected))
    }
}
