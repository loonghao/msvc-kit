# Download Command

The `download` command downloads MSVC compiler and/or Windows SDK from Microsoft servers.

## Basic Usage

```bash
# Download latest MSVC and Windows SDK
msvc-kit download
```

## Options

### Version Selection

You can specify versions using either **short** or **full** format:

```bash
# Short version (recommended) - automatically resolves to latest build
msvc-kit download --msvc-version 14.44

# Full version - specific build number
msvc-kit download --msvc-version 14.44.34823

# SDK version (always use full format)
msvc-kit download --sdk-version 10.0.26100.0

# Both
msvc-kit download --msvc-version 14.44 --sdk-version 10.0.26100.0
```

**Version Format:**

| Component | Short Format | Full Format | Example |
|-----------|-------------|-------------|---------|
| MSVC | `14.44` | `14.44.34823` | `--msvc-version 14.44` |
| SDK | N/A | `10.0.26100.0` | `--sdk-version 10.0.26100.0` |

When using short MSVC version (e.g., `14.44`), msvc-kit automatically resolves it to the latest available build number (e.g., `14.44.34823`).

### Version Compatibility Guide

#### MSVC and Windows SDK Pairing

**TL;DR:** Use the latest versions of both for best compatibility. They are designed to work together.

| Scenario | Recommended MSVC | Recommended SDK | Notes |
|----------|-----------------|-----------------|-------|
| **General Development** | Latest (`14.44`) | Latest (`10.0.26100.0`) | Best compatibility and features |
| **Windows 11 Apps** | `14.42`+ | `10.0.22621.0`+ | Win11 SDK required for Win11-specific APIs |
| **Windows 10 Support** | Any | `10.0.19041.0`+ | Older SDKs still work |
| **Legacy Projects** | Match project's VS version | Match project's SDK | See version mapping below |

#### MSVC Version ↔ Visual Studio Mapping

| MSVC Toolset | Visual Studio | _MSC_VER | Support Status |
|--------------|---------------|----------|----------------|
| `14.44` | VS 2022 17.14+ | 1944 | Current |
| `14.43` | VS 2022 17.13 | 1943 | Supported |
| `14.42` | VS 2022 17.12 | 1942 | Supported |
| `14.41` | VS 2022 17.11 | 1941 | Supported |
| `14.40` | VS 2022 17.10 | 1940 | Supported |
| `14.39` | VS 2022 17.9 | 1939 | Supported |
| `14.38` | VS 2022 17.8 (LTSC) | 1938 | Long-term support |

#### Windows SDK Version ↔ Windows Version

| SDK Version | Target Windows | Codename | Notes |
|-------------|----------------|----------|-------|
| `10.0.26100.0` | Windows 11 24H2 | | Latest, recommended |
| `10.0.22621.0` | Windows 11 22H2 | | Stable, widely used |
| `10.0.22000.0` | Windows 11 21H2 | | First Win11 SDK |
| `10.0.19041.0` | Windows 10 2004 | | Good Win10 baseline |
| `10.0.18362.0` | Windows 10 1903 | | Legacy support |

#### Common Combinations

```bash
# Recommended: Latest everything (best for new projects)
msvc-kit download --msvc-version 14.44 --sdk-version 10.0.26100.0

# Stable: Widely tested combination
msvc-kit download --msvc-version 14.42 --sdk-version 10.0.22621.0

# Windows 10 focused: Maximum compatibility
msvc-kit download --msvc-version 14.40 --sdk-version 10.0.19041.0
```

#### Important Notes

1. **Forward Compatibility:** Newer MSVC versions can target older Windows versions. Use `_WIN32_WINNT` macro to control target.

2. **SDK Selection:** The SDK version determines which Windows APIs are available at compile time, not runtime compatibility.

3. **ABI Compatibility:** MSVC 14.x toolsets (VS 2015-2022) share the same C++ ABI, so libraries built with 14.40 work with 14.44.

4. **When in Doubt:** Use the latest versions. Microsoft ensures backward compatibility.

### Component Selection

```bash
# Download only MSVC (skip SDK)
msvc-kit download --no-sdk

# Download only SDK (skip MSVC)
msvc-kit download --no-msvc
```

### Target Directory

```bash
# Custom installation directory
msvc-kit download --target C:\msvc-kit
```

### Architecture

```bash
# Target architecture (default: x64)
msvc-kit download --arch x64

# Host architecture (default: auto-detect)
msvc-kit download --host-arch x64

# Cross-compilation: build ARM64 on x64 host
msvc-kit download --host-arch x64 --arch arm64
```

Supported architectures:
- `x64` - 64-bit x86
- `x86` - 32-bit x86
- `arm64` - ARM64
- `arm` - ARM 32-bit (target only)

### Download Options

```bash
# Parallel downloads (default: 4)
msvc-kit download --parallel-downloads 8

# Skip hash verification (not recommended)
msvc-kit download --no-verify
```

## Full Example

```bash
msvc-kit download \
  --msvc-version 14.44 \
  --sdk-version 10.0.26100.0 \
  --target C:\msvc-kit \
  --arch x64 \
  --host-arch x64 \
  --parallel-downloads 8
```

## Download Progress

The download shows progress for each package:

```
Downloading MSVC packages...
[1/15] Microsoft.VC.14.44.17.14.CRT.Headers.x64 (2.3 MB)
[2/15] Microsoft.VC.14.44.17.14.CRT.Source (1.1 MB)
...
```

## Caching Behavior

Downloads are cached and skipped if already present:

| Status | Meaning |
|--------|---------|
| `cached` | File exists in index with matching hash |
| `304` | Server returned Not Modified (ETag match) |
| `size match` | File size matches expected (best-effort) |

To force re-download, use `msvc-kit clean --cache` first.

## Available Versions

List available versions before downloading:

```bash
msvc-kit list --available
```

Output:
```
Available MSVC versions:
  14.44.34823
  14.43.34808
  14.42.34433
  ...

Available SDK versions:
  10.0.26100.0
  10.0.22621.0
  10.0.22000.0
  ...
```
