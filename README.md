# msvc-kit

[![Crates.io](https://img.shields.io/crates/v/msvc-kit.svg)](https://crates.io/crates/msvc-kit)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml)

[English](README.md) | [中文](README_zh.md)

---

A portable MSVC Build Tools installer and manager for Rust/Windows.

### TL;DR

```bash
# Install the CLI
cargo install msvc-kit

# Download latest MSVC + Windows SDK into default data dir
msvc-kit download

# Apply environment to current shell (PowerShell)
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### Features

- **Download MSVC compiler** from Microsoft servers
- **Download Windows SDK** to a chosen directory
- **Auto environment setup** for immediate use (cc-rs compatible)
- **Version management** for host/target architectures (x64, x86, arm64, arm)
- **Library API** for programmatic usage
- **Resumable downloads** with redb-based index for fast skip
- **Manifest caching** with ETag/Last-Modified conditional requests
- **Multi-format extraction** (VSIX, MSI, CAB)
- **Hash verification** with SHA256

### Installation

- **From crates.io**
  ```bash
  cargo install msvc-kit
  ```
- **From source**
  ```bash
  git clone https://github.com/loonghao/msvc-kit.git
  cd msvc-kit
  cargo install --path .
  ```

### Quick Start (CLI)

#### Download

```bash
# Latest versions
msvc-kit download

# Specify versions / dirs / arch
msvc-kit download \
  --msvc-version 14.44 \
  --sdk-version 10.0.26100.0 \
  --target C:\msvc-kit \
  --arch x64 \
  --host-arch x64

# Download only MSVC (skip SDK)
msvc-kit download --no-sdk

# Download only SDK (skip MSVC)
msvc-kit download --no-msvc

# Control parallel downloads (default: 4)
msvc-kit download --parallel-downloads 8

# Skip hash verification
msvc-kit download --no-verify
```

#### Setup Environment

```bash
# Generate script for current shell
msvc-kit setup --script --shell powershell | Invoke-Expression

# Or for CMD
msvc-kit setup --script --shell cmd > setup.bat && setup.bat

# Or for Bash/WSL
eval "$(msvc-kit setup --script --shell bash)"

# Persist to Windows registry (requires admin)
msvc-kit setup --persistent
```

#### List Versions

```bash
msvc-kit list              # Show installed versions
msvc-kit list --available  # Show available versions from Microsoft
```

#### Clean Up

```bash
msvc-kit clean --msvc-version 14.44   # Remove specific MSVC version
msvc-kit clean --sdk-version 10.0.26100.0  # Remove specific SDK version
msvc-kit clean --all                  # Remove all installed versions
msvc-kit clean --all --cache          # Also clear download cache
```

#### Configuration

Config file: `%LOCALAPPDATA%\loonghao\msvc-kit\config\config.toml`

```bash
msvc-kit config                        # Show current config
msvc-kit config --set-dir C:\msvc-kit  # Set install directory
msvc-kit config --set-msvc 14.44       # Set default MSVC version
msvc-kit config --set-sdk 10.0.26100.0 # Set default SDK version
msvc-kit config --reset                # Reset to defaults
```

#### Print Environment Variables

```bash
msvc-kit env                  # Print as shell script
msvc-kit env --format json    # Print as JSON
```

### Caching & Progress

| Cache Type | Location | Description |
|------------|----------|-------------|
| Download index | `downloads/{msvc\|sdk}/.../index.db` | redb database for tracking download status |
| Manifest cache | `cache/manifests/` | Cached VS manifests with ETag/Last-Modified |
| Extraction markers | `.msvc-kit-extracted/` | Skip already-extracted packages |

- **Progress display**: Single-line spinner by default. Set `MSVC_KIT_INNER_PROGRESS=1` for detailed file progress.
- **Skip logic**: Downloads are skipped when:
  - `cached`: File exists in index with matching hash
  - `304`: Server returns Not Modified (ETag/Last-Modified match)
  - `size match`: File size matches expected (best-effort, noted in code)

### Library Usage

```toml
[dependencies]
msvc-kit = "0.1"
```

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use msvc_kit::version::Architecture;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions {
        target_dir: std::path::PathBuf::from("C:/msvc-kit"),
        arch: Architecture::X64,
        host_arch: Some(Architecture::X64),
        verify_hashes: true,
        parallel_downloads: 4,
        ..Default::default()
    };

    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;

    // Get installation paths
    println!("MSVC install path: {:?}", msvc.install_path);
    println!("SDK install path: {:?}", sdk.install_path);
    
    // Get directory paths
    println!("MSVC bin dir: {:?}", msvc.bin_dir());
    println!("MSVC include dir: {:?}", msvc.include_dir());
    println!("MSVC lib dir: {:?}", msvc.lib_dir());
    
    // Get tool paths
    println!("cl.exe: {:?}", env.cl_exe_path());
    println!("link.exe: {:?}", env.link_exe_path());
    
    // Get environment variable strings
    println!("INCLUDE: {}", env.include_path_string());
    println!("LIB: {}", env.lib_path_string());
    
    // Export to JSON (for external tools)
    let json = env.to_json();
    std::fs::write("msvc-env.json", serde_json::to_string_pretty(&json)?)?;

    Ok(())
}
```

### Environment Variables Set

After `setup_environment()` or `msvc-kit setup`:

| Variable | Description |
|----------|-------------|
| `VCINSTALLDIR` | VC install directory |
| `VCToolsInstallDir` | VC tools install directory |
| `VCToolsVersion` | VC tools version |
| `WindowsSdkDir` | Windows SDK directory |
| `WindowsSDKVersion` | Windows SDK version |
| `WindowsSdkBinPath` | Windows SDK bin path |
| `INCLUDE` | Include paths for compiler |
| `LIB` | Library paths for linker |
| `PATH` | Updated with compiler/SDK bin directories |
| `Platform` | Target platform (x64, x86, etc.) |

### Architecture Support

| Architecture | Host | Target | Description |
|--------------|------|--------|-------------|
| `x64` | ✓ | ✓ | 64-bit x86 |
| `x86` | ✓ | ✓ | 32-bit x86 |
| `arm64` | ✓ | ✓ | ARM64 |
| `arm` | - | ✓ | ARM 32-bit (target only) |

### License

MIT License - see `LICENSE`.
