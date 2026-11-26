//! Canonical install plan (`msvc-kit.install-plan.v1`) and its planner.
//!
//! The plan is the single source of truth for what an install will do: which
//! manifest snapshot it was resolved against, which payloads will be
//! downloaded or reused from cache, what will be extracted or retained, and
//! what it will cost. Dry run and apply share this document; apply never
//! re-resolves "latest" — it verifies the plan's observations still hold
//! (manifest digest, cached payload digests, free space) or fails with a
//! stale-plan report (spec 04 §6).
//!
//! `plan_id` is the SHA256 over the canonical serialization of the plan
//! without `created_at` (field order is fixed by the struct definitions), so
//! the same resolved state always produces the same identity (I-012) and
//! mutating any observed fact changes the identity.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::downloader::{
    hash::compute_file_hash, ManifestIdentity, MsvcComponent, Package, VsManifest,
};
use crate::error::{MsvcKitError, Result};
use crate::version::Architecture;
use crate::vs_channel::VsChannelSelection;

pub(crate) const INSTALL_PLAN_SCHEMA: &str = "msvc-kit.install-plan.v1";

/// The kinds of actions a plan can express (spec 03 §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanActionKind {
    DownloadPayload,
    ReuseCachedPayload,
    ReuseInstalledComponent,
    ExtractPackage,
    RetainSourceMedia,
    PrunePath,
    CreateDirectory,
    WriteMetadata,
    BackupOwnedFile,
    CommitFile,
    SkipOptionalPackage,
    Conflict,
}

impl PlanActionKind {
    /// Snake-case name used in plan JSON and CLI text output.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DownloadPayload => "download_payload",
            Self::ReuseCachedPayload => "reuse_cached_payload",
            Self::ReuseInstalledComponent => "reuse_installed_component",
            Self::ExtractPackage => "extract_package",
            Self::RetainSourceMedia => "retain_source_media",
            Self::PrunePath => "prune_path",
            Self::CreateDirectory => "create_directory",
            Self::WriteMetadata => "write_metadata",
            Self::BackupOwnedFile => "backup_owned_file",
            Self::CommitFile => "commit_file",
            Self::SkipOptionalPackage => "skip_optional_package",
            Self::Conflict => "conflict",
        }
    }
}

/// One planned step with its dependency references and estimates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanAction {
    /// Stable identifier, unique inside the plan.
    pub id: String,
    pub kind: PlanActionKind,
    /// IDs of actions that must complete before this one starts.
    pub depends_on: Vec<String>,
    /// Package that owns the action, when applicable.
    pub package_id: Option<String>,
    /// Path the action concerns, relative to the target root when it lives
    /// under the install root (payload cache paths).
    pub path: Option<String>,
    /// Expected SHA256 of the payload/file, when known.
    pub expected_sha256: Option<String>,
    /// Estimated bytes this action consumes or reclaims, when known.
    pub estimated_bytes: Option<u64>,
    pub reason: String,
    /// Risk note surfaced to plan reviewers.
    pub risk: Option<String>,
}

/// Normalized request recorded inside a plan. It carries the selectors an
/// apply run needs to reproduce the exact same package set against the same
/// manifest snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub profile: Option<String>,
    pub provider: String,
    pub host_arch: String,
    pub target_arches: Vec<String>,
    /// MSVC version selector as planned (two-component prefix or None=latest).
    pub msvc_version: Option<String>,
    /// Windows SDK version selector as planned (full version or None=latest).
    pub sdk_version: Option<String>,
    /// Visual Studio channel selector that served the manifest.
    pub vs_channel: Option<String>,
    pub include_components: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub include_msvc: bool,
    pub include_sdk: bool,
}

/// One payload the plan resolved (spec 04 §5 `resolved.payloads`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanPayload {
    /// `msvc` or `sdk`.
    pub component: String,
    pub file_name: String,
    pub url: String,
    pub sha256: Option<String>,
    pub size: u64,
    pub package_id: String,
    /// Lowercased extension: `vsix`, `zip`, `msi`, `cab`, `exe`, ...
    pub kind: String,
}

/// Resolved selection summary recorded inside a plan (spec 04 §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedSummary {
    /// Selected MSVC version prefix (e.g. `14.44`); `None` when unplanned.
    pub msvc_version: Option<String>,
    /// Selected full SDK version (e.g. `10.0.26100.0`); `None` when unplanned.
    pub sdk_version: Option<String>,
    /// Display name of the channel that served the manifest.
    pub channel: Option<String>,
    pub manifest_digest: Option<String>,
    pub packages: Vec<String>,
    pub payloads: Vec<PlanPayload>,
}

/// Facts an apply run revalidates before executing a plan (spec 03 §3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanObservations {
    /// Official SHA256 the channel declared for the package manifest.
    pub manifest_digest: Option<String>,
    /// Exact URL the manifest was fetched from.
    pub manifest_url: Option<String>,
    /// Normalized target root the plan was computed against.
    pub target_root: String,
    /// Free-space snapshot of the target volume at planning time; apply
    /// re-checks against the download estimate.
    pub free_space_bytes: Option<u64>,
}

