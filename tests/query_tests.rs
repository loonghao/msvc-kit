//! Query module integration tests

use msvc_kit::query::{
    query_installation, ComponentInfo, QueryComponent, QueryOptions, QueryProperty, QueryResult,
};
use msvc_kit::version::Architecture;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

// ============================================================================
// QueryComponent Tests
// ============================================================================

#[test]
fn test_query_component_parse_all() {
    assert_eq!(
        "all".parse::<QueryComponent>().unwrap(),
        QueryComponent::All
    );
}

#[test]
fn test_query_component_parse_msvc() {
    assert_eq!(
        "msvc".parse::<QueryComponent>().unwrap(),
        QueryComponent::Msvc
    );
    assert_eq!(
        "MSVC".parse::<QueryComponent>().unwrap(),
        QueryComponent::Msvc
    );
}

#[test]
fn test_query_component_parse_sdk() {
    assert_eq!(
        "sdk".parse::<QueryComponent>().unwrap(),
        QueryComponent::Sdk
    );
    assert_eq!(
        "winsdk".parse::<QueryComponent>().unwrap(),
        QueryComponent::Sdk
    );
}

#[test]
fn test_query_component_parse_invalid() {
    assert!("invalid".parse::<QueryComponent>().is_err());
    assert!("".parse::<QueryComponent>().is_err());
}

#[test]
fn test_query_component_display() {
    assert_eq!(QueryComponent::All.to_string(), "all");
    assert_eq!(QueryComponent::Msvc.to_string(), "msvc");
    assert_eq!(QueryComponent::Sdk.to_string(), "sdk");
}

#[test]
fn test_query_component_default() {
    assert_eq!(QueryComponent::default(), QueryComponent::All);
}

// ============================================================================
// QueryProperty Tests
// ============================================================================

#[test]
fn test_query_property_parse_all() {
    assert_eq!("all".parse::<QueryProperty>().unwrap(), QueryProperty::All);
}

#[test]
fn test_query_property_parse_path_variants() {
    assert_eq!(
        "path".parse::<QueryProperty>().unwrap(),
        QueryProperty::Path
    );
    assert_eq!(
        "paths".parse::<QueryProperty>().unwrap(),
        QueryProperty::Path
    );
    assert_eq!(
        "install-path".parse::<QueryProperty>().unwrap(),
        QueryProperty::Path
    );
}

#[test]
fn test_query_property_parse_env_variants() {
    assert_eq!("env".parse::<QueryProperty>().unwrap(), QueryProperty::Env);
    assert_eq!(
        "environment".parse::<QueryProperty>().unwrap(),
        QueryProperty::Env
    );
    assert_eq!(
        "env-vars".parse::<QueryProperty>().unwrap(),
        QueryProperty::Env
    );
}

#[test]
fn test_query_property_parse_tools_variants() {
    assert_eq!(
        "tools".parse::<QueryProperty>().unwrap(),
        QueryProperty::Tools
    );
    assert_eq!(
        "tool".parse::<QueryProperty>().unwrap(),
        QueryProperty::Tools
    );
    assert_eq!(
        "executables".parse::<QueryProperty>().unwrap(),
        QueryProperty::Tools
    );
}

#[test]
fn test_query_property_parse_version_variants() {
    assert_eq!(
        "version".parse::<QueryProperty>().unwrap(),
        QueryProperty::Version
    );
    assert_eq!(
        "versions".parse::<QueryProperty>().unwrap(),
        QueryProperty::Version
    );
    assert_eq!(
        "ver".parse::<QueryProperty>().unwrap(),
        QueryProperty::Version
    );
}

#[test]
fn test_query_property_parse_include() {
    assert_eq!(
        "include".parse::<QueryProperty>().unwrap(),
        QueryProperty::Include
    );
    assert_eq!(
        "includes".parse::<QueryProperty>().unwrap(),
        QueryProperty::Include
    );
    assert_eq!(
        "include-paths".parse::<QueryProperty>().unwrap(),
        QueryProperty::Include
    );
}

#[test]
fn test_query_property_parse_lib() {
    assert_eq!("lib".parse::<QueryProperty>().unwrap(), QueryProperty::Lib);
    assert_eq!(
        "libs".parse::<QueryProperty>().unwrap(),
        QueryProperty::Lib
    );
    assert_eq!(
        "lib-paths".parse::<QueryProperty>().unwrap(),
        QueryProperty::Lib
    );
}

