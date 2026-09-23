# DownloadOptions

Configuration for download operations.

## Definition

```rust
pub struct DownloadOptions {
    /// Target directory for installation
    pub target_dir: PathBuf,
    
    /// MSVC version to download (None = latest)
    pub msvc_version: Option<String>,
    
    /// SDK version to download (None = latest)
    pub sdk_version: Option<String>,
    
    /// Target architecture
    pub arch: Architecture,
    
    /// Host architecture (None = auto-detect)
    pub host_arch: Option<Architecture>,
    
    /// Verify file hashes
    pub verify_hashes: bool,
    
    /// Number of parallel downloads
    pub parallel_downloads: usize,
    
    /// Custom HTTP client (None = create default)
    pub http_client: Option<reqwest::Client>,
    
    /// Custom progress handler (None = use default indicatif)
    pub progress_handler: Option<BoxedProgressHandler>,
    
    /// Custom cache manager (None = use default file system cache)
    pub cache_manager: Option<BoxedCacheManager>,
    
    /// Visual Studio channel used for manifest discovery (None = auto)
    ///
    /// Accepts a major version (`17`, `v17`), a release year (`2022`) or the
    /// keyword `auto` / `latest`. `None` picks the newest channel that serves a
    /// usable manifest.
    pub vs_channel: Option<String>,
    
    /// Directory that stores cached Visual Studio manifests.
    ///
    /// `None` falls back to the platform default (or to the cache manager's
    /// directory when one is injected). Set it to
    /// `MsvcKitConfig::manifest_cache_dir()` to wire the configured cache
    /// directory into the downloader.
    pub manifest_cache_dir: Option<PathBuf>,
    
    /// Dry-run mode: preview without downloading
    pub dry_run: bool,

    /// Additional MSVC components to include (default: empty = standard install)
    ///
    /// By default the standard toolchain (Tools, CRT, MFC, ATL) is downloaded.
    /// Use this to opt into extras such as Spectre-mitigated libraries.
    pub include_components: HashSet<MsvcComponent>,

    /// Package ID patterns to exclude (case-insensitive substring match)
    pub exclude_patterns: Vec<String>,
}
```

## Default Values

`Default::default()` reads the environment, so it is not a fixed table:

```rust
impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            // `MSVC_KIT_INSTALL_DIR`, else the relative path "msvc-kit"
            target_dir: ...,
            msvc_version: std::env::var("MSVC_KIT_MSVC_VERSION").ok(),
            sdk_version: std::env::var("MSVC_KIT_SDK_VERSION").ok(),
            arch: Architecture::host(),
            host_arch: None,                 // Auto-detect
            verify_hashes: true,             // MSVC_KIT_VERIFY_HASHES
            parallel_downloads: 4,           // MSVC_KIT_PARALLEL_DOWNLOADS
            http_client: None,
            progress_handler: None,
            cache_manager: None,
            vs_channel: std::env::var("MSVC_KIT_VS_CHANNEL").ok(),
            manifest_cache_dir: None,        // platform default cache root
            dry_run: false,                  // MSVC_KIT_DRY_RUN
            include_components: ...,         // MSVC_KIT_INCLUDE_COMPONENTS
            exclude_patterns: ...,           // MSVC_KIT_EXCLUDE_PATTERNS
        }
    }
}
```

Recognised environment variables:

| Variable | Effect | Default when unset |
|----------|--------|--------------------|
| `MSVC_KIT_INSTALL_DIR` | Target directory | `msvc-kit` (relative to the working directory) |
| `MSVC_KIT_MSVC_VERSION` | MSVC version | `None` (latest) |
| `MSVC_KIT_SDK_VERSION` | SDK version | `None` (latest) |
| `MSVC_KIT_PARALLEL_DOWNLOADS` | Concurrent downloads | `4` |
| `MSVC_KIT_VERIFY_HASHES` | Hash verification | `true` (anything but `0`/`false`/`no`) |
| `MSVC_KIT_DRY_RUN` | Preview mode | `false` (`1`/`true`/`yes` enables) |
| `MSVC_KIT_VS_CHANNEL` | Visual Studio channel | `None` (auto) |
| `MSVC_KIT_INCLUDE_COMPONENTS` | Comma separated components | empty |
| `MSVC_KIT_EXCLUDE_PATTERNS` | Comma separated patterns | empty |

These apply to the library only. The CLI builds its `DownloadOptions` from flags
and the configuration file and does not read them.

## Usage Examples

### Default Options

```rust
use msvc_kit::{download_msvc, DownloadOptions};

let options = DownloadOptions::default();
let info = download_msvc(&options).await?;
```

### Custom Directory