/// Byte estimates with explicit unknowns (spec 04 §7). `None` means
/// "cannot estimate"; zero is only ever a measured fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskUsageEstimate {
    pub download_bytes: u64,
    pub reused_cache_bytes: u64,
    pub staging_bytes: Option<u64>,
    pub backup_bytes: Option<u64>,
    pub peak_extra_bytes: Option<u64>,
    pub final_delta_bytes: Option<i64>,
    /// `exact` / `bounded` / `unknown`.
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegeRequirement {
    pub requires_elevation: bool,
    pub reason: String,
}

/// The canonical install plan document (`msvc-kit.install-plan.v1`).
///
/// `plan_id` is derived from the canonical serialization of the plan without
/// `created_at`; it is verified, not recomputed, on load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallPlanDocument {
    pub schema: String,
    pub plan_id: String,
    /// Recorded wall-clock time; excluded from plan identity.
    pub created_at: chrono::DateTime<Utc>,
    pub request: PlanRequest,
    pub resolved: ResolvedSummary,
    pub observations: PlanObservations,
    pub actions: Vec<PlanAction>,
    pub disk_usage: DiskUsageEstimate,
    pub privilege: PrivilegeRequirement,
    pub warnings: Vec<String>,
    pub blocking_conflicts: Vec<String>,
}

/// Canonical serialization of everything except `created_at` and `plan_id`.
#[derive(Serialize)]
struct PlanIdentityRef<'a> {
    request: &'a PlanRequest,
    resolved: &'a ResolvedSummary,
    observations: &'a PlanObservations,
    actions: &'a [PlanAction],
    disk_usage: &'a DiskUsageEstimate,
    privilege: &'a PrivilegeRequirement,
    warnings: &'a [String],
    blocking_conflicts: &'a [String],
}

impl InstallPlanDocument {
    /// Recompute the plan identity from the plan's content.
    pub fn compute_plan_id(&self) -> String {
        let identity = PlanIdentityRef {
            request: &self.request,
            resolved: &self.resolved,
            observations: &self.observations,
            actions: &self.actions,
            disk_usage: &self.disk_usage,
            privilege: &self.privilege,
            warnings: &self.warnings,
            blocking_conflicts: &self.blocking_conflicts,
        };
        // serde_json serializes struct fields in declaration order, so this is
        // a stable canonical form for a fixed schema version.
        let bytes = serde_json::to_vec(&identity).expect("plan identity serialization cannot fail");
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    /// Whether the recorded `plan_id` still matches the plan content.
    pub fn verify_plan_id(&self) -> Result<()> {
        let computed = self.compute_plan_id();
        if self.plan_id != computed {
            return Err(MsvcKitError::Other(format!(
                "Install plan integrity check failed: recorded plan_id {} does not match the \
                 plan content ({}). The plan file was modified or truncated.",
                self.plan_id, computed
            )));
        }
        Ok(())
    }

    /// Human-readable one-line summary for CLI output.
    pub fn summarize(&self) -> String {
        let size = humansize::format_size(self.disk_usage.download_bytes, humansize::BINARY);
        let reused = humansize::format_size(self.disk_usage.reused_cache_bytes, humansize::BINARY);
        format!(
            "{} msvc {} sdk {} ({}): {} packages, {} payloads, {} to download, {} reused, \
             {} actions, {} blocking conflicts",
            self.request.provider,
            self.resolved.msvc_version.as_deref().unwrap_or("-"),
            self.resolved.sdk_version.as_deref().unwrap_or("-"),
            self.resolved.channel.as_deref().unwrap_or("-"),
            self.resolved.packages.len(),
            self.resolved.payloads.len(),
            size,
            reused,
            self.actions.len(),
            self.blocking_conflicts.len()
        )
    }
}

/// Inputs for [`create_install_plan`]. Mirrors the download options subset
/// that determines package selection; everything else is planner-internal.
#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub msvc_version: Option<String>,
    pub sdk_version: Option<String>,
    pub target_dir: PathBuf,
    pub arch: Architecture,
    pub host_arch: Option<Architecture>,
    pub vs_channel: Option<String>,
    pub include_components: Vec<MsvcComponent>,
    pub exclude_patterns: Vec<String>,
    pub manifest_cache_dir: PathBuf,
    pub include_msvc: bool,
    pub include_sdk: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            msvc_version: None,
            sdk_version: None,
            target_dir: PathBuf::from("msvc-kit"),
            arch: Architecture::host(),
            host_arch: None,
            vs_channel: None,
            include_components: Vec::new(),
            exclude_patterns: Vec::new(),
            manifest_cache_dir: crate::downloader::cache::default_manifest_cache_dir(),
            include_msvc: true,
            include_sdk: true,
        }
    }
}

/// Load a plan file and verify its schema and content identity.
pub fn load_and_verify_plan(path: &Path) -> Result<InstallPlanDocument> {
    let bytes = std::fs::read(path)?;
    let plan: InstallPlanDocument = serde_json::from_slice(&bytes)?;
    if plan.schema != INSTALL_PLAN_SCHEMA {
        return Err(MsvcKitError::Other(format!(
            "Unsupported install plan schema: {} (this build supports {INSTALL_PLAN_SCHEMA})",
            plan.schema
        )));
    }
    plan.verify_plan_id()?;
    Ok(plan)
}

/// What apply revalidation found. Empty `stale_reasons` means the plan still
/// describes the world accurately and execution may proceed.
#[derive(Debug, Default, Clone)]
pub struct PlanRevalidation {
    pub stale_reasons: Vec<String>,
    /// Manifest identity observed during revalidation.
    pub manifest: Option<ManifestIdentity>,
}

