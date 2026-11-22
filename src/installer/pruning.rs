//! Pruning engine (spec 06 §1).
//!
//! Rules are declarative, version-scoped and auditable: each rule names the
//! paths it removes with explicit placeholders, carries a reason, and is
//! scoped to a pruning profile. `full` never prunes; `portable` ships only
//! evidence-backed rules (telemetry payloads and non-target-architecture SDK
//! libraries); `minimal` deliberately adds nothing until a rule has its own
//! compile/link acceptance evidence. Pruning executes on the transaction
//! staging tree only - the install root is never pruned in place - and every
//! removed path is recorded for the receipt (path, rule id, reason, size).
//!
//! Deliberate non-rules (spec 06 §1.4): PDBs, DIA SDK, debug CRT and the
//! Auxiliary tree are never pruned by any built-in profile.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{MsvcKitError, Result};

/// Pruning profile identifiers (spec 06 §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PruningProfile {
    /// No pruning at all: every byte of every selected package is installed.
    #[default]
    Full,
    /// Evidence-backed trimming only: telemetry payloads and libraries for
    /// architectures the plan did not select.
    Portable,
    /// Reserved for more aggressive rules; empty until a rule carries its own
    /// compile/link acceptance evidence.
    Minimal,
    /// User-provided rule document (path to a `msvc-kit.pruning-rules.v1`
    /// TOML file); never enabled by default.
    Custom,
}

impl PruningProfile {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "full" => Ok(Self::Full),
            "portable" => Ok(Self::Portable),
            "minimal" => Ok(Self::Minimal),
            "custom" => Ok(Self::Custom),
            other => Err(MsvcKitError::Config(format!(
                "Unknown pruning profile {other:?}; valid: full, portable, minimal, custom"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Portable => "portable",
            Self::Minimal => "minimal",
            Self::Custom => "custom",
        }
    }
}

pub(crate) const PRUNING_RULES_SCHEMA: &str = "msvc-kit.pruning-rules.v1";

/// One declarative pruning rule (spec 06 §1.2 shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PruningRule {
    /// Stable identifier used in receipts and journals.
    pub id: String,
    /// Rule revision; bump when the match semantics change.
    pub version: u32,
    /// `remove` (files) or `remove_dir_if_empty` (directories).
    pub action: String,
    /// Path templates relative to the install root. Placeholders:
    /// `{vctools}` (full MSVC version), `{sdk}` (full SDK version),
    /// `{hostdir}` (HostX64 form), `{target}` (x64 form), `{arch}` (every
    /// architecture the plan did NOT select), `**` (any suffix).
    pub match_patterns: Vec<String>,
    /// Human-readable justification carried into receipts.
    pub reason: String,
    /// MSVC toolset versions the rule applies to (dot-prefixes, e.g. `14`);
    /// empty means "all versions the placeholders resolve for".
    #[serde(default)]
    pub applies_to_msvc_versions: Vec<String>,
    /// Package IDs whose files the rule may touch; empty means any.
    #[serde(default)]
    pub requires_package: Vec<String>,
}

/// A rule document (`msvc-kit.pruning-rules.v1`, TOML on disk).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PruningRulesDocument {
    pub schema: String,
    pub profile: String,
    pub rules: Vec<PruningRule>,
}

/// Context that resolves rule placeholders for one install.
#[derive(Debug, Clone, Default)]
pub struct PruningContext {
    /// Full MSVC toolset version (e.g. `14.44.34823`), when known.
    pub msvc_version: Option<String>,
    /// Full Windows SDK version (e.g. `10.0.26100.0`), when known.
    pub sdk_version: Option<String>,
    /// Host architecture in `HostX64` form.
    pub host: Option<String>,
    /// Target architecture in `x64` form.
    pub target: Option<String>,
    /// Every architecture the install selected (raw forms), for
    /// architecture filters.
    pub selected_arches: BTreeSet<String>,
}

