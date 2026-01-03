# msvc-kit Architecture

This document describes the internal architecture of msvc-kit, including module responsibilities, data flow, and extension points.

## Module Overview

```
msvc-kit/
├── src/
│   ├── lib.rs              # Public API exports
│   ├── bin/msvc-kit.rs     # CLI entry point
│   ├── config/             # Configuration management (TOML)
│   ├── constants.rs        # Centralized constants
│   ├── downloader/         # Core download logic
│   │   ├── mod.rs          # DownloadOptions, public API
│   │   ├── traits.rs       # ComponentDownloader trait
│   │   ├── progress.rs     # ProgressHandler trait
│   │   ├── cache.rs        # Manifest caching (ETag/Last-Modified)
│   │   ├── common.rs       # CommonDownloader implementation
│   │   ├── manifest.rs     # VS manifest parsing
│   │   ├── msvc.rs         # MsvcDownloader
│   │   ├── sdk.rs          # SdkDownloader
│   │   ├── index.rs        # Download index (redb)
│   │   ├── hash.rs         # SHA256 verification
│   │   └── http.rs         # HTTP client configuration
│   ├── env/                # Environment setup
│   │   ├── mod.rs          # MsvcEnvironment, ToolPaths
│   │   └── setup.rs        # Activation scripts, registry
│   ├── error.rs            # Error types
│   ├── installer/          # Extraction logic
│   │   ├── mod.rs          # InstallInfo
│   │   └── extractor.rs    # VSIX/MSI/CAB extraction
│   └── version/            # Version types
│       └── mod.rs          # Architecture, Version<T>
└── tests/
    ├── e2e_tests.rs        # End-to-end tests
    └── unit_tests.rs       # Unit tests
```

## Core Traits

### ComponentDownloader

```rust
#[async_trait]
pub trait ComponentDownloader: Send + Sync {
    /// Download the component
    async fn download(&self) -> Result<InstallInfo>;
    
    /// Get the component type
    fn component_type(&self) -> ComponentType;
    
    /// Get the component name for display
    fn component_name(&self) -> &'static str;
}
```

Implementations:
- `MsvcDownloader` - Downloads MSVC compiler toolchain
- `SdkDownloader` - Downloads Windows SDK

### ProgressHandler

```rust
pub trait ProgressHandler: Send + Sync {
    fn on_download_start(&self, total_files: usize, total_bytes: u64);
    fn on_file_start(&self, file_name: &str, file_size: u64);
    fn on_file_progress(&self, file_name: &str, bytes_downloaded: u64, total_bytes: u64);
    fn on_file_complete(&self, file_name: &str, outcome: &str);
    fn on_download_complete(&self, total_files: usize, total_bytes: u64);
}
```

Implementations:
- `IndicatifProgressHandler` - Terminal progress bars
- `NoopProgressHandler` - Silent operation

### CacheManager

```rust
pub trait CacheManager: Send + Sync {
    /// Get cached data by key
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    
    /// Store data in cache
    fn set(&self, key: &str, value: &[u8]) -> Result<()>;
    
    /// Invalidate a cache entry
    fn invalidate(&self, key: &str) -> Result<()>;
    
    /// Clear all cache entries
    fn clear(&self) -> Result<()>;
}
```

Implementations:
- `FileSystemCacheManager` - Disk-based caching

## Data Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              User Request                                │
│                     (CLI or Library API call)                            │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           DownloadOptions                                │
│  - msvc_version, sdk_version                                            │
│  - target_dir, arch                                                     │
│  - http_client (injectable)                                             │
│  - progress_handler (injectable)                                        │
│  - cache_manager (injectable)                                           │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      ComponentDownloader                                 │
│                   (MsvcDownloader / SdkDownloader)                       │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
           ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
           │  Manifest    │ │    Cache     │ │   Progress   │
           │   Parser     │ │   Manager    │ │   Handler    │
           └──────────────┘ └──────────────┘ └──────────────┘
                    │               │               │
                    └───────────────┼───────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        CommonDownloader                                  │