impl PlanRevalidation {
    pub fn is_stale(&self) -> bool {
        !self.stale_reasons.is_empty()
    }
}

/// Re-check a plan's observations against the current world without writing
/// anything or downloading payloads: re-fetch the manifest (cache-assisted)
/// and compare digests, re-hash cached payloads, and compare free space with
/// the plan's download estimate.
pub async fn revalidate_plan(
    plan: &InstallPlanDocument,
    manifest_cache_dir: &Path,
) -> Result<PlanRevalidation> {
    let mut revalidation = PlanRevalidation::default();

    let selection = VsChannelSelection::from_optional(plan.request.vs_channel.as_deref())?;
    let (_, _, identity) = VsManifest::fetch_with_identity(selection, manifest_cache_dir).await?;
    revalidation.manifest = Some(identity.clone());

    match &plan.observations.manifest_digest {
        Some(recorded) if recorded.eq_ignore_ascii_case(&identity.sha256) => {}
        Some(recorded) => revalidation.stale_reasons.push(format!(
            "The Visual Studio manifest changed since this plan was created (plan pinned {}, \
             the channel now serves {} at {}). Rerun `msvc-kit plan` to resolve against the \
             current manifest.",
            recorded, identity.sha256, identity.url
        )),
        None => revalidation.stale_reasons.push(
            "The plan was created without a manifest digest and cannot be revalidated".to_string(),
        ),
    }

    // Every cache-reuse action must still point at bytes hashing to its
    // planned digest; anything else means the world moved under the plan.
    let target_root = PathBuf::from(&plan.observations.target_root);
    for action in &plan.actions {
        if action.kind != PlanActionKind::ReuseCachedPayload {
            continue;
        }
        let Some(expected) = &action.expected_sha256 else {
            continue;
        };
        let relative = action.path.as_deref().unwrap_or_default();
        let cache_path = target_root.join(relative);
        let observed = match compute_file_hash(&cache_path).await {
            Ok(hash) => hash,
            Err(_) => {
                revalidation.stale_reasons.push(format!(
                    "Cached payload planned for reuse is no longer readable at {}",
                    cache_path.display()
                ));
                continue;
            }
        };
        if !observed.eq_ignore_ascii_case(expected) {
            revalidation.stale_reasons.push(format!(
                "Cached payload {} no longer matches its planned digest (expected {}, found {})",
                relative, expected, observed
            ));
        }
    }

    if let Some(free) = plan.observations.free_space_bytes {
        if let Some(current) = available_space(&target_root) {
            if current < plan.disk_usage.download_bytes {
                revalidation.stale_reasons.push(format!(
                    "Free space on the target volume ({current} bytes) dropped below the plan's \
                     download estimate ({} bytes)",
                    plan.disk_usage.download_bytes
                ));
            }
        } else if free < plan.disk_usage.download_bytes {
            revalidation.stale_reasons.push(format!(
                "The plan was created with only {free} bytes free, below its own download \
                 estimate ({} bytes)",
                plan.disk_usage.download_bytes
            ));
        }
    }

    Ok(revalidation)
}

/// Build the canonical install plan for the given options.
///
/// Planning never downloads payloads and never writes to the target root; it
/// reads the manifest (cache-assisted) and probes the payload cache.
pub async fn create_install_plan(options: &PlanOptions) -> Result<InstallPlanDocument> {
    create_install_plan_inner(options, None).await
}

/// Like [`create_install_plan`], but the MSVC package set comes from the
/// dependency-resolver graph seeded by the build profile (spec 04 section 4;
/// roadmap P08: the plan is produced by the resolved graph). SDK selection is
/// unchanged; `--exclude-pattern` still applies at seed level, but required
/// dependencies pulled in by the closure are always planned.
pub async fn create_install_plan_with_profile(
    options: &PlanOptions,
    profile: &crate::installer::resolver::BuildProfile,
) -> Result<InstallPlanDocument> {
    create_install_plan_inner(options, Some(profile)).await
}

