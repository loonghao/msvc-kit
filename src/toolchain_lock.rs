//! Portable selection locks and verified source payload receipts.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::downloader::{hash::compute_file_hash, DownloadIndex};
use crate::storage::atomic_write;
use crate::toolchain::{resolve_toolchain, ToolchainRequest};
use crate::{Architecture, InstallInfo, MsvcKitError, Result};

async fn source_entry(file: &Path) -> Result<crate::downloader::IndexEntry> {
    for parent in file.ancestors().skip(1) {
        let database = parent.join("index.db");
        if database.is_file() {
            let lock_path = parent.join(".download.lock");
            let _cache_lock =
                tokio::task::spawn_blocking(move || crate::storage::lock_file(&lock_path))
                    .await
                    .map_err(|error| MsvcKitError::Other(error.to_string()))??;
            let index = DownloadIndex::load(&database).await?;
            let relative = file
                .strip_prefix(parent)
                .map_err(|error| MsvcKitError::Config(error.to_string()))?
                .to_string_lossy();
            for name in [
                relative.to_string(),
                relative.replace('\\', "/"),
                relative.replace('/', "\\"),
            ] {
                if let Some(entry) = index.get_entry(&name).await? {
                    return Ok(entry);
                }
            }
            break;
        }
    }
    Err(MsvcKitError::Config(format!(
        "No download provenance for {}",
        file.display()
    )))
}

/// Hold while downloading, extracting and recording one installation transaction.
pub fn lock_installation(root: &Path) -> Result<std::fs::File> {
    crate::storage::lock_file(&root.join(".msvc-kit").join("installation.lock"))
}

/// A source archive verified during acquisition. Paths on the acquiring machine are omitted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LockedPayload {
    pub file_name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// Download provenance for one installed component and host/target pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationReceipt {
    pub schema: String,
    pub component: String,
    pub version: String,
    pub arch: Architecture,
    pub host_arch: Architecture,
    pub vs_channel: Option<String>,
    pub components: Vec<String>,
    pub payloads: Vec<LockedPayload>,
}

/// Exact installed versions plus optional source provenance from msvc-kit downloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainLock {
    pub schema: String,
    pub msvc_version: String,
    pub sdk_version: String,
    pub arch: Architecture,
    pub host_arch: Architecture,
    pub fingerprint: String,
    pub receipts: Vec<InstallationReceipt>,
}

fn receipt_path(
    root: &Path,
    component: &str,
    version: &str,
    host: Architecture,
    arch: Architecture,
) -> PathBuf {
    root.join(".msvc-kit")
        .join("receipts")
        .join(format!("{component}-{version}-{host}-{arch}.json"))
}

impl ToolchainLock {
    pub const SCHEMA: &'static str = "msvc-kit.toolchain-lock.v1";

    /// Capture installed versions without a network request. Existing external installs may have no receipts.
    pub fn capture(request: &ToolchainRequest) -> Result<Self> {
        let environment = resolve_toolchain(request)?;
        let mut receipts = Vec::new();
        for (component, version) in [
            ("msvc", &environment.vc_tools_version),
            ("sdk", &environment.windows_sdk_version),
        ] {
            let path = receipt_path(
                &request.install_dir,
                component,
                version,
                request.host_arch,
                request.arch,
            );
            if path.exists() {
                let receipt: InstallationReceipt = serde_json::from_slice(&std::fs::read(path)?)?;
                if receipt.schema != "msvc-kit.installation-receipt.v1"
                    || receipt.component != component
                    || receipt.version != *version
                    || receipt.arch != request.arch
                    || receipt.host_arch != request.host_arch
                {
                    return Err(MsvcKitError::Config(
                        "Installation receipt does not match the selected toolchain".into(),
                    ));
                }
                receipts.push(receipt);
            }
        }
        let lock = Self {
            schema: Self::SCHEMA.into(),
            msvc_version: environment.vc_tools_version.clone(),
            sdk_version: environment.windows_sdk_version.clone(),
            arch: request.arch,
            host_arch: request.host_arch,
            fingerprint: environment.fingerprint(),
            receipts,
        };
        lock.validate()?;
        Ok(lock)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let lock: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        lock.validate()?;
        Ok(lock)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }

    pub fn request(&self, install_dir: PathBuf) -> ToolchainRequest {
        ToolchainRequest {
            install_dir,
            msvc_version: Some(self.msvc_version.clone()),
            sdk_version: Some(self.sdk_version.clone()),
            arch: self.arch,
            host_arch: self.host_arch,
        }
    }

