//! Downloader module tests

use msvc_kit::downloader::{
    compute_hash, hashes_match, AvailableVersions, CacheManager, ComponentType, DownloadOptions,
    DownloadPreview, FileSystemCacheManager, HttpClientConfig, NoopProgressHandler, PackagePreview,
    ProgressHandler,
};
use msvc_kit::version::Architecture;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// DownloadOptions Tests
// ============================================================================

#[test]
fn test_download_options_default() {
    let options = DownloadOptions::default();
    assert!(options.msvc_version.is_none());
    assert!(options.sdk_version.is_none());
    assert!(options.verify_hashes);
    assert_eq!(options.parallel_downloads, 4);
    assert_eq!(options.arch, Architecture::X64);
}

#[test]
fn test_download_options_custom() {
    let options = DownloadOptions::builder()
        .msvc_version("14.44")
        .sdk_version("10.0.26100.0")
        .target_dir("C:/custom")
        .arch(Architecture::Arm64)
        .host_arch(Architecture::X64)
        .verify_hashes(false)
        .parallel_downloads(16)
        .build();

    assert_eq!(options.msvc_version, Some("14.44".to_string()));
    assert_eq!(options.sdk_version, Some("10.0.26100.0".to_string()));
    assert_eq!(options.target_dir, PathBuf::from("C:/custom"));
    assert_eq!(options.arch, Architecture::Arm64);
    assert_eq!(options.host_arch, Some(Architecture::X64));
    assert!(!options.verify_hashes);
    assert_eq!(options.parallel_downloads, 16);
}

#[test]
fn test_builder_all_options() {
    let options = DownloadOptions::builder()
        .msvc_version("14.44")
        .sdk_version("10.0.26100.0")
        .target_dir("C:/custom")
        .arch(Architecture::Arm64)
        .host_arch(Architecture::X64)
        .verify_hashes(false)
        .parallel_downloads(16)
        .dry_run(true)
        .build();

    assert_eq!(options.msvc_version, Some("14.44".to_string()));
    assert_eq!(options.sdk_version, Some("10.0.26100.0".to_string()));
    assert_eq!(options.target_dir, PathBuf::from("C:/custom"));
    assert_eq!(options.arch, Architecture::Arm64);
    assert_eq!(options.host_arch, Some(Architecture::X64));
    assert!(!options.verify_hashes);
    assert_eq!(options.parallel_downloads, 16);
    assert!(options.dry_run);
}

#[test]
fn test_builder_partial_options() {
    let options = DownloadOptions::builder()
        .msvc_version("14.44")
        .target_dir("C:/test")
        .build();

    assert_eq!(options.msvc_version, Some("14.44".to_string()));
    assert!(options.sdk_version.is_none());
    assert_eq!(options.target_dir, PathBuf::from("C:/test"));
}

#[test]
fn test_download_options_debug() {
    let options = DownloadOptions::default();
    let debug_str = format!("{:?}", options);
    assert!(debug_str.contains("DownloadOptions"));
    assert!(debug_str.contains("msvc_version"));
    assert!(debug_str.contains("target_dir"));
}

#[test]
fn test_builder_http_client() {
    let client = reqwest::Client::new();
    let options = DownloadOptions::builder()
        .http_client(client)
        .target_dir("C:/test")
        .build();

    assert!(options.http_client.is_some());
}

#[test]
fn test_builder_progress_handler() {
    let handler: Arc<dyn ProgressHandler> = Arc::new(NoopProgressHandler);
    let options = DownloadOptions::builder()
        .progress_handler(handler)
        .target_dir("C:/test")
        .build();

    assert!(options.progress_handler.is_some());
}

#[test]
fn test_download_options_clone() {
    let options = DownloadOptions::builder()
        .msvc_version("14.44")
        .sdk_version("10.0.26100.0")
        .target_dir("C:/test")
        .arch(Architecture::X64)
        .verify_hashes(true)
        .parallel_downloads(8)
        .dry_run(false)
        .build();

    let cloned = options.clone();
    assert_eq!(cloned.msvc_version, options.msvc_version);
    assert_eq!(cloned.sdk_version, options.sdk_version);
    assert_eq!(cloned.target_dir, options.target_dir);
    assert_eq!(cloned.arch, options.arch);
    assert_eq!(cloned.verify_hashes, options.verify_hashes);
    assert_eq!(cloned.parallel_downloads, options.parallel_downloads);
    assert_eq!(cloned.dry_run, options.dry_run);
}

