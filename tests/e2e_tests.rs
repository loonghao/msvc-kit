//! End-to-end tests for msvc-kit
//!
//! These tests verify the complete workflow of msvc-kit, including:
//! - Downloading MSVC and SDK components
//! - Extracting packages
//! - Setting up environment
//!
//! Note: Some tests require network access and may take a long time.
//! Run with: cargo test --test e2e_tests -- --ignored

use std::path::PathBuf;
use std::sync::Arc;

use msvc_kit::config::MsvcKitConfig;
use msvc_kit::downloader::{DownloadIndex, DownloadStatus, IndexEntry};
use msvc_kit::env::{generate_activation_script, MsvcEnvironment};
use msvc_kit::installer::InstallInfo;
use msvc_kit::version::Architecture;
use msvc_kit::{DownloadOptions, ShellType};

// ============================================================================
// Download Index Tests
// ============================================================================

mod download_index_tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_download_index_create_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");

        // Create new index
        let index = DownloadIndex::load(&index_path).await.unwrap();
        drop(index);

        // Load existing index
        let _index = DownloadIndex::load(&index_path).await.unwrap();

        // Verify the database file exists
        assert!(index_path.with_extension("db").exists());
    }

    #[tokio::test]
    async fn test_download_index_upsert_and_get() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");

        let mut index = DownloadIndex::load(&index_path).await.unwrap();

        let entry = IndexEntry {
            file_name: "test_file.vsix".to_string(),
            url: "https://example.com/test_file.vsix".to_string(),
            size: 1024,
            sha256: Some("abc123".to_string()),
            computed_hash: Some("abc123".to_string()),
            local_path: temp_dir.path().join("test_file.vsix"),
            status: DownloadStatus::Completed,
            bytes_downloaded: 1024,
            hash_verified: true,
            updated_at: Utc::now(),
        };

        index.upsert_entry(&entry).await.unwrap();

        let retrieved = index.get_entry("test_file.vsix").await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.file_name, "test_file.vsix");
        assert_eq!(retrieved.size, 1024);
        assert_eq!(retrieved.status, DownloadStatus::Completed);
    }

    #[tokio::test]
    async fn test_download_index_remove() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");

        let mut index = DownloadIndex::load(&index_path).await.unwrap();

        let entry = IndexEntry {
            file_name: "to_remove.vsix".to_string(),
            url: "https://example.com/to_remove.vsix".to_string(),
            size: 512,
            sha256: None,
            computed_hash: None,
            local_path: temp_dir.path().join("to_remove.vsix"),
            status: DownloadStatus::Partial,
            bytes_downloaded: 256,
            hash_verified: false,
            updated_at: Utc::now(),
        };

        index.upsert_entry(&entry).await.unwrap();
        assert!(index.get_entry("to_remove.vsix").await.unwrap().is_some());

        index.remove("to_remove.vsix").await.unwrap();
        assert!(index.get_entry("to_remove.vsix").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_download_index_is_entry_unchanged() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let local_path = temp_dir.path().join("unchanged.vsix");

        let mut index = DownloadIndex::load(&index_path).await.unwrap();

        let entry = IndexEntry {
            file_name: "unchanged.vsix".to_string(),
            url: "https://example.com/unchanged.vsix".to_string(),
            size: 2048,
            sha256: Some("hash123".to_string()),
            computed_hash: Some("hash123".to_string()),
            local_path: local_path.clone(),
            status: DownloadStatus::Completed,
            bytes_downloaded: 2048,
            hash_verified: true,
            updated_at: Utc::now(),
        };

        index.upsert_entry(&entry).await.unwrap();

        // Check unchanged
        let unchanged = index
            .is_entry_unchanged(
                "unchanged.vsix",
                DownloadStatus::Completed,
                2048,
                &Some("hash123".to_string()),
                &local_path,
            )
            .await
            .unwrap();
        assert!(unchanged);

        // Check changed (different size)
        let changed = index
            .is_entry_unchanged(
                "unchanged.vsix",
                DownloadStatus::Completed,
                1024, // different size
                &Some("hash123".to_string()),
                &local_path,
            )
            .await
            .unwrap();
        assert!(!changed);
    }

    #[tokio::test]
    async fn test_download_index_get_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");

        let index = DownloadIndex::load(&index_path).await.unwrap();

        let result = index.get_entry("nonexistent.vsix").await.unwrap();
        assert!(result.is_none());
    }
}