    /// Check acquired archives before extraction when this lock contains their source provenance.
    pub async fn verify_download(&self, info: &InstallInfo) -> Result<()> {
        let Some(receipt) = self
            .receipts
            .iter()
            .find(|receipt| receipt.component == info.component_type)
        else {
            return Ok(());
        };
        if info.downloaded_files.len() != receipt.payloads.len() {
            return Err(MsvcKitError::Config(format!(
                "Locked {} payload set changed",
                info.component_type
            )));
        }
        for file in &info.downloaded_files {
            let entry = source_entry(file).await?;
            let name = entry.file_name.as_str();
            let payload = receipt
                .payloads
                .iter()
                .find(|payload| payload.file_name == name)
                .ok_or_else(|| {
                    MsvcKitError::Config(format!("Payload {name} is not in the lock"))
                })?;
            let actual = compute_file_hash(file).await?;
            if entry.url != payload.url {
                return Err(MsvcKitError::Config(format!(
                    "Locked source URL changed for {name}"
                )));
            }
            if std::fs::metadata(file)?.len() != payload.size
                || !actual.eq_ignore_ascii_case(&payload.sha256)
            {
                return Err(MsvcKitError::HashMismatch {
                    file: name.into(),
                    expected: payload.sha256.clone(),
                    actual,
                });
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if self.schema != Self::SCHEMA {
            return Err(MsvcKitError::Config(format!(
                "Unsupported toolchain lock schema: {}",
                self.schema
            )));
        }
        for (version, count) in [(&self.msvc_version, 3), (&self.sdk_version, 4)] {
            if version.split('.').count() != count
                || version
                    .split('.')
                    .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
            {
                return Err(MsvcKitError::Config(format!(
                    "Lock requires an exact numeric version: {version}"
                )));
            }
        }
        if self.fingerprint
            != crate::env::toolchain_fingerprint(
                Some(&self.msvc_version),
                Some(&self.sdk_version),
                self.host_arch,
                self.arch,
            )
        {
            return Err(MsvcKitError::Config(
                "Lock fingerprint does not match its selection".into(),
            ));
        }
        let mut components = std::collections::HashSet::new();
        for receipt in &self.receipts {
            if !components.insert(&receipt.component) {
                return Err(MsvcKitError::Config("Duplicate locked component".into()));
            }
            let expected = match receipt.component.as_str() {
                "msvc" => &self.msvc_version,
                "sdk" => &self.sdk_version,
                other => {
                    return Err(MsvcKitError::Config(format!(
                        "Unknown locked component: {other}"
                    )))
                }
            };
            if receipt.schema != "msvc-kit.installation-receipt.v1"
                || &receipt.version != expected
                || receipt.arch != self.arch
                || receipt.host_arch != self.host_arch
            {
                return Err(MsvcKitError::Config(
                    "Lock receipt selection mismatch".into(),
                ));
            }
            let mut names = std::collections::HashSet::new();
            for payload in &receipt.payloads {
                if payload.file_name.is_empty() || !names.insert(&payload.file_name) {
                    return Err(MsvcKitError::Config(
                        "Empty or duplicate locked payload name".into(),
                    ));
                }
                if payload.sha256.len() != 64
                    || !payload.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(MsvcKitError::Config(format!(
                        "Invalid SHA256 for {}",
                        payload.file_name
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Persist provenance only after extraction has completed successfully.
pub async fn record_installation(
    info: &InstallInfo,
    root: &Path,
    host_arch: Architecture,
    vs_channel: Option<String>,
    components: Vec<String>,
) -> Result<()> {
    let mut payloads = Vec::new();
    for file in &info.downloaded_files {
        let entry = source_entry(file).await?;
        payloads.push(LockedPayload {
            file_name: entry.file_name,
            url: entry.url,
            size: std::fs::metadata(file)?.len(),
            sha256: compute_file_hash(file).await?,
        });
    }
    payloads.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    let receipt = InstallationReceipt {
        schema: "msvc-kit.installation-receipt.v1".into(),
        component: info.component_type.clone(),
        version: info.version.clone(),
        arch: info.arch,
        host_arch,
        vs_channel,
        components,
        payloads,
    };
    atomic_write(
        &receipt_path(
            root,
            &info.component_type,
            &info.version,
            host_arch,
            info.arch,
        ),
        &serde_json::to_vec_pretty(&receipt)?,
    )
}
