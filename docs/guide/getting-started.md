# Getting Started

This guide will help you install msvc-kit and set up your first MSVC environment.

## Prerequisites

- Windows 10/11 (x64 or ARM64)
- Rust toolchain (for installation via cargo)
- ~2-5 GB disk space for MSVC + SDK

## Installation

### From crates.io (Recommended)

```bash
cargo install msvc-kit
```

### From Source

```bash
git clone https://github.com/loonghao/msvc-kit.git
cd msvc-kit
cargo install --path .
```

## Quick Start

### 1. Download MSVC and Windows SDK

```bash
msvc-kit download
```

This downloads:
- Latest MSVC compiler (cl.exe, link.exe, etc.)
- Latest Windows SDK (headers, libraries, tools)

Default location: `%LOCALAPPDATA%\loonghao\msvc-kit\`

### 2. Setup Environment

#### PowerShell (Recommended)

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression
```

#### CMD

```cmd
msvc-kit setup --script --shell cmd > setup.bat && setup.bat
```

#### Bash (WSL/Git Bash)

```bash
eval "$(msvc-kit setup --script --shell bash)"
```

### 3. Verify Installation

```bash
# Check cl.exe is available
cl /?

# Check version
cl
# Microsoft (R) C/C++ Optimizing Compiler Version 19.xx.xxxxx for x64
```

## What Gets Installed

After running `msvc-kit download`, your directory structure looks like:

```
%LOCALAPPDATA%\loonghao\msvc-kit\
├── VC/
│   └── Tools/
│       └── MSVC/
│           └── 14.xx.xxxxx/
│               ├── bin/
│               │   └── Hostx64/
│               │       └── x64/
│               │           ├── cl.exe
│               │           ├── link.exe
│               │           └── ...
│               ├── include/
│               └── lib/
├── Windows Kits/
│   └── 10/
│       ├── bin/
│       │   └── 10.0.xxxxx.0/
│       ├── Include/
│       │   └── 10.0.xxxxx.0/
│       └── Lib/
│           └── 10.0.xxxxx.0/
└── downloads/
    ├── msvc/
    └── sdk/
```

## Environment Variables Set

After `msvc-kit setup`, these environment variables are configured:

| Variable | Description |
|----------|-------------|
| `VCINSTALLDIR` | VC installation directory |
| `VCToolsInstallDir` | VC tools directory |
| `VCToolsVersion` | VC tools version |
| `WindowsSdkDir` | Windows SDK directory |
| `WindowsSDKVersion` | Windows SDK version |
| `INCLUDE` | Include paths for compiler |
| `LIB` | Library paths for linker |
| `PATH` | Updated with bin directories |

## Next Steps

- [Download Options](./cli-download.md) - Customize download behavior
- [Configuration](./cli-config.md) - Persistent configuration
- [Library API](../api/library.md) - Use as Rust library
