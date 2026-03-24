//! Query module for inspecting installed MSVC toolchain components
//!
//! This module provides a structured API for querying installed MSVC and
//! Windows SDK components, including paths, environment variables, and
//! tool locations. It is designed for both programmatic (library) and
//! CLI usage.
//!
//! # Example
//!
//! ```rust,no_run
//! use msvc_kit::query::{QueryOptions, QueryResult, query_installation};
//! use msvc_kit::Architecture;
//!
//! let options = QueryOptions::builder()
//!     .install_dir("C:/msvc-kit")
//!     .arch(Architecture::X64)
//!     .build();
//!
//! let result = query_installation(&options)?;
//!
//! // Get cl.exe path
//! if let Some(cl) = result.tool_path("cl") {
//!     println!("cl.exe: {}", cl.display());
//! }
//!
//! // Get environment variables
//! for (key, value) in &result.env_vars {
//!     println!("{}={}", key, value);
//! }
//! # Ok::<(), msvc_kit::MsvcKitError>(())
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::{get_env_vars, MsvcEnvironment};
use crate::error::{MsvcKitError, Result};
use crate::installer::InstallInfo;
use crate::version::{list_installed_msvc, list_installed_sdk, Architecture};

/// Which component to query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryComponent {
    /// Query both MSVC and SDK (default)
    All,
    /// Query only MSVC compiler
    Msvc,
    /// Query only Windows SDK
    Sdk,
}

impl Default for QueryComponent {
    fn default() -> Self {
        Self::All
    }
}

impl std::fmt::Display for QueryComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryComponent::All => write!(f, "all"),
            QueryComponent::Msvc => write!(f, "msvc"),
            QueryComponent::Sdk => write!(f, "sdk"),
        }
    }
}

impl std::str::FromStr for QueryComponent {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(QueryComponent::All),
            "msvc" => Ok(QueryComponent::Msvc),
            "sdk" | "winsdk" => Ok(QueryComponent::Sdk),
            _ => Err(format!(
                "Unknown component '{}'. Valid: all, msvc, sdk",
                s
            )),
        }
    }
}

/// What property to query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryProperty {
    /// Return all information (default)
    All,
    /// Return installation paths
    Path,
    /// Return environment variables
    Env,
    /// Return tool executable paths (cl.exe, link.exe, etc.)
    Tools,
    /// Return version information
    Version,
    /// Return include paths
    Include,
    /// Return library paths
    Lib,
}

impl Default for QueryProperty {
    fn default() -> Self {
        Self::All
    }
}

impl std::fmt::Display for QueryProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryProperty::All => write!(f, "all"),
            QueryProperty::Path => write!(f, "path"),
            QueryProperty::Env => write!(f, "env"),
            QueryProperty::Tools => write!(f, "tools"),
            QueryProperty::Version => write!(f, "version"),
            QueryProperty::Include => write!(f, "include"),
            QueryProperty::Lib => write!(f, "lib"),
        }
    }
}

impl std::str::FromStr for QueryProperty {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(QueryProperty::All),
            "path" | "paths" | "install-path" => Ok(QueryProperty::Path),
            "env" | "environment" | "env-vars" => Ok(QueryProperty::Env),
            "tools" | "tool" | "executables" => Ok(QueryProperty::Tools),
            "version" | "versions" | "ver" => Ok(QueryProperty::Version),
            "include" | "includes" | "include-paths" => Ok(QueryProperty::Include),
            "lib" | "libs" | "lib-paths" => Ok(QueryProperty::Lib),
            _ => Err(format!(
                "Unknown property '{}'. Valid: all, path, env, tools, version, include, lib",
                s
            )),
        }
    }
}

/// Options for querying an installation
#[derive(Debug, Clone)]
pub struct QueryOptions {
    /// Installation directory to query
    pub install_dir: PathBuf,

    /// Target architecture
    pub arch: Architecture,

