# Architecture Support

msvc-kit supports multiple CPU architectures for both host (build machine) and target (output binaries).

## Supported Architectures

| Architecture | As Host | As Target | Description |
|--------------|---------|-----------|-------------|
| `x64` | ✅ | ✅ | 64-bit x86 (AMD64) |
| `x86` | ✅ | ✅ | 32-bit x86 |
| `arm64` | ✅ | ✅ | ARM 64-bit |
| `arm` | ❌ | ✅ | ARM 32-bit (target only) |

## Host vs Target Architecture

- **Host Architecture**: The CPU architecture of your build machine
- **Target Architecture**: The CPU architecture of the compiled binaries

### Native Compilation

Build machine and output have the same architecture:

```bash
# On x64 machine, build x64 binaries
msvc-kit download --host-arch x64 --arch x64
```

### Cross Compilation

Build on one architecture for another:

```bash
# On x64 machine, build ARM64 binaries
msvc-kit download --host-arch x64 --arch arm64

# On x64 machine, build x86 (32-bit) binaries
msvc-kit download --host-arch x64 --arch x86
```

## Architecture Filtering

msvc-kit intelligently filters downloaded packages based on your specified architecture, significantly reducing download size and installation time.

### What Gets Downloaded

When you specify an architecture (e.g., `--arch x64`), msvc-kit downloads only:

| Package Type | Filtering Behavior |
|--------------|-------------------|
| **Tools** | Only matching host/target combination (e.g., `HostX64.TargetX64`) |
| **CRT Libraries** | Only matching architecture (e.g., `CRT.x64.Desktop`) |
| **MFC/ATL** | Only matching architecture (e.g., `MFC.x64`, `ATL.x64`) |
| **Headers** | Always included (architecture-neutral) |
| **Spectre Libraries** | Excluded by default (rarely needed) |

### What Gets Excluded

The following are automatically excluded to minimize download size:

- **Other architectures**: ARM64, x86, ARM packages when targeting x64
- **Spectre-mitigated libraries**: `.Spectre` suffix packages
- **Redundant packages**: Duplicate architecture variants

### Download Size Comparison

| Configuration | Approximate Size |
|--------------|------------------|
| All architectures (old behavior) | ~2.5 GB |
| Single architecture (x64) | ~300-500 MB |
| Minimal (tools only) | ~150-250 MB |

## Directory Structure

MSVC uses a specific directory structure for cross-compilation:

```
VC/Tools/MSVC/14.xx/bin/
├── Hostx64/
│   ├── x64/      # x64 host → x64 target (native)
│   ├── x86/      # x64 host → x86 target (cross)
│   └── arm64/    # x64 host → ARM64 target (cross)
├── Hostx86/
│   ├── x86/      # x86 host → x86 target (native)
│   └── x64/      # x86 host → x64 target (cross)
└── Hostarm64/
    ├── arm64/    # ARM64 host → ARM64 target (native)
    ├── x64/      # ARM64 host → x64 target (cross)
    └── x86/      # ARM64 host → x86 target (cross)
```

## Auto-Detection

By default, msvc-kit auto-detects your host architecture:

```bash
# Auto-detect host, target x64
msvc-kit download --arch x64
```

## Common Scenarios

### Build 32-bit on 64-bit Windows

```bash
msvc-kit download --host-arch x64 --arch x86
msvc-kit setup --script --shell powershell | Invoke-Expression

# cl.exe now targets x86
cl /c myfile.cpp  # Produces x86 object file
```

### Build ARM64 for Windows on ARM

```bash
# On x64 machine
msvc-kit download --host-arch x64 --arch arm64
msvc-kit setup --script --shell powershell | Invoke-Expression

# cl.exe now targets ARM64
cl /c myfile.cpp  # Produces ARM64 object file
```

### Multi-Architecture Setup

Download multiple architectures:

```bash
# Download for multiple targets
msvc-kit download --arch x64
msvc-kit download --arch x86
msvc-kit download --arch arm64
```

## Library API

```rust
use msvc_kit::{DownloadOptions, Architecture};

let options = DownloadOptions::builder()
    .arch(Architecture::X64)
    .host_arch(Architecture::X64)
    .target_dir("C:/msvc-kit")
    .build();

// Only x64 packages will be downloaded
let info = msvc_kit::download_msvc(&options).await?;
```

### Cross-Compilation Example

```rust
use msvc_kit::{DownloadOptions, Architecture};

// Build ARM64 binaries on x64 host
let options = DownloadOptions::builder()
    .arch(Architecture::Arm64)
    .host_arch(Architecture::X64)
    .target_dir("C:/msvc-kit")
    .build();

// Downloads: HostX64.TargetARM64 tools + ARM64 CRT/MFC/ATL
let info = msvc_kit::download_msvc(&options).await?;
```
