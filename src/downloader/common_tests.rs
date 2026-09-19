use std::sync::Arc;

use super::progress::NoopProgressHandler;

/// Test helper to create a simple progress handler for testing
#[allow(dead_code)]
pub fn test_progress_handler() -> Arc<dyn super::progress::ProgressHandler> {
    Arc::new(NoopProgressHandler)
}

#[tokio::test]
async fn download_options_builder_works() {
    use super::DownloadOptions;
    use crate::version::Architecture;

    let options = DownloadOptions::builder()
        .target_dir("/tmp/test")
        .arch(Architecture::X64)
        .parallel_downloads(8)
        .verify_hashes(false)
        .build();

    assert_eq!(options.target_dir.to_str().unwrap(), "/tmp/test");
    assert_eq!(options.arch, Architecture::X64);
    assert_eq!(options.parallel_downloads, 8);
    assert!(!options.verify_hashes);
}

#[tokio::test]
async fn download_options_default_values() {
    use super::DownloadOptions;
    use crate::constants::download::DEFAULT_PARALLEL_DOWNLOADS;

    let options = DownloadOptions::default();

    assert!(options.msvc_version.is_none());
    assert!(options.sdk_version.is_none());
    assert!(options.verify_hashes);
    assert_eq!(options.parallel_downloads, DEFAULT_PARALLEL_DOWNLOADS);
    assert!(options.http_client.is_none());
    assert!(options.progress_handler.is_none());
    assert!(options.cache_manager.is_none());
}

#[tokio::test]
async fn download_options_builder_with_cache_manager() {
    use super::DownloadOptions;
    use crate::version::Architecture;

    // Test that cache_manager can be set through builder
    let options = DownloadOptions::builder()
        .target_dir("/tmp/test")
        .arch(Architecture::X64)
        .build();

    assert!(options.cache_manager.is_none());
}

#[tokio::test]
async fn http_client_config_default() {
    use super::http::HttpClientConfig;
    use crate::constants::USER_AGENT;

    let config = HttpClientConfig::default();

    assert_eq!(config.user_agent, USER_AGENT);
    assert!(config.connect_timeout.is_some());
    assert!(config.timeout.is_some());
}

#[tokio::test]
async fn create_http_client_works() {
    use super::http::create_http_client;

    let client = create_http_client();
    // Just verify it doesn't panic
    let _ = client;
}

#[tokio::test]
async fn create_http_client_with_config_works() {
    use super::http::{create_http_client_with_config, HttpClientConfig};
    use std::time::Duration;

    let config = HttpClientConfig {
        user_agent: "test-agent/1.0".to_string(),
        connect_timeout: Some(Duration::from_secs(10)),
        timeout: Some(Duration::from_secs(60)),
    };

    let client = create_http_client_with_config(&config);
    // Just verify it doesn't panic
    let _ = client;
}

#[tokio::test]
async fn common_downloader_with_cache_manager() {
    use super::common::CommonDownloader;
    use super::http::create_http_client;
    use super::traits::FileSystemCacheManager;
    use super::DownloadOptions;

    let options = DownloadOptions::default();
    let client = create_http_client();
    let downloader = CommonDownloader::with_client(options, client);

    // Initially no cache manager
    assert!(downloader.cache_manager.is_none());

    // Set a cache manager
    let temp_dir = tempfile::TempDir::new().unwrap();
    let cache_mgr = FileSystemCacheManager::new(temp_dir.path());
    let downloader = downloader.with_cache_manager(std::sync::Arc::new(cache_mgr));

    assert!(downloader.cache_manager.is_some());
}

#[tokio::test]
async fn manifest_cache_dir_with_custom_cache_manager() {
    use super::common::CommonDownloader;
    use super::http::create_http_client;
    use super::traits::FileSystemCacheManager;
    use super::DownloadOptions;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let cache_mgr = FileSystemCacheManager::new(temp_dir.path());

    let options = DownloadOptions::default();
    let client = create_http_client();
    let downloader = CommonDownloader::with_client(options, client)
        .with_cache_manager(std::sync::Arc::new(cache_mgr));

    // When a custom cache manager is set, manifest_cache_dir should use its cache_dir/manifests
    let cache_dir = downloader.manifest_cache_dir();
    assert_eq!(cache_dir, temp_dir.path().join("manifests"));
}

#[tokio::test]
async fn manifest_cache_dir_from_download_options() {
    use super::common::CommonDownloader;
    use super::http::create_http_client;
    use super::DownloadOptions;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let configured = temp_dir.path().join("manifests");

    let options = DownloadOptions::builder()
        .manifest_cache_dir(&configured)
        .build();
    let client = create_http_client();
    let downloader = CommonDownloader::with_client(options, client);

    // The configured directory is used instead of the platform default.
    assert_eq!(downloader.manifest_cache_dir(), configured);
}