    /// Which component to query
    pub component: QueryComponent,

    /// What property to retrieve
    pub property: QueryProperty,

    /// Specific MSVC version to query (None = latest installed)
    pub msvc_version: Option<String>,

    /// Specific SDK version to query (None = latest installed)
    pub sdk_version: Option<String>,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            install_dir: PathBuf::from("msvc-kit"),
            arch: Architecture::host(),
            component: QueryComponent::default(),
            property: QueryProperty::default(),
            msvc_version: None,
            sdk_version: None,
        }
    }
}

impl QueryOptions {
    /// Create a builder for query options
    pub fn builder() -> QueryOptionsBuilder {
        QueryOptionsBuilder::default()
    }
}

/// Builder for QueryOptions
#[derive(Default)]
pub struct QueryOptionsBuilder {
    options: QueryOptions,
}

impl QueryOptionsBuilder {
    /// Set installation directory
    pub fn install_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.options.install_dir = dir.into();
        self
    }

    /// Set target architecture
    pub fn arch(mut self, arch: Architecture) -> Self {
        self.options.arch = arch;
        self
    }

    /// Set which component to query
    pub fn component(mut self, component: QueryComponent) -> Self {
        self.options.component = component;
        self
    }

    /// Set what property to retrieve
    pub fn property(mut self, property: QueryProperty) -> Self {
        self.options.property = property;
        self
    }

    /// Set specific MSVC version to query
    pub fn msvc_version(mut self, version: impl Into<String>) -> Self {
        self.options.msvc_version = Some(version.into());
        self
    }

    /// Set specific SDK version to query
    pub fn sdk_version(mut self, version: impl Into<String>) -> Self {
        self.options.sdk_version = Some(version.into());
        self
    }

    /// Build the query options
    pub fn build(self) -> QueryOptions {
        self.options
    }
}

/// Result of a query operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Installation root directory
    pub install_dir: PathBuf,

    /// Target architecture
    pub arch: String,

    /// MSVC component information (if installed and queried)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msvc: Option<ComponentInfo>,

    /// Windows SDK component information (if installed and queried)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk: Option<ComponentInfo>,

    /// Merged environment variables for the full toolchain
    pub env_vars: HashMap<String, String>,

    /// Tool executable paths
    pub tools: HashMap<String, PathBuf>,
}

/// Information about a single installed component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// Component type name
    pub component_type: String,

    /// Installed version
    pub version: String,

    /// Installation path
    pub install_path: PathBuf,

    /// Include paths
    pub include_paths: Vec<PathBuf>,

    /// Library paths
    pub lib_paths: Vec<PathBuf>,

    /// Binary paths
    pub bin_paths: Vec<PathBuf>,
}

impl QueryResult {
    /// Get the path to a specific tool by name (e.g., "cl", "link", "lib", "rc")
    pub fn tool_path(&self, name: &str) -> Option<&PathBuf> {
        self.tools.get(name)
    }

    /// Get a specific environment variable value
    pub fn env_var(&self, name: &str) -> Option<&String> {
        self.env_vars.get(name)
    }

    /// Get MSVC version string
    pub fn msvc_version(&self) -> Option<&str> {
        self.msvc.as_ref().map(|m| m.version.as_str())
    }

    /// Get SDK version string
    pub fn sdk_version(&self) -> Option<&str> {
        self.sdk.as_ref().map(|s| s.version.as_str())
    }

    /// Get the MSVC installation path
    pub fn msvc_install_path(&self) -> Option<&Path> {
        self.msvc.as_ref().map(|m| m.install_path.as_path())
    }

    /// Get the SDK installation path
    pub fn sdk_install_path(&self) -> Option<&Path> {
        self.sdk.as_ref().map(|s| s.install_path.as_path())
    }