```rust
use msvc_kit::{download_msvc, DownloadOptions};
use std::path::PathBuf;

let options = DownloadOptions {
    target_dir: PathBuf::from("C:/my-msvc"),
    ..Default::default()
};
```

### Specific Versions

```rust
use msvc_kit::{download_msvc, download_sdk, DownloadOptions};

let options = DownloadOptions {
    msvc_version: Some("14.44".to_string()),
    sdk_version: Some("10.0.26100.0".to_string()),
    ..Default::default()
};

let msvc = download_msvc(&options).await?;
let sdk = download_sdk(&options).await?;
```

### Cross-Compilation

```rust
use msvc_kit::{download_msvc, DownloadOptions, Architecture};

// Build ARM64 binaries on x64 host
let options = DownloadOptions {
    arch: Architecture::Arm64,
    host_arch: Some(Architecture::X64),
    ..Default::default()
};
```

### Performance Tuning

```rust
use msvc_kit::{download_msvc, DownloadOptions};

let options = DownloadOptions {
    parallel_downloads: 8,  // More parallel downloads
    verify_hashes: false,   // Skip verification (not recommended)
    ..Default::default()
};
```

## Field Details

### target_dir

Installation directory. `DownloadOptions::default()` uses `MSVC_KIT_INSTALL_DIR`
when set and the relative path `msvc-kit` otherwise; it does **not** use the CLI
configuration. To install where the CLI would, pass
`load_config()?.install_dir`:

```rust
let options = DownloadOptions::builder()
    .target_dir(msvc_kit::load_config()?.install_dir)
    .build();
```

### msvc_version

MSVC version string. Examples:
- `"14.44"` - Major.minor
- `"14.44.34823"` - Full version
- `None` - Use latest available

### sdk_version

Windows SDK version. Examples:
- `"10.0.26100.0"` - Full version
- `None` - Use latest available

### arch

Target architecture for compiled binaries:
- `Architecture::X64` - 64-bit x86
- `Architecture::X86` - 32-bit x86
- `Architecture::Arm64` - ARM 64-bit
- `Architecture::Arm` - ARM 32-bit

### host_arch

Host machine architecture. Set to `None` for auto-detection.

### verify_hashes

When `true`, downloaded files are verified against SHA256 hashes from the manifest.

### parallel_downloads

Number of concurrent downloads. Higher values may speed up downloads but use more bandwidth.

### http_client

Custom `reqwest::Client` for HTTP requests. Useful for proxy configuration or custom TLS settings.

### progress_handler

Custom progress handler implementing `ProgressHandler` trait. Use `NoopProgressHandler` to suppress output.

### cache_manager

Custom cache manager implementing `CacheManager` trait. Allows shared caching across multiple instances.

```rust
use msvc_kit::{DownloadOptions, FileSystemCacheManager};
use std::path::PathBuf;
use std::sync::Arc;

let cache = Arc::new(FileSystemCacheManager::new(
    PathBuf::from("/shared/cache")
));

let options = DownloadOptions::builder()
    .target_dir("C:/msvc")
    .cache_manager(cache)
    .build();
```

### dry_run

When `true`, shows what would be downloaded without actually downloading.

### include_components / exclude_patterns

`include_components` adds optional components on top of the standard toolchain:
`MsvcComponent::Spectre`, `Mfc`, `Atl`, `Asan`, `Uwp`, `Cli`, `Modules`,
`Redist` and `Custom(String)`. `exclude_patterns` drops any package whose id
contains one of the patterns (case-insensitive).

```rust
use msvc_kit::{DownloadOptions, MsvcComponent};

let options = DownloadOptions::builder()
    .include_components([MsvcComponent::Spectre, MsvcComponent::Mfc])
    .exclude_pattern("arm64")
    .build();
```

### vs_channel / manifest_cache_dir

`vs_channel` pins the Visual Studio channel used for package discovery
(`"17"`, `"2022"`, `"auto"`, …). `manifest_cache_dir` controls where the channel
manifest is cached; leave it `None` for the platform default, or pass
`config.manifest_cache_dir()` to follow the configured cache directory.

## Builder Pattern

The recommended way to create `DownloadOptions`:

```rust
use msvc_kit::{DownloadOptions, Architecture};

let options = DownloadOptions::builder()
    .msvc_version("14.44")
    .sdk_version("10.0.26100.0")
    .target_dir("C:/msvc-kit")
    .arch(Architecture::X64)
    .host_arch(Architecture::X64)
    .verify_hashes(true)
    .parallel_downloads(8)
    .dry_run(false)
    .build();
```

## download_all

Download both MSVC and SDK in parallel:

```rust
use msvc_kit::{download_all, DownloadOptions};

let options = DownloadOptions::default();
let (msvc_info, sdk_info) = download_all(&options).await?;
```
