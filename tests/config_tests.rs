//! Config and error tests

use msvc_kit::config::MsvcKitConfig;
use msvc_kit::error::MsvcKitError;
use msvc_kit::version::Architecture;
use std::path::PathBuf;

// ============================================================================
// Config Tests
// ============================================================================

#[test]
fn test_default_config() {
    let config = MsvcKitConfig::default();
    assert!(config.verify_hashes);
    assert_eq!(config.parallel_downloads, 4);
    assert_eq!(config.default_arch, Architecture::X64);
    assert!(config.default_msvc_version.is_none());
    assert!(config.default_sdk_version.is_none());
}

#[test]
fn test_config_serde() {
    let config = MsvcKitConfig {
        install_dir: PathBuf::from("C:/test"),
        default_msvc_version: Some("14.44".to_string()),
        default_sdk_version: Some("10.0.26100.0".to_string()),
        default_arch: Architecture::X86,
        verify_hashes: false,
        parallel_downloads: 8,
        cache_dir: Some(PathBuf::from("C:/cache")),
    };

    let toml_str = toml::to_string(&config).unwrap();
    let parsed: MsvcKitConfig = toml::from_str(&toml_str).unwrap();

    assert_eq!(parsed.install_dir, config.install_dir);
    assert_eq!(parsed.default_msvc_version, config.default_msvc_version);
    assert_eq!(parsed.default_sdk_version, config.default_sdk_version);
    assert_eq!(parsed.default_arch, config.default_arch);
    assert_eq!(parsed.verify_hashes, config.verify_hashes);
    assert_eq!(parsed.parallel_downloads, config.parallel_downloads);
}

#[test]
fn test_get_msvc_install_dir() {
    let config = MsvcKitConfig {
        install_dir: PathBuf::from("C:/msvc-kit"),
        ..Default::default()
    };

    let dir = msvc_kit::config::get_msvc_install_dir(&config, "14.44.33807");
    assert!(dir.to_string_lossy().contains("MSVC"));
    assert!(dir.to_string_lossy().contains("14.44.33807"));
}

#[test]
fn test_get_sdk_install_dir() {
    let config = MsvcKitConfig {
        install_dir: PathBuf::from("C:/msvc-kit"),
        ..Default::default()
    };

    let dir = msvc_kit::config::get_sdk_install_dir(&config, "10.0.26100.0");
    assert!(dir.to_string_lossy().contains("Windows Kits"));
    assert!(dir.to_string_lossy().contains("10.0.26100.0"));
}

// ============================================================================
// Error Tests
// ============================================================================

#[test]
fn test_error_from_string() {
    let error: MsvcKitError = "test error".into();
    assert!(matches!(error, MsvcKitError::Other(_)));
    assert!(error.to_string().contains("test error"));
}

#[test]
fn test_error_from_owned_string() {
    let error: MsvcKitError = String::from("owned error").into();
    assert!(matches!(error, MsvcKitError::Other(_)));
    assert!(error.to_string().contains("owned error"));
}

#[test]
fn test_error_version_not_found() {
    let error = MsvcKitError::VersionNotFound("14.44".to_string());
    assert!(error.to_string().contains("14.44"));
    assert!(error.to_string().contains("not found"));
}

#[test]
fn test_error_component_not_found() {
    let error = MsvcKitError::ComponentNotFound("cl.exe".to_string());
    assert!(error.to_string().contains("cl.exe"));
}

#[test]
fn test_error_hash_mismatch() {
    let error = MsvcKitError::HashMismatch {
        file: "test.vsix".to_string(),
        expected: "abc123".to_string(),
        actual: "def456".to_string(),
    };
    assert!(error.to_string().contains("test.vsix"));
    assert!(error.to_string().contains("abc123"));
    assert!(error.to_string().contains("def456"));
}

#[test]
fn test_error_unsupported_platform() {
    let error = MsvcKitError::UnsupportedPlatform("Linux".to_string());
    assert!(error.to_string().contains("Linux"));
}

#[test]
fn test_error_cancelled() {
    let error = MsvcKitError::Cancelled;
    assert!(error.to_string().contains("cancelled"));
}

#[test]
fn test_error_config() {
    let error = MsvcKitError::Config("invalid config".to_string());
    assert!(error.to_string().contains("invalid config"));
}

#[test]
fn test_error_env_setup() {
    let error = MsvcKitError::EnvSetup("failed to set PATH".to_string());
    assert!(error.to_string().contains("failed to set PATH"));
}

#[test]
fn test_error_database() {
    let error = MsvcKitError::Database("connection failed".to_string());
    assert!(error.to_string().contains("connection failed"));
}

#[test]
fn test_error_cab() {
    let error = MsvcKitError::Cab("invalid cab file".to_string());
    assert!(error.to_string().contains("invalid cab file"));
}

#[test]
fn test_error_install_path() {
    let error = MsvcKitError::InstallPath("path not found".to_string());
    assert!(error.to_string().contains("path not found"));
}

#[test]
fn test_error_debug_impl() {
    let error = MsvcKitError::Other("debug test".to_string());
    let debug_str = format!("{:?}", error);
    assert!(debug_str.contains("Other"));
}

#[test]
fn test_error_from_io_error() {
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let error: MsvcKitError = io_error.into();
    assert!(matches!(error, MsvcKitError::Io(_)));
}

#[test]
fn test_error_serialization() {
    let error = MsvcKitError::Config("test config error".to_string());
    assert!(error.to_string().contains("test config error"));
}

// ============================================================================
// Constants Tests
// ============================================================================

#[test]
fn test_user_agent() {
    use msvc_kit::constants::USER_AGENT;
    assert!(USER_AGENT.contains("msvc-kit"));
}

#[test]
fn test_vs_channel_url() {
    use msvc_kit::constants::VS_CHANNEL_URL;
    assert!(VS_CHANNEL_URL.starts_with("https://"));
    assert!(VS_CHANNEL_URL.contains("vs"));
}

#[test]
fn test_download_constants() {
    use msvc_kit::constants::download;
    assert_eq!(download::MAX_RETRIES, 4);
    assert_eq!(download::DEFAULT_PARALLEL_DOWNLOADS, 4);
    assert_eq!(download::MIN_CONCURRENCY, 2);
    let low = download::LOW_THROUGHPUT_MBPS;
    let high = download::HIGH_THROUGHPUT_MBPS;
    assert!(
        low < high,
        "LOW_THROUGHPUT_MBPS should be less than HIGH_THROUGHPUT_MBPS"
    );
}

#[test]
fn test_progress_constants() {
    use msvc_kit::constants::progress;
    assert_eq!(progress::SPINNER_TICK_MS, 80);
    assert_eq!(progress::PROGRESS_TICK_MS, 120);
    assert_eq!(progress::UPDATE_INTERVAL.as_millis(), 200);
}

#[test]
fn test_hash_constants() {
    use msvc_kit::constants::hash;
    assert_eq!(hash::HASH_BUFFER_SIZE, 1024 * 1024);
}

#[test]
fn test_extraction_constants() {
    use msvc_kit::constants::extraction;
    assert_eq!(extraction::EXTRACT_BUFFER_SIZE, 128 * 1024);
}