async fn create_install_plan_inner(
    options: &PlanOptions,
    profile: Option<&crate::installer::resolver::BuildProfile>,
) -> Result<InstallPlanDocument> {
    let selection = VsChannelSelection::from_optional(options.vs_channel.as_deref())?;
    let (manifest, channel_spec, identity) =
        VsManifest::fetch_with_identity(selection, &options.manifest_cache_dir).await?;

    let host_arch = options.host_arch.unwrap_or_else(Architecture::host);
    let host = host_arch.to_string();
    let target = options.arch.to_string();

    let mut actions: Vec<PlanAction> = Vec::new();
    let mut payloads: Vec<PlanPayload> = Vec::new();
    let mut packages: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut blocking_conflicts: Vec<String> = Vec::new();
    let mut download_bytes = 0u64;
    let mut reused_bytes = 0u64;
    let mut action_seq = 0usize;
    // file_name → action id of the first planned payload with that name
    // (shared payloads across packages reference one action).
    let mut payload_action_ids: HashMap<String, String> = HashMap::new();

    let msvc_selected = if options.include_msvc {
        options
            .msvc_version
            .clone()
            .or_else(|| manifest.get_latest_msvc_version())
    } else {
        None
    };
    let sdk_selected = if options.include_sdk {
        match &options.sdk_version {
            Some(requested) => manifest.resolve_sdk_version(requested),
            None => manifest.get_latest_sdk_version(),
        }
    } else {
        None
    };

    let include_set: std::collections::HashSet<MsvcComponent> =
        options.include_components.iter().cloned().collect();

    let mut profile_used: Option<String> = None;
    if options.include_msvc {
        let version = msvc_selected.clone().ok_or_else(|| {
            MsvcKitError::VersionNotFound(
                "No MSVC version found in the manifest to plan".to_string(),
            )
        })?;
        // Profile-driven resolution: seeds from find_msvc_packages (the
        // battle-tested matcher, including the profile's component opt-ins),
        // then the resolver closes over manifest dependencies in a stable
        // topological order (roadmap P08: the plan is produced by the
        // resolved graph).
        let mut include_with_profile = include_set.clone();
        if let Some(profile) = profile {
            for component in profile.include_components() {
                include_with_profile.insert(component);
            }
        }
        let seed_packages = manifest.find_msvc_packages(
            &version,
            &host,
            &target,
            &include_with_profile,
            &options.exclude_patterns,
        );
        let packages_for_component = match profile {
            None => seed_packages,
            Some(profile) => {
                let seed_ids: Vec<String> = seed_packages
                    .iter()
                    .map(|package| package.id.clone())
                    .collect();
                let resolved = crate::installer::resolver::resolve(&manifest, &seed_ids)
                    .map_err(|error| MsvcKitError::Config(error.to_string()))?;
                profile_used = Some(profile.as_str().to_string());
                let by_id: std::collections::HashMap<&str, &crate::downloader::VsPackage> =
                    manifest
                        .packages
                        .iter()
                        .map(|package| (package.id.as_str(), package))
                        .collect();
                resolved
                    .packages
                    .iter()
                    .filter_map(|resolved| {
                        by_id
                            .get(resolved.id.as_str())
                            .map(|package| crate::downloader::Package {
                                id: package.id.clone(),
                                version: package.version.clone(),
                                package_type: package.package_type.clone(),
                                chip: package.chip.clone(),
                                payloads: package
                                    .payloads
                                    .iter()
                                    .map(|payload| crate::downloader::PackagePayload {
                                        file_name: payload.file_name.clone(),
                                        url: payload.url.clone(),
                                        size: payload.size.unwrap_or(0),
                                        sha256: payload.sha256.clone(),
                                    })
                                    .collect(),
                                total_size: package
                                    .payloads
                                    .iter()
                                    .map(|payload| payload.size.unwrap_or(0))
                                    .sum(),
                            })
                    })
                    .collect()
            }
        };
        plan_component(ComponentPlan {
            component: "msvc",
            label: "MSVC",
            slug: "msvc",
            selected_version: Some(version),
            packages: packages_for_component,
            options,
            host: &host,
            target: &target,
            actions: &mut actions,
            payloads: &mut payloads,
            packages_out: &mut packages,
            warnings: &mut warnings,
            blocking_conflicts: &mut blocking_conflicts,
            download_bytes: &mut download_bytes,
            reused_bytes: &mut reused_bytes,
            action_seq: &mut action_seq,
            payload_action_ids: &mut payload_action_ids,
        })
        .await?;
    }

    if options.include_sdk {
        let version = sdk_selected.clone().ok_or_else(|| {
            MsvcKitError::VersionNotFound(
                "No Windows SDK version found in the manifest to plan".to_string(),
            )
        })?;
        let mut packages_for_component = manifest.find_sdk_packages(&version, &target);
        if target != host.to_lowercase() {
            for package in manifest.find_sdk_packages(&version, &host) {
                if !packages_for_component
                    .iter()
                    .any(|existing| existing.id == package.id)
                {
                    packages_for_component.push(package);
                }
            }
        }
        plan_component(ComponentPlan {
            component: "sdk",
            label: "Windows SDK",
            slug: "sdk",
            selected_version: Some(version),
            packages: packages_for_component,
            options,
            host: &host,
            target: &target,
            actions: &mut actions,
            payloads: &mut payloads,
            packages_out: &mut packages,
            warnings: &mut warnings,
            blocking_conflicts: &mut blocking_conflicts,
            download_bytes: &mut download_bytes,
            reused_bytes: &mut reused_bytes,
            action_seq: &mut action_seq,
            payload_action_ids: &mut payload_action_ids,
        })
        .await?;
    }

    if actions
        .iter()
        .any(|action| action.kind == PlanActionKind::SkipOptionalPackage)
    {
        warnings.push(
            "Some payloads are recorded but not extracted (bootstrap executables, unsupported \
             types or optional SDK MSIs)"
                .to_string(),
        );
    }

    let manifest_digest = Some(identity.sha256.clone());
    let manifest_url = Some(identity.url.clone());
    let target_root = options.target_dir.to_string_lossy().into_owned();
    let free_space_bytes = available_space(&options.target_dir);

    let mut plan = InstallPlanDocument {
        schema: INSTALL_PLAN_SCHEMA.to_string(),
        plan_id: String::new(),
        created_at: Utc::now(),
        request: PlanRequest {
            profile: profile_used.clone(),
            provider: "msvc".to_string(),
            host_arch: host,
            target_arches: vec![target],
            msvc_version: options.msvc_version.clone(),
            sdk_version: options.sdk_version.clone(),
            vs_channel: options.vs_channel.clone(),
            include_components: options
                .include_components
                .iter()
                .map(|component| component.to_string())
                .collect(),
            exclude_patterns: options.exclude_patterns.clone(),
            include_msvc: options.include_msvc,
            include_sdk: options.include_sdk,
        },
        resolved: ResolvedSummary {
            msvc_version: msvc_selected,
            sdk_version: sdk_selected,
            channel: Some(channel_spec.display_name()),
            manifest_digest: manifest_digest.clone(),
            packages,
            payloads,
        },
        observations: PlanObservations {
            manifest_digest,
            manifest_url,
            target_root,
            free_space_bytes,
        },
        actions,
        disk_usage: DiskUsageEstimate {
            download_bytes,
            reused_cache_bytes: reused_bytes,
            staging_bytes: None,
            backup_bytes: None,
            peak_extra_bytes: Some(download_bytes),
            final_delta_bytes: None,
            confidence: if download_bytes == 0 && reused_bytes == 0 {
                "unknown".to_string()
            } else {
                "bounded".to_string()
            },
        },
        privilege: PrivilegeRequirement {
            requires_elevation: false,
            reason: "user-writable target".to_string(),
        },
        warnings,
        blocking_conflicts,
    };
    plan.plan_id = plan.compute_plan_id();
    Ok(plan)
}

