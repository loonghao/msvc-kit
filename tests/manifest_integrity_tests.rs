//! Manifest trust must come from a fresh channel, never mutable cache metadata.

use indicatif::ProgressBar;
use msvc_kit::downloader::cache::{
    fetch_bytes_with_cache, fetch_verified_bytes_with_cache, meta_path_for,
};
use msvc_kit::downloader::{hash::compute_hash, VsManifest};
use msvc_kit::vs_channel::VsChannelSpec;

#[tokio::test]
async fn forged_cache_body_and_metadata_cannot_authorize_not_modified() {
    let mut server = mockito::Server::new_async().await;
    let full = server
        .mock("GET", "/channel")
        .match_header("if-none-match", mockito::Matcher::Missing)
        .with_header("ETag", "official-version")
        .with_body("official")
        .expect(2)
        .create_async()
        .await;
    let conditional = server
        .mock("GET", "/channel")
        .match_header("if-none-match", "official-version")
        .with_status(304)
        .expect(0)
        .create_async()
        .await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("channel.json");
    let url = format!("{}/channel", server.url());
    let client = reqwest::Client::new();
    let spinner = ProgressBar::hidden();
    fetch_bytes_with_cache(&client, &url, &path, &spinner, "channel", "channel.json")
        .await
        .unwrap();
    std::fs::write(&path, b"poisoned").unwrap();
    let metadata = meta_path_for(&path);
    let mut meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    meta["size"] = serde_json::json!(8);
    meta["sha256"] = serde_json::json!(compute_hash(b"poisoned"));
    std::fs::write(&metadata, serde_json::to_vec(&meta).unwrap()).unwrap();
    let (body, cached) =
        fetch_bytes_with_cache(&client, &url, &path, &spinner, "channel", "channel.json")
            .await
            .unwrap();
    assert_eq!(body, b"official");
    assert!(!cached);
    full.assert_async().await;
    conditional.assert_async().await;
}

#[tokio::test]
async fn package_manifest_must_match_fresh_channel_digest_before_publication() {
    let mut server = mockito::Server::new_async().await;
    let manifest_url = format!("{}/manifest", server.url());
    let channel = serde_json::json!({"manifestVersion":"1.1", "channelItems":[{
        "id":"Microsoft.VisualStudio.Manifests.VisualStudio", "version":"17.14", "type":"Manifest",
        "payloads":[{"fileName":"VisualStudio.vsman", "url":manifest_url, "size":1,
                     "sha256":compute_hash(b"different official manifest")}] }]});
    let channel_mock = server
        .mock("GET", "/channel")
        .with_body(channel.to_string())
        .create_async()
        .await;
    let manifest_mock = server.mock("GET", "/manifest").with_body(
        r#"{"manifestVersion":"1.1","packages":[{"id":"fixture","version":"1","type":"Vsix"}]}"#)
        .create_async().await;
    let root = tempfile::tempdir().unwrap();
    let channel = VsChannelSpec::new(17, Some(2022), format!("{}/channel", server.url()));
    let result = VsManifest::fetch_with_channel(&channel, root.path()).await;
    assert!(
        result.is_err(),
        "A valid JSON manifest with a wrong official SHA must fail"
    );
    assert!(!root.path().join("vsman/VisualStudio.vsman").exists());
    channel_mock.assert_async().await;
    manifest_mock.assert_async().await;
}

#[tokio::test]
async fn trusted_digest_rejects_forged_metadata_and_allows_valid_304() {
    let mut server = mockito::Server::new_async().await;
    let full = server
        .mock("GET", "/manifest")
        .match_header("if-none-match", mockito::Matcher::Missing)
        .with_header("ETag", "official-version")
        .with_body("official")
        .expect(2)
        .create_async()
        .await;
    let conditional = server
        .mock("GET", "/manifest")
        .match_header("if-none-match", "official-version")
        .with_status(304)
        .expect(1)
        .create_async()
        .await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("manifest.json");
    let url = format!("{}/manifest", server.url());
    let client = reqwest::Client::new();
    let spinner = ProgressBar::hidden();
    let expected = compute_hash(b"official");
    fetch_verified_bytes_with_cache(
        &client,
        &url,
        &path,
        &spinner,
        "manifest",
        "manifest.json",
        Some(&expected),
    )
    .await
    .unwrap();
    let (body, cached) = fetch_verified_bytes_with_cache(
        &client,
        &url,
        &path,
        &spinner,
        "manifest",
        "manifest.json",
        Some(&expected),
    )
    .await
    .unwrap();
    assert_eq!(body, b"official");
    assert!(cached);
    std::fs::write(&path, b"poisoned").unwrap();
    let metadata = meta_path_for(&path);
    let mut meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    meta["size"] = serde_json::json!(8);
    meta["sha256"] = serde_json::json!(compute_hash(b"poisoned"));
    std::fs::write(&metadata, serde_json::to_vec(&meta).unwrap()).unwrap();
    let (body, cached) = fetch_verified_bytes_with_cache(
        &client,
        &url,
        &path,
        &spinner,
        "manifest",
        "manifest.json",
        Some(&expected),
    )
    .await
    .unwrap();
    assert_eq!(body, b"official");
    assert!(!cached);
    full.assert_async().await;
    conditional.assert_async().await;
}

#[tokio::test]
async fn stale_cache_and_failed_refresh_preserve_old_bytes_but_return_error() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("GET", "/manifest")
        .with_body("invalid replacement")
        .create_async()
        .await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("manifest.json");
    std::fs::write(&path, b"previous manifest").unwrap();
    let result = fetch_verified_bytes_with_cache(
        &reqwest::Client::new(),
        &format!("{}/manifest", server.url()),
        &path,
        &ProgressBar::hidden(),
        "manifest",
        "manifest.json",
        Some(&compute_hash(b"current manifest")),
    )
    .await;
    assert!(matches!(
        result,
        Err(msvc_kit::MsvcKitError::HashMismatch { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"previous manifest");
    request.assert_async().await;
}