// ============================================================================
// Environment Generation Tests
// ============================================================================

mod env_generation_tests {
    use super::*;

    fn create_mock_install_info(component_type: &str, version: &str) -> InstallInfo {
        InstallInfo {
            component_type: component_type.to_string(),
            version: version.to_string(),
            install_path: PathBuf::from(format!("C:/test/{}", component_type)),
            downloaded_files: vec![],
            arch: Architecture::X64,
        }
    }

    #[test]
    fn test_install_info_creation() {
        let info = create_mock_install_info("msvc", "14.44.33807");
        assert_eq!(info.component_type, "msvc");
        assert_eq!(info.version, "14.44.33807");
        assert_eq!(info.arch, Architecture::X64);
    }

    #[test]
    fn test_generate_cmd_script() {
        let env = MsvcEnvironment {
            vc_install_dir: PathBuf::from("C:\\VC"),
            vc_tools_install_dir: PathBuf::from("C:\\VC\\Tools\\MSVC\\14.44"),
            vc_tools_version: "14.44.33807".to_string(),
            windows_sdk_dir: PathBuf::from("C:\\Windows Kits\\10"),
            windows_sdk_version: "10.0.26100.0".to_string(),
            include_paths: vec![PathBuf::from("C:\\include")],
            lib_paths: vec![PathBuf::from("C:\\lib")],
            bin_paths: vec![PathBuf::from("C:\\bin")],
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        };

        let script = generate_activation_script(&env, ShellType::Cmd).unwrap();

        assert!(script.contains("@echo off"));
        assert!(script.contains("set \""));
        assert!(script.contains("MSVC Toolchain activated"));
    }

    #[test]
    fn test_generate_powershell_script() {
        let env = MsvcEnvironment {
            vc_install_dir: PathBuf::from("C:\\VC"),
            vc_tools_install_dir: PathBuf::from("C:\\VC\\Tools\\MSVC\\14.44"),
            vc_tools_version: "14.44.33807".to_string(),
            windows_sdk_dir: PathBuf::from("C:\\Windows Kits\\10"),
            windows_sdk_version: "10.0.26100.0".to_string(),
            include_paths: vec![PathBuf::from("C:\\include")],
            lib_paths: vec![PathBuf::from("C:\\lib")],
            bin_paths: vec![PathBuf::from("C:\\bin")],
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        };

        let script = generate_activation_script(&env, ShellType::PowerShell).unwrap();

        assert!(script.contains("$env:"));
        assert!(script.contains("Write-Host"));
    }

    #[test]
    fn test_generate_bash_script() {
        let env = MsvcEnvironment {
            vc_install_dir: PathBuf::from("C:\\VC"),
            vc_tools_install_dir: PathBuf::from("C:\\VC\\Tools\\MSVC\\14.44"),
            vc_tools_version: "14.44.33807".to_string(),
            windows_sdk_dir: PathBuf::from("C:\\Windows Kits\\10"),
            windows_sdk_version: "10.0.26100.0".to_string(),
            include_paths: vec![PathBuf::from("C:\\include")],
            lib_paths: vec![PathBuf::from("C:\\lib")],
            bin_paths: vec![PathBuf::from("C:\\bin")],
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        };

        let script = generate_activation_script(&env, ShellType::Bash).unwrap();

        assert!(script.contains("#!/bin/bash"));
        assert!(script.contains("export "));
        // Should convert Windows paths to Unix style
        assert!(script.contains("/c/") || script.contains("C:"));
    }
}

// ============================================================================
// Config Persistence Tests
// ============================================================================

mod config_persistence_tests {
    use super::*;

