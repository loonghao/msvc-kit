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
    
    /// Dry-run mode: preview without downloading
    pub dry_run: bool,
}
```

## Default Values

```rust
impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            target_dir: default_install_dir(),
            msvc_version: None,      // Latest
            sdk_version: None,       // Latest
            arch: Architecture::X64,
            host_arch: None,         // Auto-detect
            verify_hashes: true,
            parallel_downloads: 4,
        }
    }
}
```

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

Installation directory. Defaults to:
- Windows: `%LOCALAPPDATA%\loonghao\msvc-kit`

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
