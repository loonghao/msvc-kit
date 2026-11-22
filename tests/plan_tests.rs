//! Install plan tests: plan identity stability, JSON round-trip,
//! no-download/no-write guarantees, cache observations, blocking conflicts
//! and stale-plan detection (spec 08 §7).

use std::path::{Path, PathBuf};

use msvc_kit::downloader::hash::compute_hash;
use msvc_kit::{
    create_install_plan, load_and_verify_plan, revalidate_plan, verify_downloaded_against_plan,
    PlanActionKind, PlanOptions,
};

const MSVC_PACKAGE_ID: &str = "Microsoft.VC.14.44.Tools.HostX64.TargetX64";
const PAYLOAD_NAME: &str = "test-tools.vsix";
const PAYLOAD_ENTRY: &str = "VC/Tools/plan-check.txt";

/// Build the VSIX payload bytes served as the package payload.
fn payload_bytes() -> Vec<u8> {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(file.path()).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(PAYLOAD_ENTRY, options).unwrap();
    use std::io::Write as _;
    zip.write_all(b"plan-check").unwrap();
    zip.finish().unwrap();
    std::fs::read(file.path()).unwrap()
}

/// One MSVC tools package with a single payload pointing at `payload_url`.
fn msvc_manifest_body(payload_url: &str, payload_sha: &str, payload_size: u64) -> String {
    serde_json::json!({
        "manifestVersion": "1.0",
        "packages": [
            {
                "id": MSVC_PACKAGE_ID,
                "version": "14.44.34823",
                "type": "VSIX",
                "chip": null,
                "language": null,
                "payloads": [
                    {
                        "fileName": PAYLOAD_NAME,
                        "url": payload_url,
                        "sha256": payload_sha,
                        "size": payload_size
                    }
                ],
                "dependencies": {},
                "machineArch": null,
                "productArch": null
            }
        ]
    })
    .to_string()
}

/// Channel manifest body pointing at the package manifest and pinning its
/// official digest (the fetch verifies the manifest against this digest).
fn channel_body(manifest_url: &str, manifest_sha: &str) -> String {
    serde_json::json!({
        "manifestVersion": "1.1",
        "info": {"productDisplayVersion": "17.14"},
        "channelItems": [
            {
                "id": "Microsoft.VisualStudio.Manifests.VisualStudio",
                "version": "17.14",
                "type": "Manifest",
                "payloads": [
                    {
                        "fileName": "VisualStudio.vsman",
                        "url": manifest_url,
                        "size": 1,
                        "sha256": manifest_sha
                    }
                ]
            }
        ]
    })
    .to_string()
}

/// Fetch-count choreography: the channel manifest is always fetched fully
/// (it has no independent digest to authorize a 304), while the package
/// manifest uses conditional requests once its cache metadata exists.
struct FetchCounts {
    channel_full: usize,
    vsman_full: usize,
    vsman_304: usize,
}

/// Serve a channel + package manifest pair on the mock server and return the
/// channel selector URL.
async fn serve_manifests(
    server: &mut mockito::Server,
    channel: &str,
    vsman: &str,
    counts: FetchCounts,
) -> String {
    let _channel_full = server
        .mock("GET", "/channel")
        .match_header("if-none-match", mockito::Matcher::Missing)
        .with_header("ETag", "channel-1")
        .with_body(channel)
        .expect(counts.channel_full)
        .create_async()
        .await;
    let _vsman_full = server
        .mock("GET", "/manifest")
        .match_header("if-none-match", mockito::Matcher::Missing)
        .with_header("ETag", "vsman-1")
        .with_body(vsman)
        .expect(counts.vsman_full)
        .create_async()
        .await;
    let _vsman_304 = server
        .mock("GET", "/manifest")
        .match_header("if-none-match", "vsman-1")
        .with_status(304)
        .expect(counts.vsman_304)
        .create_async()
        .await;
    format!("{}/channel", server.url())
}