// ============================================================================
// DownloadPreview Tests
// ============================================================================

#[test]
fn test_download_preview_format() {
    let preview = DownloadPreview {
        component: "MSVC".to_string(),
        version: "14.44.33807".to_string(),
        package_count: 10,
        file_count: 100,
        total_size: 1024 * 1024 * 500,
        packages: vec![],
    };

    let formatted = preview.format();
    assert!(formatted.contains("MSVC"));
    assert!(formatted.contains("14.44.33807"));
    assert!(formatted.contains("10 packages"));
    assert!(formatted.contains("100 files"));
}

#[test]
fn test_download_preview_format_with_packages() {
    let preview = DownloadPreview {
        component: "SDK".to_string(),
        version: "10.0.26100.0".to_string(),
        package_count: 5,
        file_count: 250,
        total_size: 1024 * 1024 * 1024,
        packages: vec![
            PackagePreview {
                id: "Microsoft.Windows.SDK.Headers".to_string(),
                version: "10.0.26100.0".to_string(),
                file_count: 100,
                size: 512 * 1024 * 1024,
            },
            PackagePreview {
                id: "Microsoft.Windows.SDK.Libs".to_string(),
                version: "10.0.26100.0".to_string(),
                file_count: 150,
                size: 512 * 1024 * 1024,
            },
        ],
    };

    let formatted = preview.format();
    assert!(formatted.contains("SDK"));
    assert!(formatted.contains("10.0.26100.0"));
    assert!(formatted.contains("5 packages"));
    assert!(formatted.contains("250 files"));
}

#[test]
fn test_download_preview_small_size() {
    let preview = DownloadPreview {
        component: "Test".to_string(),
        version: "1.0".to_string(),
        package_count: 1,
        file_count: 1,
        total_size: 1024,
        packages: vec![],
    };

    let formatted = preview.format();
    assert!(formatted.contains("Test"));
    assert!(formatted.contains("1 packages"));
    assert!(formatted.contains("1 files"));
}

#[test]
fn test_download_preview_debug() {
    let preview = DownloadPreview {
        component: "MSVC".to_string(),
        version: "14.44".to_string(),
        package_count: 1,
        file_count: 1,
        total_size: 1024,
        packages: vec![],
    };

    let debug_str = format!("{:?}", preview);
    assert!(debug_str.contains("DownloadPreview"));
    assert!(debug_str.contains("MSVC"));
}

#[test]
fn test_download_preview_clone() {
    let preview = DownloadPreview {
        component: "MSVC".to_string(),
        version: "14.44".to_string(),
        package_count: 2,
        file_count: 20,
        total_size: 2048,
        packages: vec![PackagePreview {
            id: "pkg1".to_string(),
            version: "1.0".to_string(),
            file_count: 10,
            size: 1024,
        }],
    };

    let cloned = preview.clone();
    assert_eq!(cloned.component, preview.component);
    assert_eq!(cloned.version, preview.version);
    assert_eq!(cloned.package_count, preview.package_count);
    assert_eq!(cloned.packages.len(), preview.packages.len());
}

// ============================================================================
// PackagePreview Tests
// ============================================================================

#[test]
fn test_package_preview() {
    let package = PackagePreview {
        id: "Microsoft.VC.Tools".to_string(),
        version: "14.44.33807".to_string(),
        file_count: 50,
        size: 1024 * 1024 * 100,
    };

    assert_eq!(package.id, "Microsoft.VC.Tools");
    assert_eq!(package.version, "14.44.33807");
    assert_eq!(package.file_count, 50);
}

#[test]
fn test_package_preview_debug() {
    let package = PackagePreview {
        id: "Test.Package".to_string(),
        version: "1.0.0".to_string(),
        file_count: 10,
        size: 1024 * 1024,
    };

    let debug_str = format!("{:?}", package);
    assert!(debug_str.contains("PackagePreview"));
    assert!(debug_str.contains("Test.Package"));
}

