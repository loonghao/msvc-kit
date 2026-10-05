use msvc_kit::downloader::{DownloadIndex, PackagePayload};
use msvc_kit::env::toolchain_fingerprint;
use msvc_kit::toolchain_lock::{InstallationReceipt, LockedPayload, ToolchainLock};
use msvc_kit::{Architecture, InstallInfo};
use sha2::{Digest, Sha256};

fn selection() -> ToolchainLock {
    ToolchainLock {
        schema: ToolchainLock::SCHEMA.into(),
        msvc_version: "14.44.35207".into(),
        sdk_version: "10.0.26100.0".into(),
        arch: Architecture::X64,
        host_arch: Architecture::X64,
        fingerprint: toolchain_fingerprint(
            Some("14.44.35207"),
            Some("10.0.26100.0"),
            Architecture::X64,
            Architecture::X64,
        ),
        receipts: vec![],
    }
}

#[test]
fn lock_roundtrip_is_portable_and_unknown_schema_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lock.json");
    let mut lock = selection();
    lock.save(&path).unwrap();
    let restored = ToolchainLock::load(&path).unwrap();
    assert_eq!(
        restored
            .request("another-root".into())
            .msvc_version
            .as_deref(),
        Some("14.44.35207")
    );
    assert!(!std::fs::read_to_string(&path)
        .unwrap()
        .contains(&temp.path().display().to_string()));
    lock.schema = "future-schema".into();
    assert!(lock.save(&path).is_err());
    assert!(
        ToolchainLock::load(&path).is_ok(),
        "failed save must preserve the previous lock"
    );
}

#[test]
fn lock_rejects_prefix_versions_and_tampered_identity() {
    let temp = tempfile::tempdir().unwrap();
    let mut lock = selection();
    lock.msvc_version = "14.44".into();
    assert!(lock.save(&temp.path().join("lock.json")).is_err());
    let mut lock = selection();
    lock.arch = Architecture::Arm64;
    assert!(lock.save(&temp.path().join("lock.json")).is_err());
    let mut lock = selection();
    lock.sdk_version = "10.0.26100".into();
    assert!(lock.save(&temp.path().join("lock.json")).is_err());
}

#[tokio::test]
async fn locked_sources_reject_modified_bytes_and_source_urls_before_extraction() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("payload.vsix");
    let hash = hex::encode(Sha256::digest(b"good"));
    std::fs::write(&file, b"good").unwrap();
    let payload = PackagePayload {
        file_name: "payload.vsix".into(),
        url: "https://example.test/package".into(),
        size: 4,
        sha256: Some(hash.clone()),
    };
    let mut index = DownloadIndex::load(&temp.path().join("index.db"))
        .await
        .unwrap();
    index
        .mark_completed(&payload, file.clone(), Some(hash.clone()))
        .await
        .unwrap();
    drop(index);
    let mut lock = selection();
    lock.receipts.push(InstallationReceipt {
        schema: "msvc-kit.installation-receipt.v1".into(),
        component: "msvc".into(),
        version: lock.msvc_version.clone(),
        arch: lock.arch,
        host_arch: lock.host_arch,
        vs_channel: Some("17".into()),
        components: vec![],
        payloads: vec![LockedPayload {
            file_name: payload.file_name.clone(),
            url: payload.url.clone(),
            size: 4,
            sha256: hash,
        }],
    });
    let info = InstallInfo {
        component_type: "msvc".into(),
        version: lock.msvc_version.clone(),
        arch: lock.arch,
        install_path: temp.path().into(),
        downloaded_files: vec![file.clone()],
    };
    lock.verify_download(&info).await.unwrap();
    std::fs::write(&file, b"evil").unwrap();
    assert!(lock.verify_download(&info).await.is_err());
    std::fs::write(&file, b"good").unwrap();
    lock.receipts[0].payloads[0].url = "https://example.test/different".into();
    assert!(lock.verify_download(&info).await.is_err());
    lock.receipts[0].payloads.clear();
    assert!(lock.verify_download(&info).await.is_err());
}