#[test]
fn test_query_property_parse_invalid() {
    assert!("invalid".parse::<QueryProperty>().is_err());
    assert!("".parse::<QueryProperty>().is_err());
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
fn test_query_property_default() {
    assert_eq!(QueryProperty::default(), QueryProperty::All);
}

// ============================================================================
// QueryOptions Builder Tests
// ============================================================================

#[test]
fn test_query_options_default() {
    let options = QueryOptions::default();
    assert_eq!(options.install_dir, PathBuf::from("msvc-kit"));
    assert_eq!(options.component, QueryComponent::All);
    assert_eq!(options.property, QueryProperty::All);
    assert!(options.msvc_version.is_none());
    assert!(options.sdk_version.is_none());
}

#[test]
fn test_query_options_builder_install_dir() {
    let options = QueryOptions::builder()
        .install_dir("C:/custom/path")
        .build();
    assert_eq!(options.install_dir, PathBuf::from("C:/custom/path"));
}

#[test]
fn test_query_options_builder_arch() {
    let options = QueryOptions::builder().arch(Architecture::Arm64).build();
    assert_eq!(options.arch, Architecture::Arm64);
}

#[test]
fn test_query_options_builder_component() {
    let options = QueryOptions::builder()
        .component(QueryComponent::Msvc)
        .build();
    assert_eq!(options.component, QueryComponent::Msvc);
}

#[test]
fn test_query_options_builder_property() {
    let options = QueryOptions::builder()
        .property(QueryProperty::Tools)
        .build();
    assert_eq!(options.property, QueryProperty::Tools);
}

#[test]
fn test_query_options_builder_versions() {
    let options = QueryOptions::builder()
        .msvc_version("14.44")
        .sdk_version("10.0.26100.0")
        .build();
    assert_eq!(options.msvc_version, Some("14.44".to_string()));
    assert_eq!(options.sdk_version, Some("10.0.26100.0".to_string()));
}

#[test]
fn test_query_options_builder_chain() {
    let options = QueryOptions::builder()
        .install_dir("C:/toolchain")
        .arch(Architecture::X86)
        .component(QueryComponent::Sdk)
        .property(QueryProperty::Env)
        .sdk_version("10.0.22621.0")
        .build();

    assert_eq!(options.install_dir, PathBuf::from("C:/toolchain"));
    assert_eq!(options.arch, Architecture::X86);
    assert_eq!(options.component, QueryComponent::Sdk);
    assert_eq!(options.property, QueryProperty::Env);
    assert_eq!(options.sdk_version, Some("10.0.22621.0".to_string()));
}

// ============================================================================
// QueryResult Tests
// ============================================================================

fn create_test_result() -> QueryResult {
    QueryResult {
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
            include_paths: vec![
                PathBuf::from("C:/msvc-kit/Windows Kits/10/Include/10.0.26100.0/ucrt"),
                PathBuf::from("C:/msvc-kit/Windows Kits/10/Include/10.0.26100.0/shared"),
            ],
            lib_paths: vec![PathBuf::from(
                "C:/msvc-kit/Windows Kits/10/Lib/10.0.26100.0/ucrt/x64",
            )],
            bin_paths: vec![],
        }),
        env_vars: {
            let mut m = HashMap::new();
            m.insert("INCLUDE".to_string(), "C:/include".to_string());
            m.insert("LIB".to_string(), "C:/lib".to_string());
            m.insert("PATH".to_string(), "C:/bin".to_string());
            m
        },
        tools: {
            let mut m = HashMap::new();
            m.insert(
                "cl".to_string(),
                PathBuf::from("C:/msvc-kit/VC/Tools/MSVC/14.44.34823/bin/Hostx64/x64/cl.exe"),
            );
            m.insert(
                "link".to_string(),
                PathBuf::from("C:/msvc-kit/VC/Tools/MSVC/14.44.34823/bin/Hostx64/x64/link.exe"),
            );
            m
        },
    }
}

#[test]
fn test_query_result_msvc_version() {
    let result = create_test_result();
    assert_eq!(result.msvc_version(), Some("14.44.34823"));
}

