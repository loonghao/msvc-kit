//! Build Profile and Dependency Resolver (spec 04 §3-4, roadmap P08).
//!
//! The resolver turns a declared build profile into capability sets, maps
//! capabilities to Visual Studio manifest package seeds through the
//! battle-tested `find_msvc_packages` matcher, closes over the manifest
//! dependency graph with a *strict* dependency-value schema (spec 04 §4.3:
//! unknown shapes error, never silently ignored), detects cycles and version
//! conflicts, and emits a stable topological order (Kahn's algorithm, ties
//! broken by package ID so identical input always yields identical output -
//! I-012).
//!
//! Dependency-value shapes observed in real VS 17.14 manifests (sampled on
//! the build machine, 2026-11-22): `{ "<package-id>": { "version": "...",
//! "behaviors": "..." } }` and `{ "<package-id>": "23.0" }`. Exactly these
//! two shapes are accepted; anything else is a resolver error.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::downloader::{MsvcComponent, VsManifest};
use crate::error::{MsvcKitError, Result};

pub(crate) const BUILD_PROFILE_SCHEMA: &str = "msvc-kit.build-profile.v1";

/// Built-in build profiles (spec 04 §3 initial set, roadmap P08).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    /// Minimal runnable C compilation target.
    CNative,
    /// Conventional native C++ build (MSVC + SDK).
    CppNative,
    /// Desktop C++ (Windows SDK headers/libs/tools emphasized).
    CppDesktop,
    /// MFC/ATL projects (explicitly enables the MFC/ATL components).
    CppMfc,
    /// Spectre-mitigated libraries (for `/Qspectre` builds).
    CppSpectre,
    /// x64 host building an ARM64 target.
    CrossArm64,
}

impl BuildProfile {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "c-native" => Ok(Self::CNative),
            "cpp-native" => Ok(Self::CppNative),
            "cpp-desktop" => Ok(Self::CppDesktop),
            "cpp-mfc" => Ok(Self::CppMfc),
            "cpp-spectre" => Ok(Self::CppSpectre),
            "cross-arm64" => Ok(Self::CrossArm64),
            other => Err(MsvcKitError::Config(format!(
                "Unknown build profile {other:?}; valid: c-native, cpp-native, cpp-desktop, \
                 cpp-mfc, cpp-spectre, cross-arm64"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CNative => "c-native",
            Self::CppNative => "cpp-native",
            Self::CppDesktop => "cpp-desktop",
            Self::CppMfc => "cpp-mfc",
            Self::CppSpectre => "cpp-spectre",
            Self::CrossArm64 => "cross-arm64",
        }
    }

    /// Component opt-ins the profile requires on top of the standard set.
    /// Evidence: Spectre libraries are opt-in in `find_msvc_packages`; MFC and
    /// ATL are part of the standard runtime set, so `cpp-mfc` documents intent
    /// and adds nothing beyond the standard matcher.
    pub fn include_components(&self) -> Vec<MsvcComponent> {
        match self {
            Self::CppSpectre => vec![MsvcComponent::Spectre],
            _ => Vec::new(),
        }
    }

    /// Target architecture override for the profile (`cross-arm64` pins it).
    pub fn target_override(&self) -> Option<crate::version::Architecture> {
        match self {
            Self::CrossArm64 => Some(crate::version::Architecture::Arm64),
            _ => None,
        }
    }
}

/// A build-profile document (`msvc-kit.build-profile.v1`, TOML).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildProfileDocument {
    pub schema: String,
    pub id: String,
    /// Package-ID fragments (case-insensitive) selecting manifest packages.
    pub capability_fragments: Vec<String>,
}

impl BuildProfileDocument {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let document: Self = toml::from_str(&raw)?;
        if document.schema != BUILD_PROFILE_SCHEMA {
            return Err(MsvcKitError::Config(format!(
                "Unsupported build profile schema: {} (this build supports {BUILD_PROFILE_SCHEMA})",
                document.schema
            )));
        }
        Ok(document)
    }
}

/// Strictly parsed manifest dependency value. Real VS manifests use exactly
/// two shapes: an object with a `version` (plus optional `behaviors`) or a
/// bare version string. Anything else is rejected (spec 04 §4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencySpec {
    pub package_id: String,
    pub version: String,
}

