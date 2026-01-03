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

/// Generate shell activation script
pub fn generate_activation_script(
    env: &MsvcEnvironment,
    shell: ShellType,
) -> String;

/// Get environment variables as HashMap
pub fn get_env_vars(env: &MsvcEnvironment) -> HashMap<String, String>;
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
pub use downloader::DownloadOptions;
pub use env::{MsvcEnvironment, ShellType, ToolPaths};
pub use error::{MsvcKitError, Result};
pub use installer::InstallInfo;
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

Currently no optional features. All functionality is included by default.

## Thread Safety

- `DownloadOptions`, `InstallInfo`, `MsvcEnvironment` are `Send + Sync`
- Download functions are async and can be called from any runtime
- Configuration functions use file locking for concurrent access

## Next Steps

- [DownloadOptions](./download-options.md) - Configure downloads
- [InstallInfo](./install-info.md) - Access installation details
- [MsvcEnvironment](./msvc-environment.md) - Environment configuration
- [ToolPaths](./tool-paths.md) - Access tool executables