#[test]
fn test_query_result_sdk_version() {
    let result = create_test_result();
    assert_eq!(result.sdk_version(), Some("10.0.26100.0"));
}

#[test]
fn test_query_result_tool_path() {
    let result = create_test_result();
    assert!(result.tool_path("cl").is_some());
    assert!(result.tool_path("link").is_some());
    assert!(result.tool_path("nonexistent").is_none());
}

#[test]
fn test_query_result_env_var() {
    let result = create_test_result();
    assert_eq!(result.env_var("INCLUDE"), Some(&"C:/include".to_string()));
    assert_eq!(result.env_var("LIB"), Some(&"C:/lib".to_string()));
    assert!(result.env_var("NONEXISTENT").is_none());
}

#[test]
fn test_query_result_all_include_paths() {
    let result = create_test_result();
    let paths = result.all_include_paths();
    assert_eq!(paths.len(), 3); // 1 MSVC + 2 SDK
}

#[test]
fn test_query_result_all_lib_paths() {
    let result = create_test_result();
    let paths = result.all_lib_paths();
    assert_eq!(paths.len(), 2); // 1 MSVC + 1 SDK
}

#[test]
fn test_query_result_msvc_install_path() {
    let result = create_test_result();
    assert_eq!(
        result.msvc_install_path(),
        Some(PathBuf::from("C:/msvc-kit/VC/Tools/MSVC/14.44.34823").as_path())
    );
}

#[test]
fn test_query_result_sdk_install_path() {
    let result = create_test_result();
    assert_eq!(
        result.sdk_install_path(),
        Some(PathBuf::from("C:/msvc-kit/Windows Kits/10").as_path())
    );
}

#[test]
fn test_query_result_to_json() {
    let result = create_test_result();
    let json = result.to_json();
    assert!(json.is_object());
    assert_eq!(json["arch"], "x64");
    assert!(json["msvc"].is_object());
    assert!(json["sdk"].is_object());
    assert!(json["env_vars"].is_object());
    assert!(json["tools"].is_object());
}

#[test]
fn test_query_result_format_summary() {
    let result = create_test_result();
    let summary = result.format_summary();
    assert!(summary.contains("14.44.34823"));
    assert!(summary.contains("10.0.26100.0"));
    assert!(summary.contains("x64"));
    assert!(summary.contains("MSVC"));
    assert!(summary.contains("SDK"));
    assert!(summary.contains("cl"));
    assert!(summary.contains("link"));
}

#[test]
fn test_query_result_no_msvc() {
    let result = QueryResult {
        install_dir: PathBuf::from("C:/test"),
        arch: "x64".to_string(),
        msvc: None,
        sdk: Some(ComponentInfo {
            component_type: "sdk".to_string(),
            version: "10.0.26100.0".to_string(),
            install_path: PathBuf::from("C:/test/Windows Kits/10"),
            include_paths: vec![],
            lib_paths: vec![],
            bin_paths: vec![],
        }),
        env_vars: HashMap::new(),
        tools: HashMap::new(),
    };

    assert!(result.msvc_version().is_none());
    assert!(result.msvc_install_path().is_none());
    assert_eq!(result.sdk_version(), Some("10.0.26100.0"));
    assert_eq!(result.all_include_paths().len(), 0);
}

#[test]
fn test_query_result_no_sdk() {
    let result = QueryResult {
        install_dir: PathBuf::from("C:/test"),
        arch: "x64".to_string(),
        msvc: Some(ComponentInfo {
            component_type: "msvc".to_string(),
            version: "14.44.34823".to_string(),
            install_path: PathBuf::from("C:/test/VC/Tools/MSVC/14.44.34823"),
            include_paths: vec![PathBuf::from("C:/include")],
            lib_paths: vec![PathBuf::from("C:/lib")],
            bin_paths: vec![],
        }),
        sdk: None,
        env_vars: HashMap::new(),
        tools: HashMap::new(),
    };

    assert!(result.sdk_version().is_none());
    assert!(result.sdk_install_path().is_none());
    assert_eq!(result.msvc_version(), Some("14.44.34823"));
    assert_eq!(result.all_include_paths().len(), 1);
    assert_eq!(result.all_lib_paths().len(), 1);
}

// ============================================================================
// ComponentInfo Tests
// ============================================================================