    /// Get all include paths (merged from all components)
    pub fn all_include_paths(&self) -> Vec<&PathBuf> {
        let mut paths = Vec::new();
        if let Some(ref msvc) = self.msvc {
            paths.extend(&msvc.include_paths);
        }
        if let Some(ref sdk) = self.sdk {
            paths.extend(&sdk.include_paths);
        }
        paths
    }

    /// Get all library paths (merged from all components)
    pub fn all_lib_paths(&self) -> Vec<&PathBuf> {
        let mut paths = Vec::new();
        if let Some(ref msvc) = self.msvc {
            paths.extend(&msvc.lib_paths);
        }
        if let Some(ref sdk) = self.sdk {
            paths.extend(&sdk.lib_paths);
        }
        paths
    }

    /// Export as JSON value
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// Format as a human-readable summary
    pub fn format_summary(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("Install directory: {}\n", self.install_dir.display()));
        output.push_str(&format!("Architecture: {}\n", self.arch));

        if let Some(ref msvc) = self.msvc {
            output.push_str(&format!("\nMSVC Compiler:\n"));
            output.push_str(&format!("  Version: {}\n", msvc.version));
            output.push_str(&format!("  Path: {}\n", msvc.install_path.display()));
        }

        if let Some(ref sdk) = self.sdk {
            output.push_str(&format!("\nWindows SDK:\n"));
            output.push_str(&format!("  Version: {}\n", sdk.version));
            output.push_str(&format!("  Path: {}\n", sdk.install_path.display()));
        }

        if !self.tools.is_empty() {
            output.push_str("\nTools:\n");
            let mut sorted_tools: Vec<_> = self.tools.iter().collect();
            sorted_tools.sort_by_key(|(k, _)| k.as_str());
            for (name, path) in sorted_tools {
                output.push_str(&format!("  {}: {}\n", name, path.display()));
            }
        }

        output
    }
}

/// Query an existing installation for component information
///
/// This is the primary API for inspecting installed MSVC toolchain components.
/// It discovers installed versions and builds a comprehensive result with
/// paths, environment variables, and tool locations.
///
/// # Arguments
///
/// * `options` - Query options specifying what to look for
///
/// # Returns
///
/// Returns a `QueryResult` with all discovered information
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::query::{QueryOptions, query_installation};
///
/// let options = QueryOptions::builder()
///     .install_dir("C:/msvc-kit")
///     .build();
///
/// let result = query_installation(&options)?;
/// println!("MSVC version: {:?}", result.msvc_version());
/// # Ok::<(), msvc_kit::MsvcKitError>(())
/// ```
pub fn query_installation(options: &QueryOptions) -> Result<QueryResult> {
    let install_dir = &options.install_dir;

    if !install_dir.exists() {
        return Err(MsvcKitError::InstallPath(format!(
            "Installation directory not found: {}",
            install_dir.display()
        )));
    }

    // Discover installed MSVC versions
    let msvc_info = if options.component != QueryComponent::Sdk {
        find_msvc_component(install_dir, options.arch, options.msvc_version.as_deref())?
    } else {
        None
    };

    // Discover installed SDK versions
    let sdk_info = if options.component != QueryComponent::Msvc {
        find_sdk_component(install_dir, options.arch, options.sdk_version.as_deref())?
    } else {
        None
    };

    if msvc_info.is_none() && sdk_info.is_none() {
        return Err(MsvcKitError::ComponentNotFound(format!(
            "No installed components found in: {}",
            install_dir.display()
        )));
    }

    // Build environment from discovered components
    let (env_vars, tools) = if let Some(ref msvc) = msvc_info {
        let msvc_install_info = InstallInfo {
            component_type: "msvc".to_string(),
            version: msvc.version.clone(),
            install_path: msvc.install_path.clone(),
            downloaded_files: vec![],
            arch: options.arch,
        };

        let sdk_install_info = sdk_info.as_ref().map(|sdk| InstallInfo {
            component_type: "sdk".to_string(),
            version: sdk.version.clone(),
            install_path: sdk.install_path.clone(),
            downloaded_files: vec![],
            arch: options.arch,
        });

        let env = MsvcEnvironment::from_install_info(
            &msvc_install_info,
            sdk_install_info.as_ref(),
            Architecture::host(),
        )?;

        let vars = get_env_vars(&env);
        let tools = build_tool_map(&env);

        (vars, tools)
    } else {
        (HashMap::new(), HashMap::new())
    };

    Ok(QueryResult {
        install_dir: install_dir.clone(),
        arch: options.arch.to_string(),
        msvc: msvc_info,
        sdk: sdk_info,
        env_vars,
        tools,
    })
}