/// One planned removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneAction {
    /// Path relative to the staging root (and, post-commit, the install root).
    pub relative: PathBuf,
    pub rule_id: String,
    pub rule_version: u32,
    pub reason: String,
    pub size: u64,
}

/// Load a custom rules document from TOML.
pub fn load_rules_document(path: &Path) -> Result<PruningRulesDocument> {
    let raw = std::fs::read_to_string(path)?;
    let document: PruningRulesDocument = toml::from_str(&raw)?;
    validate_rules(&document)?;
    Ok(document)
}

fn validate_rules(document: &PruningRulesDocument) -> Result<()> {
    if document.schema != PRUNING_RULES_SCHEMA {
        return Err(MsvcKitError::Config(format!(
            "Unsupported pruning rules schema: {} (this build supports {PRUNING_RULES_SCHEMA})",
            document.schema
        )));
    }
    let mut seen = std::collections::HashSet::new();
    for rule in &document.rules {
        if !matches!(rule.action.as_str(), "remove" | "remove_dir_if_empty") {
            return Err(MsvcKitError::Config(format!(
                "Pruning rule {} has unsupported action {:?}",
                rule.id, rule.action
            )));
        }
        if rule.match_patterns.is_empty() {
            return Err(MsvcKitError::Config(format!(
                "Pruning rule {} has no match patterns",
                rule.id
            )));
        }
        if !seen.insert(&rule.id) {
            return Err(MsvcKitError::Config(format!(
                "Duplicate pruning rule id {}",
                rule.id
            )));
        }
    }
    Ok(())
}

/// The built-in portable-profile rules. Every entry is evidence-backed
/// (portablemsvc removes the same payloads; the paths are telemetry-only
/// artifacts and libraries for architectures the plan did not select).
pub fn builtin_rules(profile: PruningProfile) -> Vec<PruningRule> {
    match profile {
        PruningProfile::Full | PruningProfile::Minimal | PruningProfile::Custom => Vec::new(),
        PruningProfile::Portable => vec![
            PruningRule {
                id: "msvc.telemetry.vctip".to_string(),
                version: 1,
                action: "remove".to_string(),
                match_patterns: vec![
                    "VC/Tools/MSVC/{vctools}/bin/{hostdir}/{target}/vctip.exe".to_string(),
                ],
                reason: "Telemetry-only executable; not required for compilation or linking"
                    .to_string(),
                applies_to_msvc_versions: vec!["14".to_string()],
                requires_package: Vec::new(),
            },
            PruningRule {
                id: "msvc.telemetry.vs_telemetry_dll".to_string(),
                version: 1,
                action: "remove".to_string(),
                match_patterns: vec![
                    "VC/Tools/MSVC/{vctools}/bin/{hostdir}/{target}/Microsoft.VisualStudio.Telemetry.dll"
                        .to_string(),
                ],
                reason: "Telemetry-only library; not required for compilation or linking"
                    .to_string(),
                applies_to_msvc_versions: vec!["14".to_string()],
                requires_package: Vec::new(),
            },
            PruningRule {
                id: "sdk.non_target_arch_libs".to_string(),
                version: 1,
                action: "remove".to_string(),
                match_patterns: vec![
                    "Windows Kits/10/Lib/{sdk}/um/{arch}/**".to_string(),
                    "Windows Kits/10/Lib/{sdk}/ucrt/{arch}/**".to_string(),
                ],
                reason: "Libraries for an architecture this install did not select"
                    .to_string(),
                applies_to_msvc_versions: Vec::new(),
                requires_package: Vec::new(),
            },
        ],
    }
}

/// Expand one pattern against the context into concrete candidate paths.
/// Every architecture a rule's `{arch}` placeholder may expand to.
const KNOWN_ARCHES: [&str; 4] = ["x64", "x86", "arm64", "arm"];