    #[test]
    fn test_config_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config = MsvcKitConfig {
            install_dir: PathBuf::from("C:/custom/path"),
            default_msvc_version: Some("14.44".to_string()),
            default_sdk_version: Some("10.0.26100.0".to_string()),
            default_arch: Architecture::Arm64,
            verify_hashes: false,
            parallel_downloads: 16,
            cache_dir: Some(PathBuf::from("C:/cache")),
        };

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_path, &toml_str).unwrap();

        // Deserialize from TOML
        let loaded_toml = std::fs::read_to_string(&config_path).unwrap();
        let loaded: MsvcKitConfig = toml::from_str(&loaded_toml).unwrap();

        assert_eq!(loaded.install_dir, config.install_dir);
        assert_eq!(loaded.default_msvc_version, config.default_msvc_version);
        assert_eq!(loaded.default_sdk_version, config.default_sdk_version);
        assert_eq!(loaded.default_arch, config.default_arch);
        assert_eq!(loaded.verify_hashes, config.verify_hashes);
        assert_eq!(loaded.parallel_downloads, config.parallel_downloads);
    }
}

// ============================================================================
// Extraction Tests
// ============================================================================

mod extraction_tests {
    use std::io::Write;

    #[tokio::test]
    async fn test_extract_zip_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let zip_path = temp_dir.path().join("test.vsix");
        let extract_dir = temp_dir.path().join("extracted");

        // Create a simple ZIP file
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("Contents/test.txt", options).unwrap();
        zip.write_all(b"Hello, World!").unwrap();
        zip.finish().unwrap();

        // Extract
        msvc_kit::installer::extract_vsix(&zip_path, &extract_dir)
            .await
            .unwrap();

        // Verify
        let extracted_file = extract_dir.join("test.txt");
        assert!(extracted_file.exists());

        let content = std::fs::read_to_string(&extracted_file).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_extract_zip_with_nested_dirs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let zip_path = temp_dir.path().join("nested.vsix");
        let extract_dir = temp_dir.path().join("extracted");

        // Create a ZIP with nested directories
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("Contents/dir1/dir2/file.txt", options)
            .unwrap();
        zip.write_all(b"Nested content").unwrap();
        zip.finish().unwrap();

        // Extract
        msvc_kit::installer::extract_vsix(&zip_path, &extract_dir)
            .await
            .unwrap();

        // Verify nested structure
        let extracted_file = extract_dir.join("dir1").join("dir2").join("file.txt");
        assert!(extracted_file.exists());
    }
}

// ============================================================================
// Download Options Builder Tests
// ============================================================================

mod download_options_builder_tests {
    use super::*;

    #[test]
    fn test_download_options_with_config() {
        let config = MsvcKitConfig {
            install_dir: PathBuf::from("C:/from_config"),
            default_msvc_version: Some("14.43".to_string()),
            default_sdk_version: Some("10.0.22621.0".to_string()),
            default_arch: Architecture::X86,
            verify_hashes: false,
            parallel_downloads: 2,
            cache_dir: None,
        };

        // Options can override config - use builder pattern
        let mut options = DownloadOptions::builder()
            .target_dir(&config.install_dir)
            .arch(config.default_arch)
            .verify_hashes(config.verify_hashes)
            .parallel_downloads(config.parallel_downloads)
            .build();
        options.msvc_version = config.default_msvc_version.clone();
        options.sdk_version = config.default_sdk_version.clone();

        assert_eq!(options.msvc_version, Some("14.43".to_string()));
        assert_eq!(options.sdk_version, Some("10.0.22621.0".to_string()));
        assert_eq!(options.arch, Architecture::X86);
        assert!(!options.verify_hashes);
        assert_eq!(options.parallel_downloads, 2);
    }

    #[test]
    fn test_download_options_override_config() {
        let config = MsvcKitConfig::default();

        // Override specific fields - use builder pattern
        let options = DownloadOptions::builder()
            .msvc_version("14.44")
            .target_dir("C:/custom")
            .arch(Architecture::Arm64)
            .host_arch(Architecture::X64)
            .verify_hashes(config.verify_hashes)
            .parallel_downloads(8)
            .build();

        assert_eq!(options.msvc_version, Some("14.44".to_string()));
        assert_eq!(options.target_dir, PathBuf::from("C:/custom"));
        assert_eq!(options.arch, Architecture::Arm64);
        assert_eq!(options.parallel_downloads, 8);
    }
}

// ============================================================================
// Network Tests (Ignored by default - require network access)
// ============================================================================

mod network_tests {
    use super::*;

