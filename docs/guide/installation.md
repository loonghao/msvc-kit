# Installation

## System Requirements

| Requirement | Minimum |
|-------------|---------|
| OS | Windows 10 1809+ / Windows 11 |
| Architecture | x64 or ARM64 |
| Disk Space | ~2 GB (MSVC only) / ~5 GB (MSVC + SDK) |
| Rust | 1.70+ (for cargo install) |

## Installation Methods

### Via Winget (Recommended)

The easiest way to install msvc-kit on Windows:

```powershell
winget install loonghao.msvc-kit
```

### Via PowerShell Script

One-liner installation script:

```powershell
irm https://github.com/loonghao/msvc-kit/releases/latest/download/install.ps1 | iex
```

This script automatically downloads the latest release and installs it to your PATH.

### Direct Download

Download the pre-built binary from the [latest GitHub Release](https://github.com/loonghao/msvc-kit/releases/latest):

| Platform | Download |
|----------|----------|
| Windows x64 | [msvc-kit-x86_64-windows.exe](https://github.com/loonghao/msvc-kit/releases/latest/download/msvc-kit-x86_64-windows.exe) |

Place the downloaded `.exe` anywhere in your `PATH`.

### Via Cargo

If you have Rust installed:

```bash
cargo install msvc-kit
```

This installs the latest stable version from crates.io.

### From Source

```bash
git clone https://github.com/loonghao/msvc-kit.git
cd msvc-kit
cargo install --path .
```

## Verify Installation

```bash
msvc-kit --version
# msvc-kit 0.1.x

msvc-kit --help
```

## Uninstallation

### Remove CLI

```bash
cargo uninstall msvc-kit
```

### Remove Downloaded Components

```bash
# Remove all installed versions and cache
msvc-kit clean --all --cache

# Or manually delete the data directory
rm -rf "$env:LOCALAPPDATA\loonghao\msvc-kit"
```

### Remove Configuration

```powershell
rm "$env:LOCALAPPDATA\loonghao\msvc-kit\config\config.json"
```

## Troubleshooting

### Cargo Install Fails

If `cargo install` fails with linker errors, you might need MSVC to build msvc-kit itself. Use Winget or the PowerShell install script instead.

### Permission Denied

Run PowerShell as Administrator if you encounter permission issues.

### Network Issues

If downloads fail, check:
- Firewall settings (allow HTTPS to Microsoft CDN)
- Proxy configuration
- Try with `--parallel-downloads 1` to reduce connections

```bash
msvc-kit download --parallel-downloads 1
```