/// Cross-check downloaded files against the plan's payload set before
/// extraction: same count, same names, same digests. A mismatch means the
/// world moved under the plan; execution must stop.
pub async fn verify_downloaded_against_plan(
    component: &str,
    downloaded_files: &[PathBuf],
    plan: &InstallPlanDocument,
) -> Result<()> {
    let expected: Vec<&PlanPayload> = plan
        .resolved
        .payloads
        .iter()
        .filter(|payload| payload.component == component)
        .collect();
    if downloaded_files.len() != expected.len() {
        return Err(MsvcKitError::Other(format!(
            "Install plan mismatch for {component}: downloaded {} files but the plan pins {}",
            downloaded_files.len(),
            expected.len()
        )));
    }
    for file in downloaded_files {
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let payload = expected
            .iter()
            .find(|payload| payload.file_name == name)
            .ok_or_else(|| {
                MsvcKitError::Other(format!(
                    "Install plan mismatch for {component}: downloaded file {name:?} is not \
                     pinned by the plan"
                ))
            })?;
        let expected_sha = payload.sha256.as_deref().ok_or_else(|| {
            MsvcKitError::Other(format!(
                "Install plan payload {name} has no digest to verify against"
            ))
        })?;
        let actual = compute_file_hash(file).await?;
        if !actual.eq_ignore_ascii_case(expected_sha) {
            return Err(MsvcKitError::HashMismatch {
                file: name.to_string(),
                expected: expected_sha.to_string(),
                actual,
            });
        }
    }
    Ok(())
}

/// Per-component planning output bundle (parameters are shared accumulators).
#[allow(clippy::too_many_arguments)]
struct ComponentPlan<'a> {
    component: &'static str,
    label: &'static str,
    slug: &'static str,
    selected_version: Option<String>,
    packages: Vec<Package>,
    options: &'a PlanOptions,
    host: &'a str,
    target: &'a str,
    actions: &'a mut Vec<PlanAction>,
    payloads: &'a mut Vec<PlanPayload>,
    packages_out: &'a mut Vec<String>,
    warnings: &'a mut Vec<String>,
    blocking_conflicts: &'a mut Vec<String>,
    download_bytes: &'a mut u64,
    reused_bytes: &'a mut u64,
    action_seq: &'a mut usize,
    payload_action_ids: &'a mut HashMap<String, String>,
}