/// Register the payload route with an exact-path matcher and zero expected
/// hits, so any planning-time payload download fails the test.
async fn forbid_payload(server: &mut mockito::Server) {
    let _payload = server
        .mock("GET", "/payload")
        .with_status(500)
        .expect(0)
        .create_async()
        .await;
}

fn plan_options(selector: &str, cache_dir: &Path, target_root: &Path) -> PlanOptions {
    PlanOptions {
        msvc_version: Some("14.44".to_string()),
        sdk_version: None,
        target_dir: target_root.to_path_buf(),
        arch: msvc_kit::Architecture::X64,
        host_arch: Some(msvc_kit::Architecture::X64),
        vs_channel: Some(selector.to_string()),
        include_components: vec![],
        exclude_patterns: vec![],
        manifest_cache_dir: cache_dir.to_path_buf(),
        include_msvc: true,
        include_sdk: false,
    }
}

/// The cache path the planner (and the downloader) use for the fixture payload.
fn planned_payload_path(target_root: &Path) -> PathBuf {
    target_root
        .join("downloads")
        .join("msvc")
        .join("14_44_x64_x64")
        .join(PAYLOAD_NAME)
}

#[tokio::test]
async fn plan_is_stable_round_trips_and_never_downloads() {
    let mut server = mockito::Server::new_async().await;
    let payload = payload_bytes();
    let payload_sha = compute_hash(&payload);
    let vsman = msvc_manifest_body(
        &format!("{}/payload", server.url()),
        &payload_sha,
        payload.len() as u64,
    );
    let vsman_sha = compute_hash(vsman.as_bytes());
    let channel = channel_body(&format!("{}/manifest", server.url()), &vsman_sha);
    let selector = serve_manifests(
        &mut server,
        &channel,
        &vsman,
        FetchCounts {
            channel_full: 2,
            vsman_full: 1,
            vsman_304: 1,
        },
    )
    .await;
    forbid_payload(&mut server).await;

    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");

    let options = plan_options(&selector, cache.path(), &target);
    let plan_a = create_install_plan(&options).await.unwrap();
    let plan_b = create_install_plan(&options).await.unwrap();

    // Same state, same identity.
    assert_eq!(plan_a.plan_id, plan_b.plan_id);

    // The plan pins the payload and plans exactly download + extract.
    assert_eq!(plan_a.resolved.packages, vec![MSVC_PACKAGE_ID.to_string()]);
    assert_eq!(plan_a.resolved.payloads.len(), 1);
    assert_eq!(
        plan_a.resolved.payloads[0].sha256.as_deref(),
        Some(payload_sha.as_str())
    );
    let kinds: Vec<PlanActionKind> = plan_a.actions.iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        vec![
            PlanActionKind::DownloadPayload,
            PlanActionKind::ExtractPackage
        ]
    );
    assert_eq!(plan_a.disk_usage.download_bytes, payload.len() as u64);
    assert_eq!(plan_a.disk_usage.reused_cache_bytes, 0);
    assert!(plan_a.blocking_conflicts.is_empty());

    // Planning must not write the target root at all.
    assert!(!target.exists(), "planning created the target root");

    // JSON round-trip preserves the document and its identity.
    let text = serde_json::to_string_pretty(&plan_a).unwrap();
    let reparsed: msvc_kit::InstallPlanDocument = serde_json::from_str(&text).unwrap();
    assert_eq!(plan_a, reparsed);
    assert!(reparsed.verify_plan_id().is_ok());

    // The saved plan file loads and verifies; tampering breaks it.
    let plan_file = root.path().join("plan.json");
    std::fs::write(&plan_file, &text).unwrap();
    let loaded = load_and_verify_plan(&plan_file).unwrap();
    assert_eq!(loaded.plan_id, plan_a.plan_id);
    let mut tampered = loaded.clone();
    tampered.warnings.push("tampered".to_string());
    assert!(tampered.verify_plan_id().is_err());
    let tampered_text = serde_json::to_string_pretty(&tampered).unwrap();
    std::fs::write(&plan_file, tampered_text).unwrap();
    assert!(load_and_verify_plan(&plan_file).is_err());
}