/// Find MSVC component in the installation directory
fn find_msvc_component(
    install_dir: &Path,
    arch: Architecture,
    requested_version: Option<&str>,
) -> Result<Option<ComponentInfo>> {
    let msvc_versions = list_installed_msvc(install_dir);

    if msvc_versions.is_empty() {
        return Ok(None);
    }

    // Find the requested version or use latest
    let version = if let Some(req_ver) = requested_version {
        msvc_versions
            .iter()
            .find(|v| v.version.starts_with(req_ver))
            .ok_or_else(|| {
                MsvcKitError::VersionNotFound(format!("MSVC version '{}' not found", req_ver))
            })?
    } else {
        &msvc_versions[0] // Already sorted, first = latest
    };

    let install_path = version.install_path.clone().ok_or_else(|| {
        MsvcKitError::InstallPath(format!("MSVC install path not found for {}", version.version))
    })?;

    let arch_str = arch.to_string();
    let host_dir = arch.msvc_host_dir();
    let target_dir = arch.msvc_target_dir();

    Ok(Some(ComponentInfo {
        component_type: "msvc".to_string(),
        version: version.version.clone(),
        install_path: install_path.clone(),
        include_paths: vec![install_path.join("include")],
        lib_paths: vec![install_path.join("lib").join(&arch_str)],
        bin_paths: vec![install_path.join("bin").join(host_dir).join(target_dir)],
    }))
}

/// Find SDK component in the installation directory
fn find_sdk_component(
    install_dir: &Path,
    arch: Architecture,
    requested_version: Option<&str>,
) -> Result<Option<ComponentInfo>> {
    let sdk_versions = list_installed_sdk(install_dir);

    if sdk_versions.is_empty() {
        return Ok(None);
    }

    // Find the requested version or use latest
    let version = if let Some(req_ver) = requested_version {
        sdk_versions
            .iter()
            .find(|v| v.version.contains(req_ver))
            .ok_or_else(|| {
                MsvcKitError::VersionNotFound(format!("SDK version '{}' not found", req_ver))
            })?
    } else {
        &sdk_versions[0] // Already sorted, first = latest
    };

    let install_path = version.install_path.clone().ok_or_else(|| {
        MsvcKitError::InstallPath(format!("SDK install path not found for {}", version.version))
    })?;

    let arch_str = arch.to_string();
    let ver = &version.version;

    Ok(Some(ComponentInfo {
        component_type: "sdk".to_string(),
        version: ver.clone(),
        install_path: install_path.clone(),
        include_paths: vec![
            install_path.join("Include").join(ver).join("ucrt"),
            install_path.join("Include").join(ver).join("shared"),
            install_path.join("Include").join(ver).join("um"),
            install_path.join("Include").join(ver).join("winrt"),
            install_path.join("Include").join(ver).join("cppwinrt"),
        ],
        lib_paths: vec![
            install_path
                .join("Lib")
                .join(ver)
                .join("ucrt")
                .join(&arch_str),
            install_path
                .join("Lib")
                .join(ver)
                .join("um")
                .join(&arch_str),
        ],
        bin_paths: vec![install_path.join("bin").join(ver).join(&arch_str)],
    }))
}