async fn plan_component(context: ComponentPlan<'_>) -> Result<()> {
    let Some(version) = context.selected_version.clone() else {
        context.blocking_conflicts.push(format!(
            "No {} version found to plan; rerun after selecting an available version",
            context.label
        ));
        return Ok(());
    };
    if context.packages.is_empty() {
        context.blocking_conflicts.push(format!(
            "No {label} packages found for version {version} (host: {host}, target: {target})",
            label = context.label,
            host = context.host,
            target = context.target
        ));
        return Ok(());
    }

    let download_dir = planned_download_dir(
        &context.options.target_dir,
        context.slug,
        &version,
        context.host,
        context.target,
    );

    for package in &context.packages {
        context.packages_out.push(package.id.clone());
        let mut extract_deps: Vec<String> = Vec::new();

        for payload in &package.payloads {
            let kind = payload_kind(&payload.file_name);
            let action_kind = match kind.as_str() {
                "cab" => PlanActionKind::RetainSourceMedia,
                "vsix" | "zip" | "msi" => PlanActionKind::ExtractPackage,
                // SDK bootstrap executables and anything the extractor does
                // not unpack today are recorded, never extracted.
                _ => PlanActionKind::SkipOptionalPackage,
            };

            // Payload identity conflicts block the plan exactly like the
            // downloader refuses conflicting identities.
            if let Some(existing) = context
                .payloads
                .iter()
                .find(|planned| planned.file_name == payload.file_name)
            {
                let differs = existing.url != payload.url
                    || existing.sha256.as_deref() != payload.sha256.as_deref()
                    || existing.size != payload.size;
                if differs {
                    context.blocking_conflicts.push(format!(
                        "Conflicting payload identities for {}: planned as both ({}, {}) and ({}, {})",
                        payload.file_name,
                        existing.url,
                        existing.sha256.as_deref().unwrap_or("no sha256"),
                        payload.url,
                        payload.sha256.as_deref().unwrap_or("no sha256")
                    ));
                    continue;
                }
                // Identical duplicate: reference the existing action.
                if let Some(id) = context.payload_action_ids.get(&payload.file_name) {
                    if matches!(
                        action_kind,
                        PlanActionKind::ExtractPackage | PlanActionKind::RetainSourceMedia
                    ) {
                        extract_deps.push(id.clone());
                    }
                }
                continue;
            }

            // A payload without an official SHA256 cannot be applied with
            // hash verification on, so it blocks the plan.
            let expected_sha = match &payload.sha256 {
                Some(sha)
                    if sha.len() == 64 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
                {
                    sha.to_ascii_lowercase()
                }
                _ => {
                    context.blocking_conflicts.push(format!(
                        "Payload {} has no official SHA256 in the manifest; it cannot be planned \
                         as source-pinned",
                        payload.file_name
                    ));
                    continue;
                }
            };

            payloads_push(
                context.payloads,
                PlanPayload {
                    component: context.component.to_string(),
                    file_name: payload.file_name.clone(),
                    url: payload.url.clone(),
                    sha256: Some(expected_sha.clone()),
                    size: payload.size,
                    package_id: package.id.clone(),
                    kind: kind.clone(),
                },
            );

            *context.action_seq += 1;
            let action_id = format!("{}-payload-{}", context.slug, context.action_seq);
            let cache_path = download_dir.join(payload_path_relative(&payload.file_name)?);
            let cached = probe_cached_payload(&cache_path, &expected_sha, payload.size).await;

            let mut action = match cached {
                CacheProbe::Verified => {
                    *context.reused_bytes += payload.size;
                    make_action(
                        &action_id,
                        PlanActionKind::ReuseCachedPayload,
                        package.id.clone(),
                        &options_rel_path(&context.options.target_dir, &cache_path),
                        &expected_sha,
                        None,
                        "cache hit verified against official SHA256",
                    )
                }
                CacheProbe::Corrupt { actual } => {
                    context.warnings.push(format!(
                        "Cached payload {} does not match its official SHA256 (found {actual}); \
                         it will be downloaded again",
                        payload.file_name
                    ));
                    *context.download_bytes += payload.size;
                    make_action(
                        &action_id,
                        PlanActionKind::DownloadPayload,
                        package.id.clone(),
                        &options_rel_path(&context.options.target_dir, &cache_path),
                        &expected_sha,
                        Some(payload.size),
                        "cache hit failed verification",
                    )
                }
                CacheProbe::Miss => {
                    *context.download_bytes += payload.size;
                    make_action(
                        &action_id,
                        PlanActionKind::DownloadPayload,
                        package.id.clone(),
                        &options_rel_path(&context.options.target_dir, &cache_path),
                        &expected_sha,
                        Some(payload.size),
                        "not in cache",
                    )
                }
            };
            action.id = action_id.clone();
            context
                .payload_action_ids
                .insert(payload.file_name.clone(), action_id.clone());

            match action.kind {
                PlanActionKind::DownloadPayload
                | PlanActionKind::ReuseCachedPayload
                | PlanActionKind::RetainSourceMedia => extract_deps.push(action.id.clone()),
                PlanActionKind::SkipOptionalPackage => {
                    action.reason = optional_skip_reason(&payload.file_name, &kind);
                }
                _ => {}
            }
            context.actions.push(action);
        }

        let has_extractable = context.actions.iter().any(|action| {
            extract_deps.contains(&action.id)
                && matches!(
                    action.kind,
                    PlanActionKind::DownloadPayload
                        | PlanActionKind::ReuseCachedPayload
                        | PlanActionKind::RetainSourceMedia
                )
        });
        if has_extractable {
            *context.action_seq += 1;
            context.actions.push(PlanAction {
                id: format!("{}-extract-{}", context.slug, context.action_seq),
                kind: PlanActionKind::ExtractPackage,
                depends_on: extract_deps,
                package_id: Some(package.id.clone()),
                path: None,
                expected_sha256: None,
                estimated_bytes: None,
                reason: "extract the verified package payloads".to_string(),
                risk: None,
            });
        }
    }

    Ok(())
}

fn make_action(
    id: &str,
    kind: PlanActionKind,
    package_id: String,
    path: &str,
    expected_sha256: &str,
    estimated_bytes: Option<u64>,
    reason: &str,
) -> PlanAction {
    PlanAction {
        id: id.to_string(),
        kind,
        depends_on: vec![],
        package_id: Some(package_id),
        path: Some(path.to_string()),
        expected_sha256: Some(expected_sha256.to_string()),
        estimated_bytes,
        reason: reason.to_string(),
        risk: None,
    }
}

fn payloads_push(payloads: &mut Vec<PlanPayload>, payload: PlanPayload) {
    payloads.push(payload);
}

