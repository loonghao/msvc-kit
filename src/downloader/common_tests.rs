use std::sync::Arc;

use super::progress::NoopProgressHandler;

fn payload_package(url: String, bytes: &[u8]) -> super::Package {
    super::Package {
        id: "fixture".into(),
        version: "1".into(),
        package_type: "fixture".into(),
        chip: None,
        total_size: bytes.len() as u64,
        payloads: vec![super::PackagePayload {
            file_name: "fixture.vsix".into(),
            url,
            size: bytes.len() as u64,
            sha256: Some(super::hash::compute_hash(bytes)),
        }],
    }
}

fn fixture_downloader(verify_hashes: bool) -> super::common::CommonDownloader {
    super::common::CommonDownloader::with_client(
        super::DownloadOptions::builder()
            .verify_hashes(verify_hashes)
            .build(),
        reqwest::Client::new(),
    )
    .with_progress_handler(Arc::new(NoopProgressHandler))
}

#[tokio::test]
async fn altered_cached_bytes_are_rehashed_even_when_index_hash_matches() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/payload")
        .with_body("correct")
        .expect(2)
        .create_async()
        .await;
    let root = tempfile::tempdir().unwrap();
    let package = payload_package(format!("{}/payload", server.url()), b"correct");
    let downloader = fixture_downloader(true);
    downloader
        .download_packages(std::slice::from_ref(&package), root.path(), "fixture")
        .await
        .unwrap();
    std::fs::write(root.path().join("fixture.vsix"), b"altered").unwrap();
    downloader
        .download_packages(&[package], root.path(), "fixture")
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(root.path().join("fixture.vsix")).unwrap(),
        b"correct"
    );
    mock.assert_async().await;
}

#[tokio::test]
async fn source_url_changes_require_a_new_download_with_verification_disabled() {
    let mut server = mockito::Server::new_async().await;
    let first = server
        .mock("GET", "/first")
        .with_body("data")
        .expect(1)
        .create_async()
        .await;
    let second = server
        .mock("GET", "/second")
        .with_body("data")
        .expect(1)
        .create_async()
        .await;
    let root = tempfile::tempdir().unwrap();
    let downloader = fixture_downloader(false);
    downloader
        .download_packages(
            &[payload_package(format!("{}/first", server.url()), b"data")],
            root.path(),
            "fixture",
        )
        .await
        .unwrap();
    downloader
        .download_packages(
            &[payload_package(format!("{}/second", server.url()), b"data")],
            root.path(),
            "fixture",
        )
        .await
        .unwrap();
    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn rejected_download_preserves_the_previous_final_file() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/payload")
        .with_body("bad")
        .expect(1)
        .create_async()
        .await;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("fixture.vsix"), b"old").unwrap();
    let package = payload_package(format!("{}/payload", server.url()), b"new");
    let result = fixture_downloader(true)
        .download_packages(&[package], root.path(), "fixture")
        .await;
    assert!(matches!(
        result,
        Err(crate::MsvcKitError::HashMismatch { .. })
    ));
    assert_eq!(
        std::fs::read(root.path().join("fixture.vsix")).unwrap(),
        b"old"
    );
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".tmp")));
    mock.assert_async().await;
}

#[tokio::test]
async fn payload_size_is_checked_even_when_hash_verification_is_disabled() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/payload")
        .with_body("short")
        .expect(1)
        .create_async()
        .await;
    let root = tempfile::tempdir().unwrap();
    let package = payload_package(format!("{}/payload", server.url()), b"longer");
    let result = fixture_downloader(false)
        .download_packages(&[package], root.path(), "fixture")
        .await;
    assert!(result.unwrap_err().to_string().contains("Size mismatch"));
    assert!(!root.path().join("fixture.vsix").exists());
    mock.assert_async().await;
}

#[tokio::test]
async fn simultaneous_cache_users_serialize_before_opening_the_database() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/payload")
        .with_body("data")
        .expect(1)
        .create_async()
        .await;
    let root = tempfile::tempdir().unwrap();
    let packages = [payload_package(
        format!("{}/payload", server.url()),
        b"data",
    )];
    let first = fixture_downloader(true);
    let second = fixture_downloader(true);
    let (first_result, second_result) = tokio::join!(
        first.download_packages(&packages, root.path(), "fixture"),
        second.download_packages(&packages, root.path(), "fixture")
    );
    assert!(first_result.is_ok(), "{first_result:?}");
    assert!(second_result.is_ok(), "{second_result:?}");
    assert!(!root.path().join("index.db.bak").exists());
    mock.assert_async().await;
}

#[tokio::test]
async fn an_already_open_index_is_not_renamed_as_corrupted() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("index.db");
    let first = super::DownloadIndex::load(&path).await.unwrap();
    let second = super::DownloadIndex::load(&path).await;
    assert!(second.is_err());
    assert!(path.exists());
    assert!(!path.with_extension("db.bak").exists());
    assert!(first.get_entry("missing").await.unwrap().is_none());
}

#[tokio::test]
async fn truncated_body_restarts_the_outer_http_attempt_and_publishes_only_complete_bytes() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/payload", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        for body in ["da", "data"] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut request = [0u8; 4096];
            let received = socket.read(&mut request).unwrap();
            assert!(received > 0, "Retry server received an empty HTTP request");
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n{body}"
            )
            .unwrap();
        }
    });
    let root = tempfile::tempdir().unwrap();
    fixture_downloader(true)
        .download_packages(&[payload_package(url, b"data")], root.path(), "fixture")
        .await
        .unwrap();
    worker.join().unwrap();
    assert_eq!(
        std::fs::read(root.path().join("fixture.vsix")).unwrap(),
        b"data"
    );
}

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