#[tokio::test]
async fn plan_reports_verified_cache_hits() {
    let mut server = mockito::Server::new_async().await;
    let payload = payload_bytes();
    let payload_sha = compute_hash(&payload);
    let vsman = msvc_manifest_body(
        &format!("{}/payload", server.url()),
        &payload_sha,
        payload.len() as u64,
    );
    let vsman_sha = compute_hash(vsman.as_bytes());
    let channel = channel_body(&format!("{}/manifest", server.url()), &vsman_sha);
    // Plan once, then two revalidations.
    let selector = serve_manifests(
        &mut server,
        &channel,
        &vsman,
        FetchCounts {
            channel_full: 3,
            vsman_full: 1,
            vsman_304: 2,
        },
    )
    .await;
    forbid_payload(&mut server).await;

    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");

    // Pre-place the payload exactly where the downloader would put it.
    let cache_path = planned_payload_path(&target);
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    std::fs::write(&cache_path, &payload).unwrap();

    let plan = create_install_plan(&plan_options(&selector, cache.path(), &target))
        .await
        .unwrap();
    let kinds: Vec<PlanActionKind> = plan.actions.iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        vec![
            PlanActionKind::ReuseCachedPayload,
            PlanActionKind::ExtractPackage
        ]
    );
    assert_eq!(plan.disk_usage.download_bytes, 0);
    assert_eq!(plan.disk_usage.reused_cache_bytes, payload.len() as u64);
    assert!(plan.blocking_conflicts.is_empty());
    assert!(plan.warnings.is_empty());

    // Revalidation of the fresh plan passes: cache and manifest unchanged.
    let revalidation = revalidate_plan(&plan, cache.path()).await.unwrap();
    assert!(!revalidation.is_stale(), "{:?}", revalidation.stale_reasons);

    // Corrupting the cached payload makes revalidation stale...
    std::fs::write(&cache_path, b"corrupted bytes").unwrap();
    let revalidation = revalidate_plan(&plan, cache.path()).await.unwrap();
    assert!(revalidation.is_stale());
    assert!(
        revalidation.stale_reasons[0].contains("no longer matches its planned digest"),
        "{:?}",
        revalidation.stale_reasons
    );
}

#[tokio::test]
async fn plan_downloads_against_corrupted_cache_with_warning() {
    let mut server = mockito::Server::new_async().await;
    let payload = payload_bytes();
    let payload_sha = compute_hash(&payload);
    let vsman = msvc_manifest_body(
        &format!("{}/payload", server.url()),
        &payload_sha,
        payload.len() as u64,
    );
    let vsman_sha = compute_hash(vsman.as_bytes());
    let channel = channel_body(&format!("{}/manifest", server.url()), &vsman_sha);
    let selector = serve_manifests(
        &mut server,
        &channel,
        &vsman,
        FetchCounts {
            channel_full: 1,
            vsman_full: 1,
            vsman_304: 0,
        },
    )
    .await;
    forbid_payload(&mut server).await;

    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");

    // Corrupted cache entry: right size, wrong bytes.
    let cache_path = planned_payload_path(&target);
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    std::fs::write(&cache_path, vec![0u8; payload.len()]).unwrap();

    let plan = create_install_plan(&plan_options(&selector, cache.path(), &target))
        .await
        .unwrap();
    let kinds: Vec<PlanActionKind> = plan.actions.iter().map(|a| a.kind).collect();
    assert_eq!(kinds[0], PlanActionKind::DownloadPayload);
    assert_eq!(plan.disk_usage.download_bytes, payload.len() as u64);
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.contains("does not match its official SHA256")),
        "corrupt cache must produce a warning: {:?}",
        plan.warnings
    );
}

