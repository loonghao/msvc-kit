//! Windows SDK download functionality

use std::path::PathBuf;

use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::{DownloadOptions, VsManifest, PackagePayload};
use crate::error::{MsvcKitError, Result};
use crate::installer::InstallInfo;

/// Windows SDK component patterns
const SDK_COMPONENTS: &[&str] = &[
    "Windows SDK for Desktop C++ {arch} Apps",
    "Windows SDK Desktop Headers {arch}",
    "Windows SDK Desktop Libs {arch}",
    "Windows SDK for Desktop C++ Build Tools",
    "Universal CRT Headers Libraries and Sources",
];

/// Windows SDK downloader
pub struct SdkDownloader {
    options: DownloadOptions,
    client: reqwest::Client,
}

impl SdkDownloader {
    /// Create a new SDK downloader
    pub fn new(options: DownloadOptions) -> Self {
        Self {
            options,
            client: reqwest::Client::builder()
                .user_agent("msvc-kit/0.1.0")
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Download Windows SDK components
    pub async fn download(&self) -> Result<InstallInfo> {
        tracing::info!("Fetching Visual Studio manifest...");
        let manifest = VsManifest::fetch().await?;

        // Determine version to download
        let version = self.options.sdk_version.clone()
            .or_else(|| manifest.get_latest_sdk_version())
            .ok_or_else(|| MsvcKitError::VersionNotFound("No Windows SDK version found".to_string()))?;

        tracing::info!("Downloading Windows SDK version: {}", version);

        // Extract the build number (e.g., "22621" from "10.0.22621.0")
        let build_number = version
            .split('.')
            .nth(2)
            .ok_or_else(|| MsvcKitError::VersionNotFound(format!("Invalid SDK version format: {}", version)))?;

        // Find packages to download
        let packages = self.find_required_packages(&manifest, build_number)?;

        if packages.is_empty() {
            return Err(MsvcKitError::ComponentNotFound(
                format!("No Windows SDK packages found for version {}", version)
            ));
        }

        // Create download directory
        let download_dir = self.options.target_dir.join("downloads").join("sdk");
        tokio::fs::create_dir_all(&download_dir).await?;

        // Download all packages
        let downloaded_files = self.download_packages(&packages, &download_dir).await?;

        // Create install info
        let install_path = self.options.target_dir.join("Windows Kits").join("10");

        Ok(InstallInfo {
            component_type: "sdk".to_string(),
            version,
            install_path,
            downloaded_files,
            arch: self.options.arch,
        })
    }

    /// Find required SDK packages from the manifest
    fn find_required_packages(&self, manifest: &VsManifest, build_number: &str) -> Result<Vec<PackagePayload>> {
        let mut payloads = Vec::new();

        // Search for SDK packages matching the build number
        for item in &manifest.channel_items {
            let id_lower = item.id.to_lowercase();
            
            // Match SDK component patterns
            let is_sdk_component = 
                (id_lower.contains("windows") && id_lower.contains("sdk") && id_lower.contains(build_number)) ||
                (id_lower.contains("win10sdk") && id_lower.contains(build_number)) ||
                (id_lower.contains("win11sdk") && id_lower.contains(build_number)) ||
                (id_lower.contains("universalcrt"));

            if is_sdk_component {
                for payload in &item.payloads {
                    // Avoid duplicates
                    if !payloads.iter().any(|p: &PackagePayload| p.url == payload.url) {
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
        let total_size: u64 = packages.iter().map(|p| p.size).sum();

        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message("Downloading Windows SDK components...");

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
                        pb.set_position(downloaded_size);
                        downloaded_files.push(file_path);
                        continue;
                    }
                }
            }

            // Download the file
            tracing::info!("Downloading: {}", package.file_name);
            self.download_file(&package.url, &file_path, package.size, &pb, downloaded_size).await?;

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

        pb.finish_with_message("Download complete!");
        Ok(downloaded_files)
    }

    /// Download a single file with progress tracking
    async fn download_file(
        &self,
        url: &str,
        path: &PathBuf,
        size: u64,
        pb: &ProgressBar,
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
            pb.set_position(base_position + downloaded.min(size));
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