fn optional_skip_reason(file_name: &str, kind: &str) -> String {
    let lower = file_name.to_lowercase();
    if kind == "exe" {
        "bootstrap executable is downloaded but not extracted".to_string()
    } else if super::SDK_OPTIONAL_MSI_SKIP_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        "optional Windows SDK MSI skipped (not required for MSVC toolchains)".to_string()
    } else {
        "payload type is not extracted by msvc-kit".to_string()
    }
}

/// The planner's cache probe result.
enum CacheProbe {
    Miss,
    Verified,
    Corrupt { actual: String },
}

async fn probe_cached_payload(path: &Path, expected_sha: &str, size: u64) -> CacheProbe {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => return CacheProbe::Miss,
    };
    // A size mismatch is cheap to detect and always wrong for a verified hit.
    if metadata.len() != size {
        return CacheProbe::Corrupt {
            actual: format!("size {} != planned {size}", metadata.len()),
        };
    }
    match compute_file_hash(path).await {
        Ok(actual) if actual.eq_ignore_ascii_case(expected_sha) => CacheProbe::Verified,
        Ok(actual) => CacheProbe::Corrupt { actual },
        Err(_) => CacheProbe::Miss,
    }
}

/// Normalize a manifest payload file name to its relative cache path
/// (same normalization the downloader applies).
fn payload_path_relative(name: &str) -> Result<PathBuf> {
    let normalized = name.replace('/', std::path::MAIN_SEPARATOR_STR);
    let relative = Path::new(&normalized);
    if normalized.is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(MsvcKitError::InstallPath(format!(
            "Invalid payload file name: {name}"
        )));
    }
    Ok(relative.to_path_buf())
}