#[tokio::test]
async fn plan_marks_conflicting_payload_identities_blocking() {
    let mut server = mockito::Server::new_async().await;
    let payload = payload_bytes();
    let payload_sha = compute_hash(&payload);
    // Two packages share the payload file name with different digests.
    let vsman = serde_json::json!({
        "manifestVersion": "1.0",
        "packages": [
            {
                "id": MSVC_PACKAGE_ID,
                "version": "14.44.34823",
                "type": "VSIX",
                "payloads": [{"fileName": PAYLOAD_NAME, "url": format!("{}/payload-a", server.url()), "sha256": payload_sha, "size": payload.len()}]
            },
            {
                "id": "Microsoft.VC.14.44.CRT.x64.Desktop",
                "version": "14.44.34823",
                "type": "VSIX",
                "payloads": [{"fileName": PAYLOAD_NAME, "url": format!("{}/payload-b", server.url()), "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "size": payload.len()}]
            }
        ]
    })
    .to_string();
    let vsman_sha = compute_hash(vsman.as_bytes());
    let channel = channel_body(&format!("{}/manifest", server.url()), &vsman_sha);
    let selector = serve_manifests(
        &mut server,
        &channel,
        &vsman,
        FetchCounts {
            channel_full: 1,
            vsman_full: 1,
            vsman_304: 0,
        },
    )
    .await;
    forbid_payload(&mut server).await;

    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let plan = create_install_plan(&plan_options(
        &selector,
        cache.path(),
        &root.path().join("target"),
    ))
    .await
    .unwrap();
    assert_eq!(plan.blocking_conflicts.len(), 1);
    assert!(plan.blocking_conflicts[0].contains("Conflicting payload identities"));
}

#[tokio::test]
async fn plan_blocks_payloads_without_official_sha256() {
    let mut server = mockito::Server::new_async().await;
    let payload = payload_bytes();
    let vsman = serde_json::json!({
        "manifestVersion": "1.0",
        "packages": [
            {
                "id": MSVC_PACKAGE_ID,
                "version": "14.44.34823",
                "type": "VSIX",
                "payloads": [{"fileName": PAYLOAD_NAME, "url": format!("{}/payload", server.url()), "sha256": null, "size": payload.len()}]
            }
        ]
    })
    .to_string();
    let vsman_sha = compute_hash(vsman.as_bytes());
    let channel = channel_body(&format!("{}/manifest", server.url()), &vsman_sha);
    let selector = serve_manifests(
        &mut server,
        &channel,
        &vsman,
        FetchCounts {
            channel_full: 1,
            vsman_full: 1,
            vsman_304: 0,
        },
    )
    .await;
    forbid_payload(&mut server).await;

    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let plan = create_install_plan(&plan_options(
        &selector,
        cache.path(),
        &root.path().join("target"),
    ))
    .await
    .unwrap();
    assert_eq!(plan.blocking_conflicts.len(), 1);
    assert!(plan.blocking_conflicts[0].contains("no official SHA256"));
}

#[tokio::test]
async fn apply_revalidation_detects_changed_manifest() {
    let mut server = mockito::Server::new_async().await;
    let payload = payload_bytes();
    let payload_sha = compute_hash(&payload);

    // Phase 1: manifest A.
    let vsman_a = msvc_manifest_body(
        &format!("{}/payload", server.url()),
        &payload_sha,
        payload.len() as u64,
    );
    let vsman_a_sha = compute_hash(vsman_a.as_bytes());
    let channel_a = channel_body(&format!("{}/manifest", server.url()), &vsman_a_sha);
    let selector = serve_manifests(
        &mut server,
        &channel_a,
        &vsman_a,
        FetchCounts {
            channel_full: 1,
            vsman_full: 1,
            vsman_304: 0,
        },
    )
    .await;

    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");
    let plan = create_install_plan(&plan_options(&selector, cache.path(), &target))
        .await
        .unwrap();

    // Phase 2: the channel now serves a different manifest (different digest).
    // The revalidation fetch carries the phase-1 validators, so these mocks
    // intentionally match any header set and answer 200 with the new bodies.
    let mut payload_b = payload.clone();
    payload_b.extend_from_slice(b"-version-b");
    let payload_b_sha = compute_hash(&payload_b);
    let vsman_b = msvc_manifest_body(
        &format!("{}/payload", server.url()),
        &payload_b_sha,
        payload_b.len() as u64,
    );
    let vsman_b_sha = compute_hash(vsman_b.as_bytes());
    let channel_b = channel_body(&format!("{}/manifest", server.url()), &vsman_b_sha);
    let _channel_b = server
        .mock("GET", "/channel")
        .with_header("ETag", "channel-2")
        .with_body(channel_b)
        .expect(1)
        .create_async()
        .await;
    let _vsman_b = server
        .mock("GET", "/manifest")
        .with_header("ETag", "vsman-2")
        .with_body(vsman_b)
        .expect(1)
        .create_async()
        .await;

    let revalidation = revalidate_plan(&plan, cache.path()).await.unwrap();
    assert!(revalidation.is_stale());
    assert!(
        revalidation.stale_reasons[0].contains("manifest changed"),
        "expected manifest-change staleness: {:?}",
        revalidation.stale_reasons
    );
}

#[tokio::test]
async fn downloaded_payloads_match_the_plan_pins() {
    let mut server = mockito::Server::new_async().await;
    let payload = payload_bytes();
    let payload_sha = compute_hash(&payload);
    let vsman = msvc_manifest_body(
        &format!("{}/payload", server.url()),
        &payload_sha,
        payload.len() as u64,
    );
    let vsman_sha = compute_hash(vsman.as_bytes());
    let channel = channel_body(&format!("{}/manifest", server.url()), &vsman_sha);
    // Plan once, then the downloader fetches (channel full again, vsman 304).
    let selector = serve_manifests(
        &mut server,
        &channel,
        &vsman,
        FetchCounts {
            channel_full: 2,
            vsman_full: 1,
            vsman_304: 1,
        },
    )
    .await;

    // The payload itself is served on a second phase (after planning).
    let _payload_mock = server
        .mock("GET", "/payload")
        .with_body(payload.clone())
        .expect(1)
        .create_async()
        .await;

    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");
    let options = plan_options(&selector, cache.path(), &target);
    let plan = create_install_plan(&options).await.unwrap();

    // Execute the download exactly like apply would (pinned options).
    let download_options = msvc_kit::DownloadOptions {
        msvc_version: plan.request.msvc_version.clone(),
        sdk_version: None,
        target_dir: target.clone(),
        arch: msvc_kit::Architecture::X64,
        host_arch: Some(msvc_kit::Architecture::X64),
        verify_hashes: true,
        parallel_downloads: 2,
        http_client: None,
        progress_handler: None,
        cache_manager: None,
        vs_channel: Some(selector.to_string()),
        manifest_cache_dir: Some(cache.path().to_path_buf()),
        dry_run: false,
        include_components: Default::default(),
        exclude_patterns: vec![],
    };
    let info = msvc_kit::download_msvc(&download_options).await.unwrap();

    // The downloader wrote exactly where the plan predicted.
    let planned_path = planned_payload_path(&target);
    assert_eq!(info.downloaded_files, vec![planned_path.clone()]);
    assert!(planned_path.is_file());

    // The plan pins match the downloaded bytes.
    verify_downloaded_against_plan("msvc", &info.downloaded_files, &plan)
        .await
        .unwrap();

    // A modified download must fail the pin check.
    std::fs::write(&planned_path, b"tampered").unwrap();
    let result = verify_downloaded_against_plan("msvc", &info.downloaded_files, &plan).await;
    assert!(result.is_err());
}