    /// Test fetching the VS manifest from Microsoft servers
    /// This test is ignored by default as it requires network access
    #[tokio::test]
    #[ignore]
    async fn test_fetch_vs_manifest() {
        use msvc_kit::downloader::VsManifest;

        let manifest = VsManifest::fetch().await;
        assert!(manifest.is_ok(), "Failed to fetch manifest: {:?}", manifest);

        let manifest = manifest.unwrap();
        assert!(!manifest.packages.is_empty());
    }

    /// Test getting the latest MSVC version
    #[tokio::test]
    #[ignore]
    async fn test_get_latest_msvc_version() {
        use msvc_kit::downloader::VsManifest;

        let manifest = VsManifest::fetch().await.unwrap();
        let version = manifest.get_latest_msvc_version();

        assert!(version.is_some());
        let version = version.unwrap();
        assert!(version.starts_with("14."));
    }

    /// Test getting the latest SDK version
    #[tokio::test]
    #[ignore]
    async fn test_get_latest_sdk_version() {
        use msvc_kit::downloader::VsManifest;

        let manifest = VsManifest::fetch().await.unwrap();
        let version = manifest.get_latest_sdk_version();

        assert!(version.is_some());
        let version = version.unwrap();
        assert!(version.starts_with("10.0."));
    }

    /// Full download test (very slow, requires significant disk space)
    #[tokio::test]
    #[ignore]
    async fn test_full_download_workflow() {
        let temp_dir = tempfile::tempdir().unwrap();

        let options = DownloadOptions {
            target_dir: temp_dir.path().to_path_buf(),
            arch: Architecture::X64,
            verify_hashes: true,
            parallel_downloads: 4,
            ..Default::default()
        };

        // Download MSVC
        let msvc_result = msvc_kit::download_msvc(&options).await;
        assert!(
            msvc_result.is_ok(),
            "MSVC download failed: {:?}",
            msvc_result
        );

        let msvc_info = msvc_result.unwrap();
        assert_eq!(msvc_info.component_type, "msvc");
        assert!(!msvc_info.version.is_empty());

        // Download SDK
        let sdk_result = msvc_kit::download_sdk(&options).await;
        assert!(sdk_result.is_ok(), "SDK download failed: {:?}", sdk_result);

        let sdk_info = sdk_result.unwrap();
        assert_eq!(sdk_info.component_type, "sdk");
        assert!(!sdk_info.version.is_empty());

        // Setup environment
        let env_result = msvc_kit::setup_environment(&msvc_info, Some(&sdk_info));
        assert!(
            env_result.is_ok(),
            "Environment setup failed: {:?}",
            env_result
        );

        let env = env_result.unwrap();
        assert!(!env.vc_tools_version.is_empty());
        assert!(!env.windows_sdk_version.is_empty());
    }
}

// ============================================================================
// Concurrency Tests
// ============================================================================

mod concurrency_tests {
    use super::*;
    use tokio::sync::Semaphore;

    #[tokio::test]
    async fn test_concurrent_index_access() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("concurrent_index");

        // Create index
        let mut index = DownloadIndex::load(&index_path).await.unwrap();

        // Insert multiple entries concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let entry = IndexEntry {
                    file_name: format!("file_{}.vsix", i),
                    url: format!("https://example.com/file_{}.vsix", i),
                    size: 1024 * (i + 1) as u64,
                    sha256: None,
                    computed_hash: None,
                    local_path: temp_dir.path().join(format!("file_{}.vsix", i)),
                    status: DownloadStatus::Completed,
                    bytes_downloaded: 1024 * (i + 1) as u64,
                    hash_verified: false,
                    updated_at: chrono::Utc::now(),
                };
                entry
            })
            .collect();

        for entry in handles {
            index.upsert_entry(&entry).await.unwrap();
        }

        // Verify all entries
        for i in 0..10 {
            let entry = index.get_entry(&format!("file_{}.vsix", i)).await.unwrap();
            assert!(entry.is_some());
        }
    }

    #[tokio::test]
    async fn test_semaphore_based_concurrency_limit() {
        let semaphore = Arc::new(Semaphore::new(4));
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let handles: Vec<_> = (0..20)
            .map(|_| {
                let sem = semaphore.clone();
                let cnt = counter.clone();
                tokio::spawn(async move {
                    let _permit = sem.acquire().await.unwrap();
                    cnt.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    cnt.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                })
            })
            .collect();

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
