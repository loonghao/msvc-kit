# msvc-kit

[![Crates.io](https://img.shields.io/crates/v/msvc-kit.svg)](https://crates.io/crates/msvc-kit)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml)

[English](#english) | [中文](#中文)

---

## English

A portable MSVC Build Tools installer and manager for Rust development.

### Features

- 📦 **Download MSVC Compiler** - Download MSVC compiler components from Microsoft servers
- 🪟 **Download Windows SDK** - Download Windows SDK to specified directories
- ⚙️ **Auto Environment Setup** - Configure environment variables for immediate use
- 🦀 **cc-rs Compatible** - Works seamlessly with Rust's cc-rs crate
- 📚 **Library API** - Use as a crate in your Rust projects
- 🔄 **Version Management** - Support multiple versions, easy switching

### Installation

#### From crates.io

```bash
cargo install msvc-kit
```

#### From winget (coming soon)

```bash
winget install msvc-kit
```

#### From source

```bash
git clone https://github.com/loonghao/msvc-kit.git
cd msvc-kit
cargo install --path .
```

### Quick Start

#### Download and Install

```bash
# Download latest MSVC and Windows SDK
msvc-kit download

# Download specific versions
msvc-kit download --msvc-version 14.40 --sdk-version 10.0.22621.0

# Download to custom directory
msvc-kit download --target C:\msvc-tools
```

#### Setup Environment

```bash
# Generate activation script for PowerShell
msvc-kit setup --script --shell powershell | Invoke-Expression

# Or save to file
msvc-kit setup --script --shell powershell > activate.ps1
. .\activate.ps1

# For CMD
msvc-kit setup --script --shell cmd > activate.bat
activate.bat

# Persistent setup (writes to Windows registry)
msvc-kit setup --persistent
```

#### List Installed Versions

```bash
# Show installed versions
msvc-kit list

# Show available versions from Microsoft
msvc-kit list --available
```

#### Clean Up

```bash
# Remove specific version
msvc-kit clean --msvc-version 14.40

# Remove all installations
msvc-kit clean --all

# Also remove download cache
msvc-kit clean --all --cache
```

### Library Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
msvc-kit = "0.1"
```

Example:

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = DownloadOptions {
        msvc_version: None, // Use latest
        sdk_version: None,  // Use latest
        target_dir: std::path::PathBuf::from("C:\\msvc-kit"),
        ..Default::default()
    };

    // Download MSVC
    let msvc_info = download_msvc(&options).await?;
    println!("MSVC installed to: {:?}", msvc_info.install_path);

    // Download Windows SDK
    let sdk_info = download_sdk(&options).await?;
    println!("SDK installed to: {:?}", sdk_info.install_path);

    // Setup environment
    let env = setup_environment(&msvc_info, Some(&sdk_info))?;
    println!("cl.exe available: {}", env.has_cl_exe());

    Ok(())
}
```

### Configuration

Configuration file is stored at:
- Windows: `%LOCALAPPDATA%\msvc-kit\config.json`

```bash
# Show current configuration
msvc-kit config

# Set installation directory
msvc-kit config --set-dir C:\msvc-tools

# Set default versions
msvc-kit config --set-msvc 14.40
msvc-kit config --set-sdk 10.0.22621.0

# Reset to defaults
msvc-kit config --reset
```

### Environment Variables

After running `msvc-kit setup`, the following environment variables are configured:

| Variable | Description |
|----------|-------------|
| `VCINSTALLDIR` | Visual C++ installation directory |
| `VCToolsInstallDir` | VC Tools installation directory |
| `VCToolsVersion` | VC Tools version |
| `WindowsSdkDir` | Windows SDK directory |
| `WindowsSDKVersion` | Windows SDK version |
| `INCLUDE` | Include paths for compiler |
| `LIB` | Library paths for linker |
| `PATH` | Binary paths (cl.exe, link.exe, etc.) |

These variables are compatible with Rust's `cc-rs` crate, enabling seamless C/C++ compilation in Rust projects.

---

## 中文

一个便携式的 MSVC 构建工具安装器和管理器，专为 Rust 开发设计。

### 功能特性

- 📦 **下载 MSVC 编译器** - 从微软服务器下载 MSVC 编译器组件
- 🪟 **下载 Windows SDK** - 将 Windows SDK 下载到指定目录
- ⚙️ **自动配置环境** - 配置环境变量，即可使用
- 🦀 **兼容 cc-rs** - 与 Rust 的 cc-rs crate 无缝配合
- 📚 **库 API** - 可作为 crate 在你的 Rust 项目中使用
- 🔄 **版本管理** - 支持多版本，轻松切换

### 安装

#### 从 crates.io 安装

```bash
cargo install msvc-kit
```

#### 从 winget 安装（即将支持）

```bash
winget install msvc-kit
```

#### 从源码安装

```bash
git clone https://github.com/loonghao/msvc-kit.git
cd msvc-kit
cargo install --path .
```

### 快速开始

#### 下载和安装

```bash
# 下载最新的 MSVC 和 Windows SDK
msvc-kit download

# 下载指定版本
msvc-kit download --msvc-version 14.40 --sdk-version 10.0.22621.0

# 下载到自定义目录
msvc-kit download --target C:\msvc-tools
```

#### 配置环境

```bash
# 为 PowerShell 生成激活脚本
msvc-kit setup --script --shell powershell | Invoke-Expression

# 或保存到文件
msvc-kit setup --script --shell powershell > activate.ps1
. .\activate.ps1

# 对于 CMD
msvc-kit setup --script --shell cmd > activate.bat
activate.bat

# 持久化设置（写入 Windows 注册表）
msvc-kit setup --persistent
```

#### 列出已安装版本

```bash
# 显示已安装版本
msvc-kit list

# 显示微软提供的可用版本
msvc-kit list --available
```

#### 清理

```bash
# 删除指定版本
msvc-kit clean --msvc-version 14.40

# 删除所有安装
msvc-kit clean --all

# 同时删除下载缓存
msvc-kit clean --all --cache
```

### 库使用方式

添加到你的 `Cargo.toml`：

```toml
[dependencies]
msvc-kit = "0.1"
```

示例：

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = DownloadOptions {
        msvc_version: None, // 使用最新版本
        sdk_version: None,  // 使用最新版本
        target_dir: std::path::PathBuf::from("C:\\msvc-kit"),
        ..Default::default()
    };

    // 下载 MSVC
    let msvc_info = download_msvc(&options).await?;
    println!("MSVC 安装到: {:?}", msvc_info.install_path);

    // 下载 Windows SDK
    let sdk_info = download_sdk(&options).await?;
    println!("SDK 安装到: {:?}", sdk_info.install_path);

    // 配置环境
    let env = setup_environment(&msvc_info, Some(&sdk_info))?;
    println!("cl.exe 可用: {}", env.has_cl_exe());

    Ok(())
}
```

### 配置

配置文件存储在：
- Windows: `%LOCALAPPDATA%\msvc-kit\config.json`

```bash
# 显示当前配置
msvc-kit config

# 设置安装目录
msvc-kit config --set-dir C:\msvc-tools

# 设置默认版本
msvc-kit config --set-msvc 14.40
msvc-kit config --set-sdk 10.0.22621.0

# 重置为默认值
msvc-kit config --reset
```

### 环境变量

运行 `msvc-kit setup` 后，以下环境变量会被配置：

| 变量 | 描述 |
|------|------|
| `VCINSTALLDIR` | Visual C++ 安装目录 |
| `VCToolsInstallDir` | VC 工具安装目录 |
| `VCToolsVersion` | VC 工具版本 |
| `WindowsSdkDir` | Windows SDK 目录 |
| `WindowsSDKVersion` | Windows SDK 版本 |
| `INCLUDE` | 编译器包含路径 |
| `LIB` | 链接器库路径 |
| `PATH` | 二进制文件路径（cl.exe、link.exe 等） |

这些变量与 Rust 的 `cc-rs` crate 兼容，可在 Rust 项目中无缝编译 C/C++ 代码。

### 与 vx 集成

msvc-kit 设计为可以被 [vx](https://github.com/loonghao/vx) 项目集成使用：

```rust
// 在 vx 中使用 msvc-kit
use msvc_kit::{download_all, setup_environment, DownloadOptions};

async fn setup_msvc_toolchain() -> anyhow::Result<()> {
    let options = DownloadOptions::default();
    let (msvc_info, sdk_info) = download_all(&options).await?;
    let env = setup_environment(&msvc_info, Some(&sdk_info))?;
    
    // 现在可以使用 cl.exe 编译 C/C++ 代码
    Ok(())
}
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

欢迎贡献！请随时提交 Pull Request。

## Acknowledgments

- 灵感来自 [PortableBuildTools](https://github.com/Data-Oriented-House/PortableBuildTools)
- 感谢 Rust 社区的 [cc-rs](https://github.com/rust-lang/cc-rs) crate