#[test]
fn test_component_info_serialization() {
    let info = ComponentInfo {
        component_type: "msvc".to_string(),
        version: "14.44.34823".to_string(),
        install_path: PathBuf::from("C:/test"),
        include_paths: vec![PathBuf::from("C:/test/include")],
        lib_paths: vec![PathBuf::from("C:/test/lib")],
        bin_paths: vec![PathBuf::from("C:/test/bin")],
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("14.44.34823"));
    assert!(json.contains("msvc"));

    let deserialized: ComponentInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.version, "14.44.34823");
    assert_eq!(deserialized.component_type, "msvc");
}

// ============================================================================
// query_installation Tests
// ============================================================================

#[test]
fn test_query_nonexistent_directory() {
    let options = QueryOptions::builder()
        .install_dir("C:/this/path/does/not/exist/at/all")
        .build();

    let result = query_installation(&options);
    assert!(result.is_err());
}

#[test]
fn test_query_empty_directory() {
    let temp = TempDir::new().unwrap();

    let options = QueryOptions::builder()
        .install_dir(temp.path())
        .build();

    let result = query_installation(&options);
    assert!(result.is_err());
}

#[test]
fn test_query_with_mock_msvc_installation() {
    let temp = TempDir::new().unwrap();

    // Create mock MSVC directory structure
    let msvc_version_dir = temp
        .path()
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join("14.44.34823");
    std::fs::create_dir_all(msvc_version_dir.join("include")).unwrap();
    std::fs::create_dir_all(msvc_version_dir.join("lib").join("x64")).unwrap();
    std::fs::create_dir_all(
        msvc_version_dir
            .join("bin")
            .join("Hostx64")
            .join("x64"),
    )
    .unwrap();

    let options = QueryOptions::builder()
        .install_dir(temp.path())
        .arch(Architecture::X64)
        .build();

    let result = query_installation(&options).unwrap();
    assert!(result.msvc.is_some());
    assert_eq!(result.msvc_version(), Some("14.44.34823"));
    assert!(result.sdk.is_none());
}

#[test]
fn test_query_with_mock_sdk_installation() {
    let temp = TempDir::new().unwrap();

    // Create mock MSVC directory structure (required for env)
    let msvc_version_dir = temp
        .path()
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join("14.44.34823");
    std::fs::create_dir_all(msvc_version_dir.join("include")).unwrap();
    std::fs::create_dir_all(msvc_version_dir.join("lib").join("x64")).unwrap();
    std::fs::create_dir_all(
        msvc_version_dir
            .join("bin")
            .join("Hostx64")
            .join("x64"),
    )
    .unwrap();

    // Create mock SDK directory structure
    let sdk_include = temp
        .path()
        .join("Windows Kits")
        .join("10")
        .join("Include")
        .join("10.0.26100.0");
    std::fs::create_dir_all(sdk_include.join("ucrt")).unwrap();
    std::fs::create_dir_all(sdk_include.join("shared")).unwrap();
    std::fs::create_dir_all(sdk_include.join("um")).unwrap();

    let options = QueryOptions::builder()
        .install_dir(temp.path())
        .arch(Architecture::X64)
        .build();

    let result = query_installation(&options).unwrap();
    assert!(result.msvc.is_some());
    assert!(result.sdk.is_some());
    assert_eq!(result.sdk_version(), Some("10.0.26100.0"));
}

#[test]
fn test_query_component_filter_msvc_only() {
    let temp = TempDir::new().unwrap();

    // Create both MSVC and SDK structures
    let msvc_dir = temp
        .path()
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join("14.44.34823");
    std::fs::create_dir_all(&msvc_dir).unwrap();

    let sdk_dir = temp
        .path()
        .join("Windows Kits")
        .join("10")
        .join("Include")
        .join("10.0.26100.0");
    std::fs::create_dir_all(&sdk_dir).unwrap();

    let options = QueryOptions::builder()
        .install_dir(temp.path())
        .component(QueryComponent::Msvc)
        .build();

    let result = query_installation(&options).unwrap();
    assert!(result.msvc.is_some());
    assert!(result.sdk.is_none()); // Should be filtered out
}