fn expand_pattern(pattern: &str, context: &PruningContext) -> Result<Vec<String>> {
    // {arch} expands to every KNOWN architecture the install did NOT select:
    // that is the semantics of the non-target-architecture library rule.
    if pattern.contains("{arch}") {
        if context.selected_arches.is_empty() {
            return Ok(Vec::new());
        }
        let mut current = Vec::new();
        for arch in KNOWN_ARCHES {
            if context.selected_arches.contains(arch) {
                continue;
            }
            current.push(pattern.replace("{arch}", arch));
        }
        return expand_without_arch(current, context);
    }
    expand_without_arch(vec![pattern.to_string()], context)
}

fn expand_without_arch(mut current: Vec<String>, context: &PruningContext) -> Result<Vec<String>> {
    let replacements: [(&str, &Option<String>); 4] = [
        ("{vctools}", &context.msvc_version),
        ("{sdk}", &context.sdk_version),
        ("{hostdir}", &context.host),
        ("{target}", &context.target),
    ];
    for (token, value) in replacements {
        let Some(value) = value else {
            if current.iter().any(|candidate| candidate.contains(token)) {
                // The pattern needs context the plan does not provide: the
                // rule cannot fire this install.
                return Ok(Vec::new());
            }
            continue;
        };
        current = current
            .into_iter()
            .map(|candidate| candidate.replace(token, value))
            .collect();
    }
    Ok(current)
}

/// Whether `relative` (forward-slash form) matches one expanded pattern.
/// `**` matches any remaining suffix; everything else is exact.
fn matches_pattern(relative: &str, pattern: &str) -> bool {
    let relative = relative.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return relative == prefix || relative.starts_with(&format!("{prefix}/"));
    }
    relative == pattern
}

/// Compute the pruning plan for a staged file list. `full` and rule-less
/// profiles return an empty plan; nothing here touches the file system.
pub fn plan_pruning(
    profile: PruningProfile,
    staged_files: &[(PathBuf, String, u64, String)],
    context: &PruningContext,
) -> Result<Vec<PruneAction>> {
    let rules = match profile {
        PruningProfile::Full => Vec::new(),
        PruningProfile::Custom => Vec::new(),
        _ => builtin_rules(profile),
    };
    plan_rules(&rules, staged_files, context)
}

/// Plan against an explicit rule list (custom documents, tests).
pub fn plan_rules(
    rules: &[PruningRule],
    staged_files: &[(PathBuf, String, u64, String)],
    context: &PruningContext,
) -> Result<Vec<PruneAction>> {
    let mut actions: Vec<PruneAction> = Vec::new();
    let mut applied: std::collections::HashSet<(String, PathBuf)> =
        std::collections::HashSet::new();

    for rule in rules {
        if !rule.applies_to_msvc_versions.is_empty() {
            let Some(version) = &context.msvc_version else {
                continue;
            };
            let applies = rule
                .applies_to_msvc_versions
                .iter()
                .any(|prefix| version.starts_with(prefix.as_str()));
            if !applies {
                continue;
            }
        }
        for pattern in &rule.match_patterns {
            for expanded in expand_pattern(pattern, context)? {
                for (relative, _sha, size, package_id) in staged_files {
                    let rel_text = relative.to_string_lossy().replace('\\', "/");
                    if !matches_pattern(&rel_text, &expanded) {
                        continue;
                    }
                    if !rule.requires_package.is_empty()
                        && !rule.requires_package.iter().any(|pkg| pkg == package_id)
                    {
                        continue;
                    }
                    if applied.insert((rule.id.clone(), relative.clone())) {
                        actions.push(PruneAction {
                            relative: relative.clone(),
                            rule_id: rule.id.clone(),
                            rule_version: rule.version,
                            reason: rule.reason.clone(),
                            size: *size,
                        });
                    }
                }
            }
        }
    }
    actions.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(actions)
}

