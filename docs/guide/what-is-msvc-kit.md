# What is msvc-kit?

msvc-kit is a portable MSVC Build Tools installer and manager for Rust/Windows development. It allows you to download and use the Microsoft Visual C++ compiler without installing the full Visual Studio IDE.

## Why msvc-kit?

### The Problem

Building native code on Windows typically requires:
- Installing Visual Studio (several GB)
- Running the Visual Studio Installer
- Manually configuring environment variables
- Dealing with version conflicts

This is especially painful for:
- **CI/CD pipelines** - Installing VS takes too long
- **Lightweight environments** - Docker containers, VMs
- **Version management** - Need multiple MSVC versions
- **DCC plugin development** - Maya, Houdini, UE5 plugins

### The Solution

msvc-kit downloads only the necessary components directly from Microsoft's servers:

```bash
# Just 3 commands
cargo install msvc-kit
msvc-kit download
msvc-kit setup --script --shell powershell | Invoke-Expression
```

## Features

| Feature | Description |
|---------|-------------|
| **Direct Download** | Downloads from Microsoft CDN, no VS Installer needed |
| **Resumable** | Interrupted downloads can be resumed |
| **Cached** | Manifests and packages are cached locally |
| **Multi-version** | Install multiple MSVC/SDK versions side by side |
| **Multi-arch** | Support x64, x86, arm64, arm targets |
| **Library API** | Use programmatically in Rust projects |
| **Shell Scripts** | Generate activation scripts for any shell |

## Components Downloaded

### MSVC Compiler
- `cl.exe` - C/C++ compiler
- `link.exe` - Linker
- `lib.exe` - Static library manager
- `ml64.exe` - MASM assembler
- Headers and libraries

### Windows SDK
- Windows headers
- Import libraries
- `rc.exe` - Resource compiler
- Debugging tools

## Use Cases

### Rust Development

msvc-kit is perfect for Rust development on Windows. The `cc` crate and `rustc` will automatically find the configured MSVC toolchain.

```bash
msvc-kit setup --script --shell powershell | Invoke-Expression
cargo build  # Works!
```

### CI/CD Pipelines

```yaml
# GitHub Actions example
- name: Install MSVC
  run: |
    cargo install msvc-kit
    msvc-kit download
    msvc-kit setup --script --shell powershell | Invoke-Expression
```

### DCC Plugin Development

Build plugins for Maya, Houdini, 3ds Max, Unreal Engine without installing Visual Studio:

```bash
msvc-kit download --msvc-version 14.38  # Match your DCC's compiler
msvc-kit setup --script --shell powershell | Invoke-Expression
```

## Next Steps

- [Getting Started](./getting-started.md) - Install and first use
- [Installation](./installation.md) - Detailed installation options
- [CLI Download](./cli-download.md) - Download command reference
