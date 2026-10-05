use msvc_kit::downloader::{DownloadIndex, PackagePayload};
use msvc_kit::env::toolchain_fingerprint;
use msvc_kit::toolchain_lock::{
    lock_installation, record_installation, InstallationReceipt, LockedPayload, ToolchainLock,
};
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

#[tokio::test]
async fn recorded_receipts_capture_exact_selection_and_missing_provenance_fails() {
    let temp = tempfile::tempdir().unwrap();
    let install = temp.path().join("install");
    std::fs::create_dir_all(install.join("VC/Tools/MSVC/14.44.35207")).unwrap();
    std::fs::create_dir_all(install.join("Windows Kits/10/Include/10.0.26100.0")).unwrap();
    let selection = selection();
    let request = selection.request(install.clone());
    assert!(ToolchainLock::capture(&request)
        .unwrap()
        .receipts
        .is_empty());
    let _transaction = lock_installation(&install).unwrap();
    let cache = temp.path().join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let source = cache.join("payload.vsix");
    std::fs::write(&source, b"good").unwrap();
    let hash = hex::encode(Sha256::digest(b"good"));
    let payload = PackagePayload {
        file_name: "payload.vsix".into(),
        url: "https://example.test/package".into(),
        size: 4,
        sha256: Some(hash.clone()),
    };
    let mut index = DownloadIndex::load(&cache.join("index.db")).await.unwrap();
    index
        .mark_completed(&payload, source.clone(), Some(hash))
        .await
        .unwrap();
    drop(index);
    let mut info = InstallInfo {
        component_type: "msvc".into(),
        version: selection.msvc_version.clone(),
        arch: selection.arch,
        install_path: install.clone(),
        downloaded_files: vec![source],
    };
    record_installation(
        &info,
        &install,
        selection.host_arch,
        Some("17".into()),
        vec![],
    )
    .await
    .unwrap();
    let lock = ToolchainLock::capture(&request).unwrap();
    assert_eq!(lock.receipts.len(), 1);
    assert_eq!(lock.receipts[0].payloads[0].size, 4);
    lock.verify_download(&info).await.unwrap();
    let lock_path = temp.path().join("lock.json");
    lock.save(&lock_path).unwrap();
    assert!(!std::fs::read_to_string(lock_path)
        .unwrap()
        .contains(&temp.path().display().to_string()));
    info.downloaded_files = vec![temp.path().join("unindexed.vsix")];
    std::fs::write(&info.downloaded_files[0], b"good").unwrap();
    assert!(
        record_installation(&info, &install, selection.host_arch, None, vec![])
            .await
            .is_err()
    );
    assert!(lock.verify_download(&info).await.is_err());
    // Receipts from a different selection cannot become a new captured lock.
    let receipt = install.join(".msvc-kit/receipts/msvc-14.44.35207-x64-x64.json");
    let mut bad = lock.receipts[0].clone();
    bad.host_arch = Architecture::Arm64;
    std::fs::write(receipt, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(ToolchainLock::capture(&request).is_err());
}

#[test]
fn lock_rejects_invalid_receipt_identity_and_payloads_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lock.json");
    let mut lock = selection();
    lock.receipts.push(InstallationReceipt {
        schema: "msvc-kit.installation-receipt.v1".into(),
        component: "sdk".into(),
        version: lock.sdk_version.clone(),
        arch: lock.arch,
        host_arch: lock.host_arch,
        vs_channel: Some("17".into()),
        components: vec![],
        payloads: vec![LockedPayload {
            file_name: "payload.vsix".into(),
            url: "https://example.test/package".into(),
            size: 4,
            sha256: hex::encode(Sha256::digest(b"good")),
        }],
    });
    lock.save(&path).unwrap();
    let previous = std::fs::read(&path).unwrap();
    let mut invalid = Vec::new();
    let mut changed = lock.clone();
    changed.receipts.push(changed.receipts[0].clone());
    invalid.push(changed);
    let mut changed = lock.clone();
    changed.receipts[0].component = "unknown".into();
    invalid.push(changed);
    let mut changed = lock.clone();
    changed.receipts[0].version = "10.0.1.0".into();
    invalid.push(changed);
    let mut changed = lock.clone();
    changed.receipts[0].payloads[0].file_name.clear();
    invalid.push(changed);
    let mut changed = lock.clone();
    let duplicate = changed.receipts[0].payloads[0].clone();
    changed.receipts[0].payloads.push(duplicate);
    invalid.push(changed);
    let mut changed = lock.clone();
    changed.receipts[0].payloads[0].sha256 = "z".repeat(64);
    invalid.push(changed);
    for changed in invalid {
        assert!(changed.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), previous);
    }
}