impl DependencySpec {
    /// Parse one entry of a manifest `dependencies` map.
    pub fn parse(package_id: &str, value: &serde_json::Value) -> Result<Self> {
        match value {
            serde_json::Value::Object(map) => {
                let mut version: Option<String> = None;
                for (field, field_value) in map {
                    if field == "behaviors" {
                        if field_value.as_str().is_none() {
                            return Err(MsvcKitError::Config(format!(
                                "Dependency {package_id:?}: `behaviors` must be a string"
                            )));
                        }
                        continue;
                    }
                    if field == "version" {
                        match field_value.as_str() {
                            Some(text) if !text.trim().is_empty() => {
                                version = Some(text.to_string());
                            }
                            _ => {
                                return Err(MsvcKitError::Config(format!(
                                "Dependency {package_id:?}: `version` must be a non-empty string"
                            )))
                            }
                        }
                    } else {
                        return Err(MsvcKitError::Config(format!(
                            "Dependency {package_id:?}: unknown dependency field {field:?}; \
                             refusing to guess (unknown dependency schema must not be silently \
                             ignored)"
                        )));
                    }
                }
                let version = version.ok_or_else(|| {
                    MsvcKitError::Config(format!(
                        "Dependency {package_id:?}: object shape requires a `version` field"
                    ))
                })?;
                Ok(Self {
                    package_id: package_id.to_string(),
                    version,
                })
            }
            serde_json::Value::String(text) if !text.trim().is_empty() => Ok(Self {
                package_id: package_id.to_string(),
                version: text.clone(),
            }),
            _ => Err(MsvcKitError::Config(format!(
                "Dependency {package_id:?}: unsupported dependency value shape {value}; \
                 refusing to guess (unknown dependency schema must not be silently ignored)"
            ))),
        }
    }
}

/// One resolved package with its resolved dependency edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackage {
    pub id: String,
    pub version: String,
    /// Dependency package IDs declared by the manifest (as parsed).
    pub dependencies: Vec<String>,
}

/// The resolution output: stable topological order + diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedToolchain {
    /// Packages in stable topological order (dependencies before dependents;
    /// ties broken by package ID).
    pub packages: Vec<ResolvedPackage>,
}

/// Errors that abort resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolverError {
    /// A manifest dependency references a package the manifest does not carry.
    UnsatisfiedDependency {
        package_id: String,
        dependency: String,
    },
    /// The dependency graph contains a cycle.
    DependencyCycle { path: Vec<String> },
    /// The same package ID resolves to two different versions.
    VersionConflict {
        package_id: String,
        versions: Vec<String>,
    },
    /// A dependency value shape or field is unknown (spec: never silently
    /// ignore unknown dependency schemas).
    UnknownDependencySchema { detail: String },
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsatisfiedDependency {
                package_id,
                dependency,
            } => write!(
                f,
                "Unsatisfied dependency: {package_id:?} requires {dependency:?}"
            ),
            Self::DependencyCycle { path } => {
                write!(f, "Dependency cycle detected: {}", path.join(" -> "))
            }
            Self::VersionConflict {
                package_id,
                versions,
            } => write!(
                f,
                "Version conflict for {package_id}: conflicting versions {versions:?}"
            ),
            Self::UnknownDependencySchema { detail } => {
                write!(f, "Unknown dependency schema: {detail}")
            }
        }
    }
}

impl ResolverError {
    pub fn to_error(self) -> MsvcKitError {
        match self {
            Self::UnsatisfiedDependency {
                package_id,
                dependency,
            } => MsvcKitError::Config(format!(
                "Unsatisfied dependency: {package_id:?} requires {dependency:?}, which \
                 the manifest does not provide"
            )),
            Self::DependencyCycle { path } => {
                MsvcKitError::Config(format!("Dependency cycle detected: {}", path.join(" -> ")))
            }
            Self::VersionConflict {
                package_id,
                versions,
            } => MsvcKitError::Config(format!(
                "Version conflict for {package_id}: resolved to conflicting versions {versions:?}"
            )),
            Self::UnknownDependencySchema { detail } => {
                MsvcKitError::Config(format!("Unknown dependency schema: {detail}"))
            }
        }
    }
}

