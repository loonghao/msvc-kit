# msvc-kit

[![Crates.io](https://img.shields.io/crates/v/msvc-kit.svg)](https://crates.io/crates/msvc-kit)
[![Crates.io Downloads](https://img.shields.io/crates/d/msvc-kit.svg)](https://crates.io/crates/msvc-kit)
[![GitHub Downloads](https://img.shields.io/github/downloads/loonghao/msvc-kit/total.svg)](https://github.com/loonghao/msvc-kit/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/loonghao/msvc-kit/graph/badge.svg)](https://codecov.io/gh/loonghao/msvc-kit)
[![docs.rs](https://img.shields.io/docsrs/msvc-kit)](https://docs.rs/msvc-kit)

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
- **TLS backend** uses `native-tls` (Windows schannel) to avoid `rustls`/`awslc-sys` build issues
- **Multi-format extraction** (VSIX, MSI, CAB)
- **Hash verification** with SHA256
- **Self-update** via [axoupdater](https://github.com/axodotdev/axoupdater), compatible with cargo-dist releases


### Installation

- **Via Winget (Recommended)**
  ```powershell
  winget install loonghao.msvc-kit
  ```
- **Via PowerShell Script**
  ```powershell
  irm https://github.com/loonghao/msvc-kit/releases/latest/download/install.ps1 | iex
  ```
- **From crates.io**
  ```bash
  cargo install msvc-kit
  ```
- **Pre-built Binaries**
  ```powershell
  # Download and extract to a directory in your PATH
  Invoke-WebRequest -Uri "https://github.com/loonghao/msvc-kit/releases/latest/download/msvc-kit-x86_64-pc-windows-msvc.zip" -OutFile msvc-kit.zip
  Expand-Archive msvc-kit.zip -DestinationPath $env:USERPROFILE\.cargo\bin -Force
  ```
- **From source**
  ```bash
  git clone https://github.com/loonghao/msvc-kit.git
  cd msvc-kit
  cargo install --path .
  ```

### Release bundles
- On every tagged release (or release-please cut), CI builds and uploads `msvc-bundle-<msvc>-<sdk>-<arch>.zip` for `x64`, `x86`, and `arm64` directly to the GitHub Release.
- Bundles are created via `msvc-kit bundle --accept-license`; by downloading you agree to the Microsoft Visual Studio License Terms.

### Quick Start (CLI)


#### Download

```bash
# Latest versions
msvc-kit download

# Specify versions / dirs / arch
# MSVC version can be short (14.44) or full (14.44.34823)
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

> **Note:** MSVC version can be specified as short format (e.g., `14.44`) which auto-resolves to the latest build, or full format (e.g., `14.44.34823`) for a specific build.

**Version Compatibility Quick Reference:**

| Scenario | MSVC | SDK | Command |
|----------|------|-----|---------|
| Latest (recommended) | `14.44` | `10.0.26100.0` | `msvc-kit download` |
| Windows 11 development | `14.42`+ | `10.0.22621.0`+ | `msvc-kit download --sdk-version 10.0.22621.0` |
| Maximum Win10 compat | `14.40` | `10.0.19041.0` | `msvc-kit download --msvc-version 14.40 --sdk-version 10.0.19041.0` |

See [Version Compatibility Guide](docs/guide/cli-download.md#version-compatibility-guide) for detailed information.

#### Setup Environment

```bash
# Generate script for current shell
msvc-kit setup --script --shell powershell | Invoke-Expression

# Or for CMD
msvc-kit setup --script --shell cmd > setup.bat && setup.bat

# Portable script (rewrites install root to %~dp0runtime)
msvc-kit setup --script --shell cmd --portable-root "%~dp0runtime" > setup.bat

# Or for Bash/WSL
eval "$(msvc-kit setup --script --shell bash)"

# Persist to Windows registry (requires admin)
msvc-kit setup --persistent
```

#### Create Portable Bundle

Create a self-contained bundle with MSVC toolchain that can be used anywhere:

```bash
# Create bundle (requires accepting Microsoft license)
msvc-kit bundle --accept-license

# Specify output directory and architecture
msvc-kit bundle --accept-license --output ./my-msvc-bundle --arch x64

# Cross-compilation bundle (x64 host targeting ARM64)
msvc-kit bundle --accept-license --host-arch x64 --arch arm64

# Also create a zip archive
msvc-kit bundle --accept-license --zip

# Specify versions
msvc-kit bundle --accept-license --msvc-version 14.44 --sdk-version 10.0.26100.0
```

The bundle contains:
- `msvc-kit.exe` - CLI tool
- `VC/Tools/MSVC/{version}/` - MSVC compiler and tools
- `Windows Kits/10/` - Windows SDK
- `setup.bat` - CMD activation script
- `setup.ps1` - PowerShell activation script
- `setup.sh` - Bash/WSL activation script
- `README.txt` - Usage instructions

Usage:
```bash
# Extract and run setup script
cd msvc-bundle
setup.bat          # CMD
.\setup.ps1        # PowerShell
source setup.sh    # Bash/WSL

# Now cl, link, nmake are available
cl /nologo test.c
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

#### Self-Update

```bash
# Check for updates without installing
msvc-kit update --check

# Update to the latest version
msvc-kit update

# Update to a specific version
msvc-kit update --version 0.2.5
```

The self-update feature is powered by [axoupdater](https://github.com/axodotdev/axoupdater) and queries GitHub Releases directly. It is compatible with both cargo-dist and custom release workflows. The `self-update` feature is enabled by default and can be disabled with `--no-default-features` at build time.

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

### TLS Backend Configuration

By default, msvc-kit uses `native-tls` (SChannel on Windows) to avoid requiring `cmake` and `NASM` during compilation. This resolves the [aws-lc-sys build issue](https://github.com/loonghao/msvc-kit/issues/44).

| Feature | TLS Backend | Build Requirements | Default |
|---------|-------------|--------------------|---------|
| `native-tls` | Platform native (SChannel/OpenSSL) | None | ✓ |
| `rustls-tls` | rustls + aws-lc-rs | cmake, NASM | - |

```toml
# Default: native-tls (recommended, no extra build deps)
[dependencies]
msvc-kit = "0.2"

# Or explicitly choose a TLS backend:
msvc-kit = { version = "0.2", default-features = false, features = ["native-tls"] }

# Use rustls instead (requires cmake + NASM on Windows):
msvc-kit = { version = "0.2", default-features = false, features = ["rustls-tls"] }
```

### Library Usage

```toml
[dependencies]
msvc-kit = "0.2"
```

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use msvc_kit::{list_available_versions, Architecture};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    // List available versions from Microsoft
    let versions = list_available_versions().await?;
    println!("Latest MSVC: {:?}", versions.latest_msvc);
    println!("Latest SDK: {:?}", versions.latest_sdk);

    // Download with builder pattern
    let options = DownloadOptions::builder()
        .target_dir("C:/msvc-kit")
        .arch(Architecture::X64)
        .build();

    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;

    println!("cl.exe: {:?}", env.cl_exe_path());
    Ok(())
}
```

See [Library API Documentation](docs/api/library.md) for full API reference.

### Architecture Support

| Architecture | Host | Target | Description |
|--------------|------|--------|-------------|
| `x64` | ✓ | ✓ | 64-bit x86 |
| `x86` | ✓ | ✓ | 32-bit x86 |
| `arm64` | ✓ | ✓ | ARM64 |
| `arm` | - | ✓ | ARM 32-bit (target only) |

### License

MIT License - see `LICENSE`.

**Important: Microsoft Software License Notice**

The MSVC compiler and Windows SDK downloaded by this tool are property of Microsoft
and subject to [Microsoft Visual Studio License Terms](https://visualstudio.microsoft.com/license-terms/).

- **msvc-kit** itself is MIT licensed
- MSVC Build Tools and Windows SDK are **NOT redistributable** - users must download them directly
- By using `msvc-kit download` or `msvc-kit bundle --accept-license`, you agree to Microsoft's license terms
- This tool automates the download process; it does not redistribute Microsoft software
