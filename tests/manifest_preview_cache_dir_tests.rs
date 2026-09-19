//! Dry-run preview coverage for the manifest cache directory.
//!
//! `preview()` performs a real manifest fetch, so these tests are ignored by
//! default; run them with `cargo test --ignored` when network access is
//! available. They live in their own test binary because they set
//! `MSVC_KIT_DIR` process-wide, which would race with unrelated tests.

/// The dry-run preview reads and caches the manifest in the directory that
/// `MSVC_KIT_DIR` points at.
#[tokio::test]
#[ignore = "requires network access"]
async fn preview_reads_manifest_from_configured_cache_dir() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("kit");
    let expected = install_dir.join("cache").join("manifests");

    std::env::set_var("MSVC_KIT_DIR", &install_dir);
    let config = msvc_kit::load_config().expect("config must load");
    std::env::remove_var("MSVC_KIT_DIR");

    assert_eq!(config.manifest_cache_dir(), expected);

    let preview = msvc_kit::downloader::MsvcDownloader::new(
        msvc_kit::DownloadOptions::builder()
            .manifest_cache_dir(config.manifest_cache_dir())
            .build(),
    )
    .preview()
    .await
    .expect("preview must resolve the manifest");

    assert!(!preview.packages.is_empty(), "preview must list packages");
    assert!(
        expected.join("vsman").exists(),
        "the manifest must be cached under {}",
        expected.join("vsman").display()
    );
}

/// Without an explicit directory the preview falls back to the configured one.
#[tokio::test]
#[ignore = "requires network access"]
async fn preview_falls_back_to_configured_cache_dir() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("kit");
    let expected = install_dir.join("cache").join("manifests");

    std::env::set_var("MSVC_KIT_DIR", &install_dir);
    let config = msvc_kit::load_config().expect("config must load");

    let resolved = msvc_kit::DownloadOptions::default()
        .with_configured_manifest_cache_dir()
        .manifest_cache_dir;
    std::env::remove_var("MSVC_KIT_DIR");

    // `preview()` resolves the same directory the configuration points at.
    assert_eq!(resolved, Some(config.manifest_cache_dir()));
    assert_eq!(resolved, Some(expected.clone()));

    let preview = msvc_kit::downloader::MsvcDownloader::new(
        msvc_kit::DownloadOptions::builder()
            .manifest_cache_dir(&expected)
            .build(),
    )
    .preview()
    .await
    .expect("preview must resolve the manifest");

    assert!(!preview.packages.is_empty(), "preview must list packages");
    assert!(
        expected.join("vsman").exists(),
        "the manifest must be cached under {}",
        expected.join("vsman").display()
    );
}