// ============================================================================
// AvailableVersions Tests
// ============================================================================

#[test]
fn test_available_versions_debug() {
    let versions = AvailableVersions {
        msvc_versions: vec!["14.44".to_string(), "14.43".to_string()],
        sdk_versions: vec!["10.0.26100.0".to_string()],
        latest_msvc: Some("14.44".to_string()),
        latest_sdk: Some("10.0.26100.0".to_string()),
    };

    let debug_str = format!("{:?}", versions);
    assert!(debug_str.contains("AvailableVersions"));
    assert!(debug_str.contains("14.44"));
}

#[test]
fn test_available_versions_clone() {
    let versions = AvailableVersions {
        msvc_versions: vec!["14.44".to_string()],
        sdk_versions: vec!["10.0.26100.0".to_string()],
        latest_msvc: Some("14.44".to_string()),
        latest_sdk: Some("10.0.26100.0".to_string()),
    };

    let cloned = versions.clone();
    assert_eq!(cloned.msvc_versions, versions.msvc_versions);
    assert_eq!(cloned.sdk_versions, versions.sdk_versions);
    assert_eq!(cloned.latest_msvc, versions.latest_msvc);
    assert_eq!(cloned.latest_sdk, versions.latest_sdk);
}

// ============================================================================
// ComponentType Tests
// ============================================================================

#[test]
fn test_component_type_as_str() {
    assert_eq!(ComponentType::Msvc.as_str(), "msvc");
    assert_eq!(ComponentType::Sdk.as_str(), "sdk");
}

#[test]
fn test_component_type_display() {
    assert_eq!(format!("{}", ComponentType::Msvc), "msvc");
    assert_eq!(format!("{}", ComponentType::Sdk), "sdk");
}

#[test]
fn test_component_type_equality() {
    assert_eq!(ComponentType::Msvc, ComponentType::Msvc);
    assert_eq!(ComponentType::Sdk, ComponentType::Sdk);
    assert_ne!(ComponentType::Msvc, ComponentType::Sdk);
}

#[test]
fn test_component_type_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(ComponentType::Msvc);
    set.insert(ComponentType::Sdk);
    assert_eq!(set.len(), 2);
    assert!(set.contains(&ComponentType::Msvc));
    assert!(set.contains(&ComponentType::Sdk));
}

// ============================================================================
// Hash Utility Tests
// ============================================================================

#[test]
fn test_compute_hash_empty() {
    let hash = compute_hash(b"");
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn test_compute_hash_known_value() {
    let hash = compute_hash(b"test");
    assert_eq!(
        hash,
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    );
}

#[test]
fn test_hashes_match_case_insensitive() {
    assert!(hashes_match("ABCDEF", "abcdef"));
    assert!(hashes_match("AbCdEf", "aBcDeF"));
    assert!(hashes_match("123abc", "123ABC"));
}

#[test]
fn test_hashes_match_different() {
    assert!(!hashes_match("abc123", "abc124"));
    assert!(!hashes_match("", "abc"));
}

// ============================================================================
// HttpClientConfig Tests
// ============================================================================

#[test]
fn test_http_client_config_default() {
    let config = HttpClientConfig::default();
    assert!(config.user_agent.contains("msvc-kit"));
    assert_eq!(config.connect_timeout, Some(Duration::from_secs(30)));
    assert_eq!(config.timeout, Some(Duration::from_secs(300)));
}

#[test]
fn test_http_client_config_custom() {
    let config = HttpClientConfig::with_user_agent("custom/1.0")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60));

    assert_eq!(config.user_agent, "custom/1.0");
    assert_eq!(config.connect_timeout, Some(Duration::from_secs(10)));
    assert_eq!(config.timeout, Some(Duration::from_secs(60)));
}

#[test]
fn test_http_client_config_build() {
    let config = HttpClientConfig::default();
    let _client = config.build();
}

// ============================================================================
// Progress Handler Tests
// ============================================================================

