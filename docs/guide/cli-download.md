# Download Command

The `download` command downloads MSVC compiler and/or Windows SDK from Microsoft servers.

## Basic Usage

```bash
# Download latest MSVC and Windows SDK
msvc-kit download
```

## Options

### Version Selection

```bash
# Specific MSVC version
msvc-kit download --msvc-version 14.44

# Specific SDK version
msvc-kit download --sdk-version 10.0.26100.0

# Both
msvc-kit download --msvc-version 14.44 --sdk-version 10.0.26100.0
```

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