│  - Parallel downloads (configurable)                                    │
│  - Hash verification (SHA256)                                           │
│  - Resume support (via DownloadIndex)                                   │
│  - ETag/Last-Modified caching                                           │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           Extractor                                      │
│  - VSIX (ZIP format)                                                    │
│  - MSI (Windows Installer)                                              │
│  - CAB (Cabinet archives)                                               │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           InstallInfo                                    │
│  - install_path: PathBuf                                                │
│  - version: String                                                      │
│  - components: Vec<InstalledComponent>                                  │
└─────────────────────────────────────────────────────────────────────────┘
```

## Caching Strategy

msvc-kit uses a multi-layer caching strategy:

### 1. Manifest Cache (ETag/Last-Modified)

```
Priority: ETag/If-None-Match > Last-Modified/If-Modified-Since > Fingerprint
```

- **ETag**: Preferred method, semantically correct
- **Last-Modified**: Fallback when ETag unavailable
- **Fingerprint**: `name + size` hash for fast skip (best-effort, not cryptographically strong)

### 2. Payload Cache (Download Index)

Uses `redb` embedded database for crash-safe tracking:

```rust
struct IndexEntry {
    url: String,
    local_path: PathBuf,
    sha256: Option<String>,
    size: u64,
    status: DownloadStatus,
    updated_at: DateTime<Utc>,
}
```

### 3. Skip Logic Output

All skip decisions are clearly logged:
- `cached` - Full cache hit via ETag/304
- `size match` - Fingerprint-based skip
- `hash match` - SHA256 verification passed

## Extension Points

### 1. HTTP Client Injection

```rust
let custom_client = reqwest::Client::builder()
    .proxy(reqwest::Proxy::all("http://proxy:8080")?)
    .build()?;

let options = DownloadOptions::builder()
    .http_client(custom_client)
    .build();
```

### 2. Progress Handler Injection

```rust
struct MyProgressHandler;

impl ProgressHandler for MyProgressHandler {
    // Custom implementation
}

let options = DownloadOptions::builder()
    .progress_handler(Box::new(MyProgressHandler))
    .build();
```

### 3. Cache Manager Injection (vx integration)

```rust
let shared_cache = vx::cache::SharedCacheManager::new();

let options = DownloadOptions::builder()
    .cache_manager(Box::new(shared_cache))
    .build();
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MSVC_KIT_INSTALL_DIR` | Installation directory | `msvc-kit` |
| `MSVC_KIT_MSVC_VERSION` | MSVC version | latest |
| `MSVC_KIT_SDK_VERSION` | SDK version | latest |
| `MSVC_KIT_PARALLEL_DOWNLOADS` | Concurrent downloads | 4 |
| `MSVC_KIT_VERIFY_HASHES` | Hash verification | true |
| `MSVC_KIT_DRY_RUN` | Preview mode | false |

## Configuration (TOML)

```toml
# ~/.config/msvc-kit/config.toml

install_dir = "C:\\msvc-kit"
default_msvc_version = "14.40"
default_sdk_version = "10.0.22621"
default_arch = "x64"
verify_hashes = true
parallel_downloads = 8
cache_dir = "C:\\msvc-kit\\cache"
```

## Error Handling

All errors are typed via `MsvcKitError`:

```rust
pub enum MsvcKitError {
    Io(std::io::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
    Toml(toml::de::Error),
    Extraction(String),
    VersionNotFound(String),
    HashMismatch { expected: String, actual: String },
    Other(String),
}
```

## Testing Strategy

- **Unit tests**: `tests/unit_tests.rs` - Individual component tests
- **E2E tests**: `tests/e2e_tests.rs` - Full download workflow (requires network)
- **Mock tests**: Use `mockito` for HTTP mocking

Run tests:
```bash
cargo test                    # Unit tests only
cargo test --features e2e     # Include E2E tests
```