/// Extract the strictly-parsed dependency list from a manifest package.
/// Missing/empty `dependencies` maps are fine; unknown shapes error.
pub fn parse_dependencies(
    package_id: &str,
    dependencies: &std::collections::HashMap<String, serde_json::Value>,
) -> Result<Vec<DependencySpec>> {
    let mut specs = Vec::new();
    for (id, value) in dependencies {
        specs.push(DependencySpec::parse(id, value).map_err(|error| {
            ResolverError::UnknownDependencySchema {
                detail: format!("{package_id} -> {}", error),
            }
            .to_error()
        })?);
    }
    Ok(specs)
}

/// Capability-fragment seed selection over the manifest packages (the same
/// substring semantics as the existing component matching; `--exclude-pattern`
/// continues to apply at seed level).
pub fn select_seeds(
    manifest: &VsManifest,
    version_prefix: &str,
    fragments: &[String],
    exclude_patterns: &[String],
) -> Vec<String> {
    let prefix = format!("microsoft.vc.{}.", version_prefix.to_lowercase());
    let mut seeds: BTreeSet<String> = BTreeSet::new();
    for package in &manifest.packages {
        let id = package.id.to_lowercase();
        if !id.starts_with(&prefix) {
            continue;
        }
        if exclude_patterns
            .iter()
            .any(|pattern| id.contains(&pattern.to_lowercase()))
        {
            continue;
        }
        if fragments
            .iter()
            .any(|fragment| id.contains(&fragment.to_lowercase()))
        {
            seeds.insert(package.id.clone());
        }
    }
    seeds.into_iter().collect()
}

