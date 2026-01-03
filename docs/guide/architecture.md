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

let options = DownloadOptions {
    arch: Architecture::Arm64,
    host_arch: Some(Architecture::X64),
    ..Default::default()
};
```
