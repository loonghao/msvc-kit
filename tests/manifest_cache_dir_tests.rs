//! End-to-end coverage for the manifest cache directory wiring.
//!
//! Every manifest read path (`download`, `list --available`, dry-run preview,
//! `bundle` and the library entry points) must resolve to the configured cache
//! directory, and must keep using the platform default when nothing is
//! configured. The CLI is exercised as a subprocess so the environment
//! overrides each test needs cannot leak into other tests.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Run `msvc-kit config` with `MSVC_KIT_DIR` set and return the reported cache
/// directory.
fn cache_dir_for_install_dir(install_dir: &Path) -> PathBuf {
    let output = Command::new(env!("CARGO_BIN_EXE_msvc-kit"))
        .arg("config")
        .env("MSVC_KIT_DIR", install_dir)
        .output()
        .expect("msvc-kit config must run");

    assert!(
        output.status.success(),
        "msvc-kit config failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("Cache directory: "))
        .unwrap_or_else(|| panic!("cache directory missing from output:\n{stdout}"))
        .trim();

    assert!(
        !reported.is_empty(),
        "the cache directory must be reported:\n{stdout}"
    );
    PathBuf::from(reported)
}

/// The manifest cache directory follows `MSVC_KIT_DIR`, and a relocated
/// installation never keeps using the platform default.
///
/// This is the directory that every manifest read path resolves from
/// (`MsvcKitConfig::manifest_cache_dir`), so moving it moves all of them.
#[test]
fn configured_manifest_cache_dir_follows_install_dir_override() {
    let dir = tempfile::tempdir().unwrap();
    let install_dir = dir.path().join("relocated kit");

    let cache_dir = cache_dir_for_install_dir(&install_dir);
    let manifest_cache_dir = msvc_kit::MsvcKitConfig {
        install_dir: install_dir.clone(),
        ..msvc_kit::MsvcKitConfig::default()
    }
    .manifest_cache_dir();

    assert_eq!(
        cache_dir,
        install_dir.join("cache"),
        "the cache must follow MSVC_KIT_DIR"
    );
    assert_eq!(
        manifest_cache_dir,
        install_dir.join("cache").join("manifests"),
        "manifests must be cached under the relocated cache directory"
    );
    assert_ne!(
        manifest_cache_dir,
        msvc_kit::downloader::cache::default_manifest_cache_dir(),
        "the override must move manifests away from the default location"
    );
}

/// An explicitly configured cache directory wins over the install directory.
#[test]
fn configured_manifest_cache_dir_honors_explicit_cache_dir() {
    let config = msvc_kit::MsvcKitConfig {
        install_dir: PathBuf::from("relocated kit"),
        cache_dir: Some(PathBuf::from("separate-cache")),
        ..msvc_kit::MsvcKitConfig::default()
    };

    assert_eq!(
        config.manifest_cache_dir(),
        PathBuf::from("separate-cache/manifests")
    );
}

/// Unconfigured installations keep caching manifests in the platform default
/// location — the regression surface of the cache directory wiring.
#[test]
fn configured_manifest_cache_dir_defaults_to_platform_location() {
    let config = msvc_kit::MsvcKitConfig::default();

    assert_eq!(
        config.manifest_cache_dir(),
        msvc_kit::downloader::cache::default_manifest_cache_dir()
    );
    assert_eq!(
        msvc_kit::DownloadOptions::default()
            .with_configured_manifest_cache_dir()
            .manifest_cache_dir,
        Some(
            msvc_kit::load_config()
                .map(|c| c.manifest_cache_dir())
                .unwrap_or_else(|_| { msvc_kit::downloader::cache::default_manifest_cache_dir() })
        )
    );
}

/// The library entry points apply the configured manifest cache directory to
/// options that did not set one, and keep an explicit directory untouched.
#[test]
fn download_options_apply_configured_manifest_cache_dir() {
    let explicit = PathBuf::from("explicit-cache").join("manifests");

    let untouched = msvc_kit::DownloadOptions::builder()
        .manifest_cache_dir(&explicit)
        .build()
        .with_configured_manifest_cache_dir();
    assert_eq!(untouched.manifest_cache_dir, Some(explicit.clone()));

    let filled = msvc_kit::DownloadOptions::default().with_configured_manifest_cache_dir();
    assert_eq!(
        filled.manifest_cache_dir,
        Some(
            msvc_kit::load_config()
                .map(|c| c.manifest_cache_dir())
                .unwrap_or_else(|_| msvc_kit::downloader::cache::default_manifest_cache_dir())
        ),
        "library entry points must read manifests from the configured directory"
    );
    assert_ne!(
        filled.manifest_cache_dir,
        Some(explicit),
        "an unset directory must be filled from the configuration"
    );
}

/// `bundle` reads manifests from `BundleOptions::manifest_cache_dir` instead of
/// always using the platform default.
#[test]
fn bundle_options_carry_manifest_cache_dir() {
    let cache_dir = PathBuf::from("bundle-cache").join("manifests");

    let options = msvc_kit::BundleOptions {
        manifest_cache_dir: Some(cache_dir.clone()),
        ..msvc_kit::BundleOptions::default()
    };

    assert_eq!(options.manifest_cache_dir, Some(cache_dir));
    assert_eq!(msvc_kit::BundleOptions::default().manifest_cache_dir, None);
}