/// Build a map of tool name -> tool path from MsvcEnvironment
fn build_tool_map(env: &MsvcEnvironment) -> HashMap<String, PathBuf> {
    let mut tools = HashMap::new();

    let tool_queries = [
        ("cl", "cl.exe"),
        ("link", "link.exe"),
        ("lib", "lib.exe"),
        ("ml64", "ml64.exe"),
        ("nmake", "nmake.exe"),
        ("rc", "rc.exe"),
        ("mt", "mt.exe"),
        ("dumpbin", "dumpbin.exe"),
        ("editbin", "editbin.exe"),
    ];

    for (name, exe) in &tool_queries {
        for bin_path in &env.bin_paths {
            let full_path = bin_path.join(exe);
            if full_path.exists() {
                tools.insert(name.to_string(), full_path);
                break;
            }
        }
    }

    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_component_parse() {
        assert_eq!(
            "all".parse::<QueryComponent>().unwrap(),
            QueryComponent::All
        );
        assert_eq!(
            "msvc".parse::<QueryComponent>().unwrap(),
            QueryComponent::Msvc
        );
        assert_eq!(
            "sdk".parse::<QueryComponent>().unwrap(),
            QueryComponent::Sdk
        );
        assert!("invalid".parse::<QueryComponent>().is_err());
    }

    #[test]
    fn test_query_property_parse() {
        assert_eq!("all".parse::<QueryProperty>().unwrap(), QueryProperty::All);
        assert_eq!(
            "path".parse::<QueryProperty>().unwrap(),
            QueryProperty::Path
        );
        assert_eq!(
            "paths".parse::<QueryProperty>().unwrap(),
            QueryProperty::Path
        );
        assert_eq!("env".parse::<QueryProperty>().unwrap(), QueryProperty::Env);
        assert_eq!(
            "tools".parse::<QueryProperty>().unwrap(),
            QueryProperty::Tools
        );
        assert_eq!(
            "version".parse::<QueryProperty>().unwrap(),
            QueryProperty::Version
        );
        assert_eq!(
            "include".parse::<QueryProperty>().unwrap(),
            QueryProperty::Include
        );
        assert_eq!("lib".parse::<QueryProperty>().unwrap(), QueryProperty::Lib);
        assert!("invalid".parse::<QueryProperty>().is_err());
    }

    #[test]
    fn test_query_component_display() {
        assert_eq!(QueryComponent::All.to_string(), "all");
        assert_eq!(QueryComponent::Msvc.to_string(), "msvc");
        assert_eq!(QueryComponent::Sdk.to_string(), "sdk");
    }

    #[test]
    fn test_query_property_display() {
        assert_eq!(QueryProperty::All.to_string(), "all");
        assert_eq!(QueryProperty::Path.to_string(), "path");
        assert_eq!(QueryProperty::Env.to_string(), "env");
        assert_eq!(QueryProperty::Tools.to_string(), "tools");
        assert_eq!(QueryProperty::Version.to_string(), "version");
        assert_eq!(QueryProperty::Include.to_string(), "include");
        assert_eq!(QueryProperty::Lib.to_string(), "lib");
    }

    #[test]
    fn test_query_options_builder() {
        let options = QueryOptions::builder()
            .install_dir("C:/msvc-kit")
            .arch(Architecture::X64)
            .component(QueryComponent::Msvc)
            .property(QueryProperty::Path)
            .msvc_version("14.44")
            .build();

        assert_eq!(options.install_dir, PathBuf::from("C:/msvc-kit"));
        assert_eq!(options.arch, Architecture::X64);
        assert_eq!(options.component, QueryComponent::Msvc);
        assert_eq!(options.property, QueryProperty::Path);
        assert_eq!(options.msvc_version, Some("14.44".to_string()));
    }

    #[test]
    fn test_query_result_accessors() {
        let result = QueryResult {
            install_dir: PathBuf::from("C:/msvc-kit"),
            arch: "x64".to_string(),
            msvc: Some(ComponentInfo {
                component_type: "msvc".to_string(),
                version: "14.44.34823".to_string(),
                install_path: PathBuf::from("C:/msvc-kit/VC/Tools/MSVC/14.44.34823"),
                include_paths: vec![PathBuf::from(
                    "C:/msvc-kit/VC/Tools/MSVC/14.44.34823/include",
                )],
                lib_paths: vec![PathBuf::from(
                    "C:/msvc-kit/VC/Tools/MSVC/14.44.34823/lib/x64",
                )],
                bin_paths: vec![PathBuf::from(
                    "C:/msvc-kit/VC/Tools/MSVC/14.44.34823/bin/Hostx64/x64",
                )],
            }),
            sdk: Some(ComponentInfo {
                component_type: "sdk".to_string(),
                version: "10.0.26100.0".to_string(),
                install_path: PathBuf::from("C:/msvc-kit/Windows Kits/10"),
                include_paths: vec![PathBuf::from(
                    "C:/msvc-kit/Windows Kits/10/Include/10.0.26100.0/ucrt",
                )],
                lib_paths: vec![PathBuf::from(
                    "C:/msvc-kit/Windows Kits/10/Lib/10.0.26100.0/ucrt/x64",
                )],
                bin_paths: vec![PathBuf::from(
                    "C:/msvc-kit/Windows Kits/10/bin/10.0.26100.0/x64",
                )],
            }),
            env_vars: {
                let mut m = HashMap::new();
                m.insert("INCLUDE".to_string(), "C:/include".to_string());
                m.insert("LIB".to_string(), "C:/lib".to_string());
                m
            },
            tools: {
                let mut m = HashMap::new();
                m.insert(
                    "cl".to_string(),
                    PathBuf::from("C:/msvc-kit/VC/Tools/MSVC/14.44.34823/bin/Hostx64/x64/cl.exe"),
                );
                m
            },
        };

        assert_eq!(result.msvc_version(), Some("14.44.34823"));
        assert_eq!(result.sdk_version(), Some("10.0.26100.0"));
        assert!(result.tool_path("cl").is_some());
        assert!(result.tool_path("nonexistent").is_none());
        assert_eq!(result.env_var("INCLUDE"), Some(&"C:/include".to_string()));
        assert_eq!(result.all_include_paths().len(), 2);
        assert_eq!(result.all_lib_paths().len(), 2);
    }

    #[test]
    fn test_query_result_json() {
        let result = QueryResult {
            install_dir: PathBuf::from("C:/test"),
            arch: "x64".to_string(),
            msvc: None,
            sdk: None,
            env_vars: HashMap::new(),
            tools: HashMap::new(),
        };

        let json = result.to_json();
        assert!(json.is_object());
        assert_eq!(json["arch"], "x64");
    }

    #[test]
    fn test_query_result_format_summary() {
        let result = QueryResult {
            install_dir: PathBuf::from("C:/msvc-kit"),
            arch: "x64".to_string(),
            msvc: Some(ComponentInfo {
                component_type: "msvc".to_string(),
                version: "14.44.34823".to_string(),
                install_path: PathBuf::from("C:/msvc-kit/VC/Tools/MSVC/14.44.34823"),
                include_paths: vec![],
                lib_paths: vec![],
                bin_paths: vec![],
            }),
            sdk: None,
            env_vars: HashMap::new(),
            tools: HashMap::new(),
        };

        let summary = result.format_summary();
        assert!(summary.contains("14.44.34823"));
        assert!(summary.contains("x64"));
    }

    #[test]
    fn test_query_nonexistent_dir() {
        let options = QueryOptions::builder()
            .install_dir("C:/nonexistent/path/that/does/not/exist")
            .build();

        let result = query_installation(&options);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_options_default() {
        let options = QueryOptions::default();
        assert_eq!(options.component, QueryComponent::All);
        assert_eq!(options.property, QueryProperty::All);
        assert!(options.msvc_version.is_none());
        assert!(options.sdk_version.is_none());
    }
}
