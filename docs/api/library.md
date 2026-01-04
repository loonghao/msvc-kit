# Library API Overview

msvc-kit can be used as a Rust library for programmatic access to MSVC toolchain management.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
msvc-kit = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Quick Example

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    // Download with default options
    let options = DownloadOptions::default();
    
    let msvc_info = download_msvc(&options).await?;
    let sdk_info = download_sdk(&options).await?;
    
    // Setup environment
    let env = setup_environment(&msvc_info, Some(&sdk_info))?;
    
    // Access paths
    println!("cl.exe: {:?}", env.cl_exe_path());
    println!("INCLUDE: {}", env.include_path_string());
    println!("LIB: {}", env.lib_path_string());
    
    Ok(())
}
```

## Main Types

### Version Discovery Functions

```rust
/// Fetch available versions from Microsoft servers
pub async fn list_available_versions() -> Result<AvailableVersions>;

/// Available version information
pub struct AvailableVersions {
    pub msvc_versions: Vec<String>,  // e.g., ["14.44", "14.43", "14.42"]
    pub sdk_versions: Vec<String>,   // e.g., ["10.0.26100.0", "10.0.22621.0"]
    pub latest_msvc: Option<String>, // e.g., Some("14.44")
    pub latest_sdk: Option<String>,  // e.g., Some("10.0.26100.0")
}
```

**Example:**

```rust
use msvc_kit::list_available_versions;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let versions = list_available_versions().await?;
    
    println!("Latest MSVC: {:?}", versions.latest_msvc);
    println!("Latest SDK: {:?}", versions.latest_sdk);
    
    println!("\nAvailable MSVC versions:");
    for v in &versions.msvc_versions {
        println!("  {}", v);
    }
    Ok(())
}
```

### Download Functions

```rust
/// Download MSVC compiler components
pub async fn download_msvc(options: &DownloadOptions) -> Result<InstallInfo>;

/// Download Windows SDK components
pub async fn download_sdk(options: &DownloadOptions) -> Result<InstallInfo>;
```

### Environment Functions

```rust
/// Setup environment from install info
pub fn setup_environment(
    msvc_info: &InstallInfo,
    sdk_info: Option<&InstallInfo>,
) -> Result<MsvcEnvironment>;

/// Get environment variables as HashMap
pub fn get_env_vars(env: &MsvcEnvironment) -> HashMap<String, String>;
```

### Script Generation Functions

```rust
/// Generate portable scripts (relative paths, for bundles)
pub fn generate_portable_scripts(ctx: &ScriptContext) -> Result<GeneratedScripts>;

/// Generate absolute path scripts (for installed environments)
pub fn generate_absolute_scripts(ctx: &ScriptContext) -> Result<GeneratedScripts>;

/// Generate a single script for specified shell
pub fn generate_script(ctx: &ScriptContext, shell: ShellType) -> Result<String>;

/// Save scripts to a directory
pub async fn save_scripts(
    scripts: &GeneratedScripts,
    output_dir: &Path,
    base_name: &str,
) -> Result<()>;
```

### Script Context

```rust
/// Create portable script context (uses relative paths like %~dp0)
let ctx = ScriptContext::portable(
    "14.44.34823",      // MSVC version
    "10.0.26100.0",     // SDK version
    Architecture::X64,   // Target arch
    Architecture::X64,   // Host arch
);

/// Create absolute script context (uses actual paths)
let ctx = ScriptContext::absolute(
    PathBuf::from("C:/msvc-kit"),
    "14.44.34823",
    "10.0.26100.0",
    Architecture::X64,
    Architecture::X64,
);
```

### Configuration Functions

```rust
/// Load configuration from disk
pub fn load_config() -> Result<MsvcKitConfig>;

/// Save configuration to disk
pub fn save_config(config: &MsvcKitConfig) -> Result<()>;
```

## Re-exported Types

```rust
pub use config::MsvcKitConfig;
pub use downloader::{DownloadOptions, AvailableVersions, list_available_versions};
pub use env::{MsvcEnvironment, ToolPaths};
pub use error::{MsvcKitError, Result};
pub use installer::InstallInfo;
pub use scripts::{GeneratedScripts, ScriptContext, ShellType};
pub use version::{Architecture, MsvcVersion, SdkVersion};
```

## Error Handling

All functions return `msvc_kit::Result<T>`:

```rust
use msvc_kit::{download_msvc, DownloadOptions, MsvcKitError};

async fn example() {
    let options = DownloadOptions::default();
    
    match download_msvc(&options).await {
        Ok(info) => println!("Installed to {:?}", info.install_path),
        Err(MsvcKitError::NetworkError(e)) => eprintln!("Network error: {}", e),
        Err(MsvcKitError::VersionNotFound(v)) => eprintln!("Version not found: {}", v),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

## Feature Flags

msvc-kit provides optional features to reduce dependency conflicts:

### `self-update` (default)

Enables the CLI self-update functionality. This feature includes the `self_update` crate which depends on `lzma-sys`.

```toml
# Include self-update (default)
[dependencies]
msvc-kit = "0.1"

# Or explicitly enable
[dependencies]
msvc-kit = { version = "0.1", features = ["self-update"] }
```

### Library-only Usage (No Self-update)

If you're using msvc-kit as a library and encounter dependency conflicts (e.g., with `liblzma-sys`), you can disable the default features:

```toml
[dependencies]
msvc-kit = { version = "0.1", default-features = false }
```

This is useful when integrating msvc-kit into projects that use different LZMA implementations, avoiding the `lzma-sys` conflict:

```
error: the crate `lzma` is compiled multiple times, possibly with different configurations
  - crate `liblzma_sys` links to native library `lzma`
  - crate `lzma_sys` links to native library `lzma`
```

## Thread Safety

- `DownloadOptions`, `InstallInfo`, `MsvcEnvironment` are `Send + Sync`
- Download functions are async and can be called from any runtime
- Configuration functions use file locking for concurrent access

## Next Steps

- [DownloadOptions](./download-options.md) - Configure downloads
- [InstallInfo](./install-info.md) - Access installation details
- [MsvcEnvironment](./msvc-environment.md) - Environment configuration
- [ToolPaths](./tool-paths.md) - Access tool executables