fn options_rel_path(target_root: &Path, path: &Path) -> String {
    path.strip_prefix(target_root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The download directory the downloader will use for a component, derived
/// from the plan's resolved versions (same layout as `msvc.rs`/`sdk.rs`).
fn planned_download_dir(
    target_root: &Path,
    slug: &str,
    version: &str,
    host: &str,
    target: &str,
) -> PathBuf {
    if slug == "msvc" {
        target_root.join("downloads").join("msvc").join(format!(
            "{}_{}_{}",
            version.replace('.', "_"),
            host.to_lowercase(),
            target.to_lowercase()
        ))
    } else {
        let build_number = version.split('.').nth(2).unwrap_or(version);
        target_root.join("downloads").join("sdk").join(format!(
            "{}_{}",
            build_number,
            target.to_lowercase()
        ))
    }
}

fn payload_kind(file_name: &str) -> String {
    Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// Free bytes available to the current user on the volume holding `path`
/// (Windows only; other platforms report `None` and the plan leaves the
/// observation unset).
#[cfg(windows)]
fn available_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt as _;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free_to_caller = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    // SAFETY: all pointers are valid for the duration of the call and the
    // path buffer outlives it.
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total,
            &mut free,
        )
    };
    if ok != 0 {
        Some(free_to_caller)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn available_space(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod golden_tests {
    use super::*;

    /// Round-trip the plan document through its golden JSON so field names,
    /// casing and optionality stay stable across refactors.
    #[test]
    fn install_plan_golden_round_trip() {
        let json = r#"{
          "schema": "msvc-kit.install-plan.v1",
          "plan_id": "sha256:abcdef",
          "created_at": "2026-11-22T00:00:00Z",
          "request": {
            "profile": null,
            "provider": "msvc",
            "host_arch": "x64",
            "target_arches": ["x64"],
            "msvc_version": "14.44",
            "sdk_version": "10.0.26100.0",
            "vs_channel": "2026",
            "include_components": [],
            "exclude_patterns": [".uwp"],
            "include_msvc": true,
            "include_sdk": true
          },
          "resolved": {
            "msvc_version": "14.44",
            "sdk_version": "10.0.26100.0",
            "channel": "Visual Studio 2026 (v18)",
            "manifest_digest": "sha256:1234",
            "packages": ["Microsoft.VC.14.44.Tools.HostX64.TargetX64"],
            "payloads": [
              {
                "component": "msvc",
                "file_name": "Microsoft.VC.14.44.Tools.HostX64.TargetX64.vsix",
                "url": "https://download.visualstudio.microsoft.com/download/pr/x/x.vsix",
                "sha256": "aa",
                "size": 1048576,
                "package_id": "Microsoft.VC.14.44.Tools.HostX64.TargetX64",
                "kind": "vsix"
              }
            ]
          },
          "observations": {
            "manifest_digest": "sha256:1234",
            "manifest_url": "https://example.com/VisualStudio.vsman",
            "target_root": "C:/tools/msvc",
            "free_space_bytes": 107374182400
          },
          "actions": [
            {
              "id": "msvc-payload-1",
              "kind": "download_payload",
              "depends_on": [],
              "package_id": "Microsoft.VC.14.44.Tools.HostX64.TargetX64",
              "path": "downloads/msvc/14_44_x64/x.vsix",
              "expected_sha256": "aa",
              "estimated_bytes": 1048576,
              "reason": "not in cache",
              "risk": null
            },
            {
              "id": "msvc-extract-2",
              "kind": "extract_package",
              "depends_on": ["msvc-payload-1"],
              "package_id": "Microsoft.VC.14.44.Tools.HostX64.TargetX64",
              "path": null,
              "expected_sha256": null,
              "estimated_bytes": null,
              "reason": "extract the verified package payloads",
              "risk": null
            }
          ],
          "disk_usage": {
            "download_bytes": 1048576,
            "reused_cache_bytes": 0,
            "staging_bytes": null,
            "backup_bytes": null,
            "peak_extra_bytes": 1048576,
            "final_delta_bytes": null,
            "confidence": "bounded"
          },
          "privilege": {"requires_elevation": false, "reason": "user-writable target"},
          "warnings": [],
          "blocking_conflicts": []
        }"#;

        let plan: InstallPlanDocument = serde_json::from_str(json).unwrap();
        assert_eq!(plan.schema, INSTALL_PLAN_SCHEMA);
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.actions[0].kind, PlanActionKind::DownloadPayload);
        assert_eq!(plan.actions[1].kind, PlanActionKind::ExtractPackage);
        assert_eq!(
            plan.actions[1].depends_on,
            vec!["msvc-payload-1".to_string()]
        );
        assert_eq!(plan.resolved.payloads[0].component, "msvc");
        let reencoded = serde_json::to_string(&plan).unwrap();
        let reparsed: InstallPlanDocument = serde_json::from_str(&reencoded).unwrap();
        assert_eq!(plan, reparsed);
        assert_eq!(
            serde_json::to_value(&plan).unwrap()["disk_usage"]["staging_bytes"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn install_plan_rejects_unknown_fields_and_bad_kinds() {
        let base = r#"{
          "schema": "msvc-kit.install-plan.v1",
          "plan_id": "sha256:abcdef",
          "created_at": "2026-11-22T00:00:00Z",
          "request": {"profile": null, "provider": "msvc", "host_arch": "x64", "target_arches": ["x64"], "msvc_version": null, "sdk_version": null, "vs_channel": null, "include_components": [], "exclude_patterns": [], "include_msvc": true, "include_sdk": true},
          "resolved": {"msvc_version": null, "sdk_version": null, "channel": null, "manifest_digest": null, "packages": [], "payloads": []},
          "observations": {"manifest_digest": null, "manifest_url": null, "target_root": "C:/t", "free_space_bytes": null},
          "actions": [],
          "disk_usage": {"download_bytes": 0, "reused_cache_bytes": 0, "staging_bytes": null, "backup_bytes": null, "peak_extra_bytes": null, "final_delta_bytes": null, "confidence": "unknown"},
          "privilege": {"requires_elevation": false, "reason": "r"},
          "warnings": [],
          "blocking_conflicts": []
        }"#;

        let with_unknown: serde_json::Value =
            serde_json::from_str::<serde_json::Value>(base).unwrap();
        let mut value = with_unknown.clone();
        value["unexpected_field"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<InstallPlanDocument>(value).is_err());

        let mut value = with_unknown.clone();
        value["actions"] = serde_json::json!([{
            "id": "x",
            "kind": "format_disk",
            "depends_on": [],
            "package_id": null,
            "path": null,
            "expected_sha256": null,
            "estimated_bytes": null,
            "reason": "no",
            "risk": null
        }]);
        assert!(serde_json::from_value::<InstallPlanDocument>(value).is_err());

        let mut value = with_unknown;
        value["schema"] = serde_json::Value::String("msvc-kit.install-plan.v9".into());
        assert!(serde_json::from_value::<InstallPlanDocument>(value).is_ok());
    }

    #[test]
    fn plan_id_excludes_created_at_and_detects_tampering() {
        let mut plan = InstallPlanDocument {
            schema: INSTALL_PLAN_SCHEMA.to_string(),
            plan_id: String::new(),
            created_at: chrono::Utc::now(),
            request: PlanRequest {
                profile: None,
                provider: "msvc".to_string(),
                host_arch: "x64".to_string(),
                target_arches: vec!["x64".to_string()],
                msvc_version: Some("14.44".to_string()),
                sdk_version: None,
                vs_channel: None,
                include_components: vec![],
                exclude_patterns: vec![],
                include_msvc: true,
                include_sdk: false,
            },
            resolved: ResolvedSummary {
                msvc_version: Some("14.44".to_string()),
                sdk_version: None,
                channel: Some("test".to_string()),
                manifest_digest: Some("sha256:1".to_string()),
                packages: vec!["p1".to_string()],
                payloads: vec![],
            },
            observations: PlanObservations {
                manifest_digest: Some("sha256:1".to_string()),
                manifest_url: None,
                target_root: "C:/t".to_string(),
                free_space_bytes: None,
            },
            actions: vec![],
            disk_usage: DiskUsageEstimate {
                download_bytes: 0,
                reused_cache_bytes: 0,
                staging_bytes: None,
                backup_bytes: None,
                peak_extra_bytes: None,
                final_delta_bytes: None,
                confidence: "unknown".to_string(),
            },
            privilege: PrivilegeRequirement {
                requires_elevation: false,
                reason: "user-writable target".to_string(),
            },
            warnings: vec![],
            blocking_conflicts: vec![],
        };
        plan.plan_id = plan.compute_plan_id();
        assert!(plan.verify_plan_id().is_ok());

        // created_at does not participate in the identity.
        plan.created_at += chrono::Duration::hours(1);
        assert!(plan.verify_plan_id().is_ok());

        // Any content change does.
        plan.warnings.push("changed".to_string());
        assert!(plan.verify_plan_id().is_err());
    }
}