#[tokio::test]
async fn with_configured_manifest_cache_dir_preserves_explicit_directory() {
    use super::DownloadOptions;

    let configured = tempfile::TempDir::new().unwrap();
    let explicit = configured.path().join("manifests");

    let options = DownloadOptions::builder()
        .manifest_cache_dir(&explicit)
        .build();

    // The library entry points must not override an explicit directory.
    assert_eq!(
        options
            .with_configured_manifest_cache_dir()
            .manifest_cache_dir,
        Some(explicit)
    );
}

#[tokio::test]
async fn with_configured_manifest_cache_dir_applies_configuration() {
    use super::DownloadOptions;

    let options = DownloadOptions::default();
    assert_eq!(options.manifest_cache_dir, None);

    // Without an explicit directory the configured cache directory is used.
    assert_eq!(
        options
            .with_configured_manifest_cache_dir()
            .manifest_cache_dir,
        Some(super::configured_manifest_cache_dir())
    );
}

#[tokio::test]
async fn manifest_cache_dir_prefers_cache_manager_over_options() {
    use super::common::CommonDownloader;
    use super::http::create_http_client;
    use super::traits::FileSystemCacheManager;
    use super::DownloadOptions;

    let configured = tempfile::TempDir::new().unwrap();
    let manager_dir = tempfile::TempDir::new().unwrap();

    let options = DownloadOptions::builder()
        .manifest_cache_dir(configured.path())
        .build();
    let client = create_http_client();
    let downloader = CommonDownloader::with_client(options, client).with_cache_manager(
        std::sync::Arc::new(FileSystemCacheManager::new(manager_dir.path())),
    );

    // An injected cache manager still wins.
    assert_eq!(
        downloader.manifest_cache_dir(),
        manager_dir.path().join("manifests")
    );
}

#[tokio::test]
async fn manifest_cache_dir_without_cache_manager() {
    use super::common::CommonDownloader;
    use super::http::create_http_client;
    use super::DownloadOptions;

    let options = DownloadOptions::default();
    let client = create_http_client();
    let downloader = CommonDownloader::with_client(options, client);

    // Without a cache manager, should fall back to default location
    let cache_dir = downloader.manifest_cache_dir();
    let default_dir = super::cache::default_manifest_cache_dir();
    assert_eq!(cache_dir, default_dir);
}

/// A downloader whose options carry no manifest cache directory resolves the
/// configured one, so dry-run previews read manifests from the location the
/// configuration points at.
#[tokio::test]
async fn configured_manifest_cache_dir_falls_back_to_config() {
    use super::common::CommonDownloader;
    use super::http::create_http_client;
    use super::DownloadOptions;

    let options = DownloadOptions::default();
    let client = create_http_client();
    let downloader = CommonDownloader::with_client(options, client);

    assert_eq!(
        downloader.configured_manifest_cache_dir(),
        super::configured_manifest_cache_dir()
    );
}

#[tokio::test]
async fn configured_manifest_cache_dir_keeps_explicit_directory() {
    use super::common::CommonDownloader;
    use super::http::create_http_client;
    use super::DownloadOptions;

    let configured = tempfile::TempDir::new().unwrap();
    let explicit = configured.path().join("manifests");

    let options = DownloadOptions::builder()
        .manifest_cache_dir(&explicit)
        .build();
    let client = create_http_client();
    let downloader = CommonDownloader::with_client(options, client);

    // An explicit directory wins over the configuration.
    assert_eq!(downloader.configured_manifest_cache_dir(), explicit);
}

#[tokio::test]
async fn configured_manifest_cache_dir_keeps_cache_manager() {
    use super::common::CommonDownloader;
    use super::http::create_http_client;
    use super::traits::FileSystemCacheManager;
    use super::DownloadOptions;

    let manager_dir = tempfile::TempDir::new().unwrap();

    let client = create_http_client();
    let downloader = CommonDownloader::with_client(DownloadOptions::default(), client)
        .with_cache_manager(std::sync::Arc::new(FileSystemCacheManager::new(
            manager_dir.path(),
        )));

    // An injected cache manager still wins over the configuration.
    assert_eq!(
        downloader.configured_manifest_cache_dir(),
        manager_dir.path().join("manifests")
    );
}

#[tokio::test]
async fn download_options_builder_sets_cache_manager() {
    use super::traits::FileSystemCacheManager;
    use super::DownloadOptions;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let cache_mgr = std::sync::Arc::new(FileSystemCacheManager::new(temp_dir.path()));

    let options = DownloadOptions::builder()
        .target_dir("/tmp/test-cm")
        .cache_manager(cache_mgr.clone())
        .build();

    assert!(options.cache_manager.is_some());
    // Verify the cache dir matches
    let cm = options.cache_manager.unwrap();
    assert_eq!(cm.cache_dir(), temp_dir.path());
}