#[test]
fn test_query_component_filter_sdk_only() {
    let temp = TempDir::new().unwrap();

    // Create both structures but query only SDK
    let msvc_dir = temp
        .path()
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join("14.44.34823");
    std::fs::create_dir_all(&msvc_dir).unwrap();

    let sdk_dir = temp
        .path()
        .join("Windows Kits")
        .join("10")
        .join("Include")
        .join("10.0.26100.0");
    std::fs::create_dir_all(&sdk_dir).unwrap();

    let options = QueryOptions::builder()
        .install_dir(temp.path())
        .component(QueryComponent::Sdk)
        .build();

    // SDK-only query doesn't produce env_vars (needs MSVC for that)
    let result = query_installation(&options).unwrap();
    assert!(result.msvc.is_none()); // Should be filtered out
    assert!(result.sdk.is_some());
}

#[test]
fn test_query_specific_msvc_version() {
    let temp = TempDir::new().unwrap();

    // Create two MSVC versions
    let v1 = temp
        .path()
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join("14.43.12345");
    let v2 = temp
        .path()
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join("14.44.34823");
    std::fs::create_dir_all(&v1).unwrap();
    std::fs::create_dir_all(&v2).unwrap();

    // Query for the older version specifically
    let options = QueryOptions::builder()
        .install_dir(temp.path())
        .msvc_version("14.43")
        .build();

    let result = query_installation(&options).unwrap();
    assert_eq!(result.msvc_version(), Some("14.43.12345"));
}

#[test]
fn test_query_nonexistent_version() {
    let temp = TempDir::new().unwrap();

    let msvc_dir = temp
        .path()
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join("14.44.34823");
    std::fs::create_dir_all(&msvc_dir).unwrap();

    let options = QueryOptions::builder()
        .install_dir(temp.path())
        .msvc_version("14.99")
        .build();

    let result = query_installation(&options);
    assert!(result.is_err());
}

#[test]
fn test_query_result_env_vars_populated() {
    let temp = TempDir::new().unwrap();

    // Full mock setup
    let msvc_dir = temp
        .path()
        .join("VC")
        .join("Tools")
        .join("MSVC")
        .join("14.44.34823");
    std::fs::create_dir_all(msvc_dir.join("include")).unwrap();
    std::fs::create_dir_all(msvc_dir.join("lib").join("x64")).unwrap();
    std::fs::create_dir_all(msvc_dir.join("bin").join("Hostx64").join("x64")).unwrap();

    let sdk_dir = temp
        .path()
        .join("Windows Kits")
        .join("10")
        .join("Include")
        .join("10.0.26100.0");
    std::fs::create_dir_all(sdk_dir.join("ucrt")).unwrap();

    let options = QueryOptions::builder()
        .install_dir(temp.path())
        .arch(Architecture::X64)
        .build();

    let result = query_installation(&options).unwrap();

    // Should have standard env vars
    assert!(result.env_vars.contains_key("INCLUDE"));
    assert!(result.env_vars.contains_key("LIB"));
    assert!(result.env_vars.contains_key("PATH"));
    assert!(result.env_vars.contains_key("VCToolsVersion"));
    assert!(result.env_vars.contains_key("VCINSTALLDIR"));
}

// ============================================================================
// JSON Serialization Round-trip Tests
// ============================================================================

#[test]
fn test_query_result_json_roundtrip() {
    let result = create_test_result();
    let json_str = serde_json::to_string(&result).unwrap();
    let deserialized: QueryResult = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.install_dir, result.install_dir);
    assert_eq!(deserialized.arch, result.arch);
    assert_eq!(deserialized.msvc_version(), result.msvc_version());
    assert_eq!(deserialized.sdk_version(), result.sdk_version());
    assert_eq!(deserialized.env_vars.len(), result.env_vars.len());
    assert_eq!(deserialized.tools.len(), result.tools.len());
}

#[test]
fn test_query_result_json_skip_serializing_none() {
    let result = QueryResult {
        install_dir: PathBuf::from("C:/test"),
        arch: "x64".to_string(),
        msvc: None,
        sdk: None,
        env_vars: HashMap::new(),
        tools: HashMap::new(),
    };

    let json_str = serde_json::to_string(&result).unwrap();
    // None fields should be skipped
    assert!(!json_str.contains("\"msvc\""));
    assert!(!json_str.contains("\"sdk\""));
}