/// Execute a pruning plan against the staging tree. Only staged files are
/// touched; failures abort the transaction (spec 06 §1.5).
pub fn execute_pruning(
    staging_root: &Path,
    actions: &[PruneAction],
    mut stage: impl FnMut(&PruneAction) -> Result<()>,
) -> Result<Vec<PruneAction>> {
    let mut executed = Vec::new();
    for action in actions {
        let absolute = staging_root.join(&action.relative);
        if !absolute.is_file() {
            return Err(MsvcKitError::Other(format!(
                "Pruning rule {} planned {} but the staged file is missing",
                action.rule_id,
                action.relative.display()
            )));
        }
        stage(action)?;
        executed.push(action.clone());
    }
    Ok(executed)
}

/// Remove one staged file for a prune action.
pub fn remove_staged_file(staging_root: &Path, action: &PruneAction) -> Result<()> {
    let absolute = staging_root.join(&action.relative);
    std::fs::remove_file(&absolute)?;
    // Clean now-empty parent directories up to the staging root.
    let mut parent = absolute.parent();
    while let Some(dir) = parent {
        if dir == staging_root || !dir.is_dir() {
            break;
        }
        if std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none()) {
            let _ = std::fs::remove_dir(dir);
        } else {
            break;
        }
        parent = dir.parent();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PKG: &str = "Microsoft.VC.14.44.Tools.HostX64.TargetX64";
    use sha2::{Digest, Sha256};

    fn sha_of(path: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn staged(entries: &[(&str, u64, &str)]) -> Vec<(PathBuf, String, u64, String)> {
        entries
            .iter()
            .map(|(path, size, pkg)| {
                (
                    PathBuf::from(path.replace('/', "\\")),
                    sha_of(path),
                    *size,
                    (*pkg).to_string(),
                )
            })
            .collect()
    }

    fn context() -> PruningContext {
        PruningContext {
            msvc_version: Some("14.44.34823".to_string()),
            sdk_version: Some("10.0.26100.0".to_string()),
            host: Some("HostX64".to_string()),
            target: Some("x64".to_string()),
            selected_arches: BTreeSet::from(["x64".to_string()]),
        }
    }

    #[test]
    fn full_profile_never_prunes() {
        let files = staged(&[
            (
                "VC/Tools/MSVC/14.44.34823/bin/HostX64/x64/vctip.exe",
                10,
                PKG,
            ),
            (
                "Windows Kits/10/Lib/10.0.26100.0/um/arm64/thing.lib",
                10,
                PKG,
            ),
        ]);
        assert!(plan_pruning(PruningProfile::Full, &files, &context())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn portable_prunes_telemetry_and_non_target_arch_libs_only() {
        let files = staged(&[
            // telemetry: pruned
            (
                "VC/Tools/MSVC/14.44.34823/bin/HostX64/x64/vctip.exe",
                100,
                PKG,
            ),
            (
                "VC/Tools/MSVC/14.44.34823/bin/HostX64/x64/Microsoft.VisualStudio.Telemetry.dll",
                200,
                PKG,
            ),
            // non-target arch libs: pruned
            (
                "Windows Kits/10/Lib/10.0.26100.0/um/arm64/thing.lib",
                300,
                PKG,
            ),
            (
                "Windows Kits/10/Lib/10.0.26100.0/ucrt/arm64/ucrt.lib",
                400,
                PKG,
            ),
            // same-tree PDBs must survive (spec 06 section 1.4)
            (
                "VC/Tools/MSVC/14.44.34823/bin/HostX64/x64/vctip.pdb",
                500,
                PKG,
            ),
            // selected-arch libs must survive
            (
                "Windows Kits/10/Lib/10.0.26100.0/um/x64/kernel32.lib",
                600,
                PKG,
            ),
            // lookalike names outside the pattern must survive
            (
                "VC/Tools/MSVC/14.44.34823/bin/HostX64/x64/avctip.exe",
                700,
                PKG,
            ),
        ]);
        let actions = plan_pruning(PruningProfile::Portable, &files, &context()).unwrap();
        let pruned: Vec<String> = actions
            .iter()
            .map(|action| action.relative.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(
            pruned,
            vec![
                "VC/Tools/MSVC/14.44.34823/bin/HostX64/x64/Microsoft.VisualStudio.Telemetry.dll",
                "VC/Tools/MSVC/14.44.34823/bin/HostX64/x64/vctip.exe",
                "Windows Kits/10/Lib/10.0.26100.0/ucrt/arm64/ucrt.lib",
                "Windows Kits/10/Lib/10.0.26100.0/um/arm64/thing.lib",
            ]
        );
        // Every action carries its rule id and reason for the receipt.
        assert!(actions.iter().all(|action| !action.reason.is_empty()
            && action.rule_id.starts_with("msvc.telemetry.")
            || action.rule_id == "sdk.non_target_arch_libs"));
        // PDB and lookalike survive.
        assert!(files
            .iter()
            .any(|(path, ..)| path.to_string_lossy().ends_with("vctip.pdb")));
    }

    #[test]
    fn version_scoped_rules_skip_other_versions() {
        let rules = builtin_rules(PruningProfile::Portable);
        let files = staged(&[(
            "VC/Tools/MSVC/14.99.99999/bin/HostX64/x64/vctip.exe",
            10,
            PKG,
        )]);
        let mut context = context();
        context.msvc_version = Some("14.99.99999".to_string());
        // 14.99 still starts with "14": applies. Use an out-of-scope version.
        context.msvc_version = Some("15.1.1".to_string());
        let actions = plan_rules(&rules, &files, &context).unwrap();
        assert!(
            actions.is_empty(),
            "rules must not fire outside their version range"
        );
    }

    #[test]
    fn custom_document_round_trips_through_toml() {
        let document = PruningRulesDocument {
            schema: PRUNING_RULES_SCHEMA.to_string(),
            profile: "custom".to_string(),
            rules: vec![PruningRule {
                id: "custom.example".to_string(),
                version: 2,
                action: "remove".to_string(),
                match_patterns: vec!["VC/Tools/MSVC/{vctools}/bin/**".to_string()],
                reason: "example".to_string(),
                applies_to_msvc_versions: vec!["14".to_string()],
                requires_package: vec!["pkg".to_string()],
            }],
        };
        let toml_text = toml::to_string(&document).unwrap();
        let parsed: PruningRulesDocument = toml::from_str(&toml_text).unwrap();
        assert_eq!(document, parsed);
        assert!(validate_rules(&parsed).is_ok());

        // Unknown fields and bad actions are refused.
        let bad = toml_text.replace("remove", "format_disk");
        assert!(
            toml::from_str::<PruningRulesDocument>(&bad).is_err()
                || validate_rules(&toml::from_str::<PruningRulesDocument>(&bad).unwrap()).is_err()
        );
    }

    #[test]
    fn execution_removes_only_planned_files_and_requires_them_present() {
        let root = tempfile::tempdir().unwrap();
        let planned_rel = PathBuf::from("VC/Tools/x/vctip.exe");
        let absolute = root.path().join(&planned_rel);
        std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        std::fs::write(&absolute, b"telemetry").unwrap();

        let actions = vec![PruneAction {
            relative: planned_rel.clone(),
            rule_id: "msvc.telemetry.vctip".to_string(),
            rule_version: 1,
            reason: "telemetry".to_string(),
            size: 9,
        }];
        let executed = execute_pruning(root.path(), &actions, |action| {
            remove_staged_file(root.path(), action)
        })
        .unwrap();
        assert_eq!(executed.len(), 1);
        assert!(!absolute.exists());
        // Empty parent directories are cleaned up to the staging root.
        assert!(!root.path().join("VC").join("Tools").join("x").exists());

        // A plan referencing a missing staged file must fail (no silent skip).
        let missing = vec![PruneAction {
            relative: PathBuf::from("VC/Tools/x/gone.exe"),
            rule_id: "msvc.telemetry.vctip".to_string(),
            rule_version: 1,
            reason: "telemetry".to_string(),
            size: 1,
        }];
        assert!(execute_pruning(root.path(), &missing, |action| {
            remove_staged_file(root.path(), action)
        })
        .is_err());
    }
}