/// Resolve the dependency closure over the manifest and return the stable
/// topological order.
///
/// - Seed packages are the profile-selected IDs.
/// - Each seed's manifest `dependencies` are parsed strictly; unknown shapes
///   abort.
/// - A dependency that the manifest does not provide is an error (no
///   best-effort substitution; spec 04 §4.3).
/// - The same package resolving to two different versions is a conflict.
/// - Cycles are reported with the offending nodes.
pub fn resolve(
    manifest: &VsManifest,
    seeds: &[String],
) -> std::result::Result<ResolvedToolchain, ResolverError> {
    let mut by_id: BTreeMap<String, &crate::downloader::VsPackage> = BTreeMap::new();
    for package in &manifest.packages {
        by_id.insert(package.id.clone(), package);
    }

    struct Node {
        version: String,
        deps: Vec<String>,
    }
    let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
    let mut queue: std::collections::VecDeque<String> = seeds.iter().cloned().collect();
    let mut seeded: BTreeSet<String> = seeds.iter().cloned().collect();

    while let Some(id) = queue.pop_front() {
        let package = *by_id
            .get(&id)
            .ok_or_else(|| ResolverError::UnsatisfiedDependency {
                package_id: "(seed set)".to_string(),
                dependency: id.clone(),
            })?;
        let specs = parse_dependencies(&package.id, &package.dependencies).map_err(|error| {
            ResolverError::UnknownDependencySchema {
                detail: error.to_string(),
            }
        })?;
        let deps: Vec<String> = specs.iter().map(|spec| spec.package_id.clone()).collect();

        if let Some(existing) = nodes.get(&package.id) {
            if existing.version != package.version {
                return Err(ResolverError::VersionConflict {
                    package_id: package.id.clone(),
                    versions: vec![existing.version.clone(), package.version.clone()],
                });
            }
            continue;
        }
        nodes.insert(
            package.id.clone(),
            Node {
                version: package.version.clone(),
                deps: deps.clone(),
            },
        );
        for spec in specs {
            let dependency = *by_id.get(&spec.package_id).ok_or_else(|| {
                ResolverError::UnsatisfiedDependency {
                    package_id: package.id.clone(),
                    dependency: spec.package_id.clone(),
                }
            })?;
            if dependency.version != spec.version
                && !version_prefix_matches(&dependency.version, &spec.version)
            {
                return Err(ResolverError::VersionConflict {
                    package_id: spec.package_id.clone(),
                    versions: vec![dependency.version.clone(), spec.version.clone()],
                });
            }
            if seeded.insert(spec.package_id.clone()) {
                queue.push_back(spec.package_id);
            }
        }
    }

    // Cycle detection + stable topological order (Kahn, ID-lexicographic).
    let mut remaining_deps: BTreeMap<String, BTreeSet<String>> = nodes
        .iter()
        .map(|(id, node)| {
            (
                id.clone(),
                node.deps
                    .iter()
                    .filter(|dep| nodes.contains_key(dep.as_str()))
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    let mut order: Vec<String> = Vec::new();
    let mut ready: Vec<String> = remaining_deps
        .iter()
        .filter(|(_, deps)| deps.is_empty())
        .map(|(id, _)| id.clone())
        .collect();
    ready.sort();
    ready.dedup();
    while !ready.is_empty() {
        let id = ready.remove(0);
        remaining_deps.remove(&id);
        order.push(id.clone());
        for (dependent, deps) in remaining_deps.iter_mut() {
            if deps.remove(&id) && deps.is_empty() {
                ready.push(dependent.clone());
            }
        }
        ready.sort();
        ready.dedup();
    }
    if order.len() != nodes.len() {
        let cycle_nodes: Vec<String> = remaining_deps.keys().cloned().collect();
        return Err(ResolverError::DependencyCycle { path: cycle_nodes });
    }

    let packages = order
        .iter()
        .filter_map(|id| nodes.get(id).map(|node| (id, node)))
        .map(|(id, node)| ResolvedPackage {
            id: id.clone(),
            version: node.version.clone(),
            dependencies: node.deps.clone(),
        })
        .collect();

    Ok(ResolvedToolchain { packages })
}

/// Manifest dependency versions frequently pin a shorter family ("23.0"
/// against "23.0.12345"): the requirement matches when the dependency string
/// is a dot-prefix of the actual version.
fn version_prefix_matches(actual: &str, required: &str) -> bool {
    let actual_parts: Vec<&str> = actual.split('.').collect();
    let required_parts: Vec<&str> = required.split('.').collect();
    required_parts.len() <= actual_parts.len()
        && actual_parts
            .iter()
            .zip(required_parts.iter())
            .all(|(actual, required)| actual == required)
}

/// Resolve a built-in profile: seeds come from `find_msvc_packages` (the
/// battle-tested matcher, including the profile's component opt-ins), then
/// the dependency closure runs over the manifest graph.
pub fn profile_seeds(
    manifest: &VsManifest,
    profile: &BuildProfile,
    version_prefix: &str,
    host: &str,
    target: &str,
    exclude_patterns: &[String],
) -> Result<ResolvedToolchain> {
    let mut include_components = profile.include_components();
    // MFC/ATL profiles explicitly enable the MFC/ATL components; the standard
    // matcher already includes them, so cpp-mfc only documents intent.
    if *profile == BuildProfile::CppMfc {
        include_components.push(MsvcComponent::Mfc);
        include_components.push(MsvcComponent::Atl);
    }
    let include_set: std::collections::HashSet<MsvcComponent> =
        include_components.into_iter().collect();
    let seed_packages =
        manifest.find_msvc_packages(version_prefix, host, target, &include_set, exclude_patterns);
    let seed_ids: Vec<String> = seed_packages
        .iter()
        .map(|package| package.id.clone())
        .collect();
    resolve(manifest, &seed_ids).map_err(|error| error.to_error())
}

/// Test helper: construct a minimal manifest for unit tests.
#[cfg(test)]
pub(crate) fn __test_manifest() -> VsManifest {
    use crate::downloader::VsPackage;
    VsManifest {
        manifest_version: "1.0".to_string(),
        engine_version: None,
        packages: vec![VsPackage {
            id: "Microsoft.VC.14.44.Tools.HostX64.TargetX64".to_string(),
            version: "14.44.34823".to_string(),
            package_type: "VSIX".to_string(),
            chip: None,
            language: None,
            payloads: Vec::new(),
            dependencies: Default::default(),
            machine_arch: None,
            product_arch: None,
        }],
    }
}

/// Load a build-profile document (custom profiles): returns the profile id
/// and its capability fragments for seed selection.
pub fn load_profile(path: &Path) -> Result<(String, Vec<String>)> {
    let document = BuildProfileDocument::load(path)?;
    Ok((document.id, document.capability_fragments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::VsPackage;

    const V: &str = "1.0.0";

    fn pkg(id: &str, version: &str, deps: Vec<(&str, serde_json::Value)>) -> VsPackage {
        VsPackage {
            id: id.to_string(),
            version: version.to_string(),
            package_type: "VSIX".to_string(),
            chip: None,
            language: None,
            payloads: Vec::new(),
            dependencies: deps
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
            machine_arch: None,
            product_arch: None,
        }
    }

    fn manifest(packages: Vec<VsPackage>) -> VsManifest {
        VsManifest {
            manifest_version: "1.0".to_string(),
            engine_version: None,
            packages,
        }
    }

    #[test]
    fn chain_closure_and_stable_topological_order() {
        // C depends on B, B depends on A: order must be A, B, C.
        let m = manifest(vec![
            pkg("P.C", V, vec![("P.B", serde_json::json!({"version": V}))]),
            pkg("P.B", V, vec![("P.A", serde_json::json!({"version": V}))]),
            pkg("P.A", V, vec![]),
        ]);
        let resolved = resolve(&m, &["P.C".to_string()]).unwrap();
        let ids: Vec<&str> = resolved.packages.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["P.A", "P.B", "P.C"],
            "dependencies before dependents"
        );
    }

    #[test]
    fn diamond_and_repeated_runs_are_deterministic() {
        let m = manifest(vec![
            pkg(
                "P.D",
                V,
                vec![("P.B", serde_json::json!(V)), ("P.C", serde_json::json!(V))],
            ),
            pkg("P.C", V, vec![("P.A", serde_json::json!(V))]),
            pkg("P.B", V, vec![("P.A", serde_json::json!(V))]),
            pkg("P.A", V, vec![]),
        ]);
        let first = resolve(&m, &["P.D".to_string()]).unwrap();
        let second = resolve(&m, &["P.D".to_string()]).unwrap();
        assert_eq!(first, second, "same input must resolve identically (I-012)");
        let ids: Vec<&str> = first.packages.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"P.A"));
        assert_eq!(ids.last(), Some(&"P.D"));
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn cycle_is_detected_with_the_nodes() {
        let m = manifest(vec![
            pkg("P.A", V, vec![("P.B", serde_json::json!(V))]),
            pkg("P.B", V, vec![("P.A", serde_json::json!(V))]),
        ]);
        let error = resolve(&m, &["P.A".to_string()]).unwrap_err();
        match error {
            ResolverError::DependencyCycle { path } => {
                assert!(path.contains(&"P.A".to_string()) && path.contains(&"P.B".to_string()));
            }
            other => panic!("expected cycle, got {other:?}"),
        }
    }

    #[test]
    fn missing_dependency_is_an_error_not_a_guess() {
        let m = manifest(vec![pkg(
            "P.A",
            V,
            vec![("P.Ghost", serde_json::json!({"version": V}))],
        )]);
        let error = resolve(&m, &["P.A".to_string()]).unwrap_err();
        assert!(
            matches!(error, ResolverError::UnsatisfiedDependency { ref dependency, .. } if dependency == "P.Ghost"),
            "{error:?}"
        );
    }

    #[test]
    fn unknown_dependency_shapes_are_rejected() {
        // Unexpected dependency VALUE shape (array).
        let m = manifest(vec![pkg(
            "P.A",
            V,
            vec![("P.B", serde_json::json!(["weird"]))],
        )]);
        let error = resolve(&m, &["P.A".to_string()]).unwrap_err();
        assert!(
            matches!(error, ResolverError::UnknownDependencySchema { .. }),
            "{error:?}"
        );

        // Object with an unknown FIELD.
        let m = manifest(vec![pkg(
            "P.A",
            V,
            vec![(
                "P.B",
                serde_json::json!({"version": V, "installer": "custom.exe"}),
            )],
        )]);
        let error = resolve(&m, &["P.A".to_string()]).unwrap_err();
        assert!(
            matches!(error, ResolverError::UnknownDependencySchema { .. }),
            "{error:?}"
        );

        // Object missing `version`.
        let m = manifest(vec![pkg("P.A", V, vec![("P.B", serde_json::json!({}))])]);
        let error = resolve(&m, &["P.A".to_string()]).unwrap_err();
        assert!(
            matches!(error, ResolverError::UnknownDependencySchema { .. }),
            "{error:?}"
        );

        // Non-string behaviors.
        let m = manifest(vec![pkg(
            "P.A",
            V,
            vec![("P.B", serde_json::json!({"version": V, "behaviors": 3}))],
        )]);
        let error = resolve(&m, &["P.A".to_string()]).unwrap_err();
        assert!(
            matches!(error, ResolverError::UnknownDependencySchema { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn version_conflict_between_dependency_requirement_and_manifest() {
        let m = manifest(vec![
            pkg("P.A", V, vec![]),
            pkg("P.B", "2.0.0", vec![("P.A", serde_json::json!("9.9.9"))]),
        ]);
        let error = resolve(&m, &["P.B".to_string()]).unwrap_err();
        assert!(
            matches!(error, ResolverError::VersionConflict { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn short_version_prefix_requirement_matches_full_version() {
        // Real manifests pin "23.0" against "23.0.12345" - a dot-prefix match.
        let m = manifest(vec![
            pkg("P.A", "23.0.12345", vec![]),
            pkg("P.B", V, vec![("P.A", serde_json::json!("23.0"))]),
        ]);
        let resolved = resolve(&m, &["P.B".to_string()]).unwrap();
        assert_eq!(resolved.packages.len(), 2);
    }

    #[test]
    fn select_seeds_applies_fragments_and_excludes() {
        let m = manifest(vec![
            pkg("Microsoft.VC.14.44.Tools.HostX64.TargetX64", V, vec![]),
            pkg("Microsoft.VC.14.44.CRT.x64.Desktop", V, vec![]),
            pkg("Microsoft.VC.14.44.MFC.x64", V, vec![]),
        ]);
        let seeds = select_seeds(&m, "14.44", &["tools".to_string(), "mfc".to_string()], &[]);
        assert!(seeds.contains(&"Microsoft.VC.14.44.Tools.HostX64.TargetX64".to_string()));
        assert!(seeds.contains(&"Microsoft.VC.14.44.MFC.x64".to_string()));
        assert!(!seeds.iter().any(|id| id.contains("CRT")));

        // Exclude patterns knock packages out at seed level.
        let seeds = select_seeds(
            &m,
            "14.44",
            &["tools".to_string(), "mfc".to_string()],
            &["mfc".to_string()],
        );
        assert!(!seeds.iter().any(|id| id.contains("MFC")));
    }

    #[test]
    fn profile_capability_fragments_cover_the_documented_families() {
        assert_eq!(
            BuildProfile::CNative.include_components(),
            Vec::<MsvcComponent>::new()
        );
        assert_eq!(
            BuildProfile::CppSpectre.include_components(),
            vec![MsvcComponent::Spectre]
        );
        assert_eq!(
            BuildProfile::CrossArm64.target_override(),
            Some(crate::version::Architecture::Arm64)
        );
        assert!(BuildProfile::parse("cpp-desktop").is_ok());
        assert!(BuildProfile::parse("bogus").is_err());
    }

    #[test]
    fn build_profile_document_round_trips_through_toml() {
        let document = BuildProfileDocument {
            schema: BUILD_PROFILE_SCHEMA.to_string(),
            id: "cpp-desktop".to_string(),
            capability_fragments: vec!["tools".to_string(), "crt".to_string()],
        };
        let text = toml::to_string(&document).unwrap();
        let parsed: BuildProfileDocument = toml::from_str(&text).unwrap();
        assert_eq!(document, parsed);
    }

    #[test]
    fn profile_seeds_resolves_a_realistic_desktop_set() {
        let tools_id = "Microsoft.VC.14.44.Tools.HostX64.TargetX64";
        let crt_id = "Microsoft.VC.14.44.CRT.x64.Desktop";
        let m = manifest(vec![
            pkg(
                tools_id,
                V,
                vec![(crt_id, serde_json::json!({"version": V}))],
            ),
            pkg(crt_id, V, vec![]),
            pkg("Microsoft.VC.14.44.MFC.x64", V, vec![]),
        ]);
        let resolved =
            profile_seeds(&m, &BuildProfile::CppNative, "14.44", "x64", "x64", &[]).unwrap();
        let ids: Vec<&str> = resolved.packages.iter().map(|p| p.id.as_str()).collect();
        // Tools seed closes over its CRT dependency (topo order: CRT first);
        // MFC/ATL ride along because the standard matcher includes them - the
        // profile documents intent for MFC-specific work, it does not subtract.
        assert_eq!(ids, vec![crt_id, "Microsoft.VC.14.44.MFC.x64", tools_id]);
    }
}