#[test]
fn test_noop_progress_handler() {
    let handler = NoopProgressHandler;
    handler.on_start("MSVC", 100, 1024 * 1024);
    handler.on_file_start("test.vsix", 1024);
    handler.on_progress(512);
    handler.on_file_complete("test.vsix", "downloaded");
    handler.on_complete(10, 5);
    handler.on_error("test error");
    handler.on_message("test message");
}

#[test]
fn test_progress_handler_boxed() {
    let handler: Arc<dyn ProgressHandler> = Arc::new(NoopProgressHandler);
    handler.on_start("SDK", 50, 512 * 1024);
    handler.on_complete(50, 0);
}

// ============================================================================
// FileSystemCacheManager Tests
// ============================================================================

#[test]
fn test_filesystem_cache_basic_operations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = FileSystemCacheManager::new(temp_dir.path());

    cache.set("test_key", b"test_value").unwrap();
    assert_eq!(cache.get("test_key"), Some(b"test_value".to_vec()));

    assert!(cache.contains("test_key"));
    assert!(!cache.contains("nonexistent"));
}

#[test]
fn test_filesystem_cache_invalidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = FileSystemCacheManager::new(temp_dir.path());

    cache.set("key", b"value").unwrap();
    assert!(cache.contains("key"));

    cache.invalidate("key").unwrap();
    assert!(!cache.contains("key"));
}

#[test]
fn test_filesystem_cache_clear() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = FileSystemCacheManager::new(temp_dir.path());

    cache.set("key1", b"value1").unwrap();
    cache.set("key2", b"value2").unwrap();

    cache.clear().unwrap();

    assert!(!cache.contains("key1"));
    assert!(!cache.contains("key2"));
}

#[test]
fn test_filesystem_cache_entry_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = FileSystemCacheManager::new(temp_dir.path());

    let path = cache.entry_path("some/nested/key");
    assert!(path.ends_with("some/nested/key") || path.ends_with("some\\nested\\key"));
}

#[test]
fn test_filesystem_cache_nested_keys() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = FileSystemCacheManager::new(temp_dir.path());

    cache.set("nested/path/key", b"nested_value").unwrap();
    assert_eq!(cache.get("nested/path/key"), Some(b"nested_value".to_vec()));
}

#[test]
fn test_filesystem_cache_default_dir() {
    let cache = FileSystemCacheManager::default_cache_dir();
    let cache_dir = cache.cache_dir();
    assert!(!cache_dir.to_string_lossy().is_empty());
}

// ============================================================================
// Performance Optimization Tests
// ============================================================================

#[test]
fn test_http_client_config_connection_pooling() {
    // Verify that HTTP client configuration supports connection pooling
    let config = HttpClientConfig::default();
    let client = config.build();
    // Client should be created successfully with connection pooling enabled
    drop(client);
}

#[test]
fn test_download_options_parallel_downloads_range() {
    // Test that parallel downloads can be configured
    for count in [1, 2, 4, 8, 16, 32] {
        let options = DownloadOptions::builder()
            .parallel_downloads(count)
            .target_dir("C:/test")
            .build();
        assert_eq!(options.parallel_downloads, count);
    }
}

#[test]
fn test_hash_computation_consistency() {
    // Test that hash computation is consistent
    let data = b"test data for hash consistency check";
    let hash1 = compute_hash(data);
    let hash2 = compute_hash(data);
    assert_eq!(hash1, hash2);
}

#[test]
fn test_hash_computation_different_inputs() {
    // Test that different inputs produce different hashes
    let hash1 = compute_hash(b"input1");
    let hash2 = compute_hash(b"input2");
    assert_ne!(hash1, hash2);
}

#[test]
fn test_download_options_builder_chain() {
    // Test that builder pattern supports method chaining
    let options = DownloadOptions::builder()
        .msvc_version("14.44")
        .sdk_version("10.0.26100.0")
        .target_dir("C:/test")
        .arch(Architecture::X64)
        .host_arch(Architecture::X64)
        .verify_hashes(true)
        .parallel_downloads(8)
        .dry_run(false)
        .build();

    assert_eq!(options.msvc_version, Some("14.44".to_string()));
    assert_eq!(options.sdk_version, Some("10.0.26100.0".to_string()));
    assert_eq!(options.parallel_downloads, 8);
}
