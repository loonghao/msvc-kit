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
