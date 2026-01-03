# msvc-kit

[![Crates.io](https://img.shields.io/crates/v/msvc-kit.svg)](https://crates.io/crates/msvc-kit)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml)

[English](#english) | [中文](#中文)

---

## English

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

Config file: `%LOCALAPPDATA%\loonghao\msvc-kit\config\config.json`

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

    // Environment variables are now set
    println!("VCINSTALLDIR: {:?}", env.vc_install_dir);
    println!("WindowsSdkDir: {:?}", env.windows_sdk_dir);

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

---

## 中文

面向 Rust/Windows 的便携式 MSVC 构建工具安装器与管理器。

### 三步上手

```bash
cargo install msvc-kit
msvc-kit download
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### 功能特性

- **下载 MSVC 编译器** - 从微软服务器下载
- **下载 Windows SDK** - 到指定目录
- **自动配置环境** - 兼容 `cc-rs`
- **多版本多架构管理** - 支持 x64、x86、arm64、arm
- **提供库 API** - 可编程使用
- **断点续传** - 基于 redb 索引快速跳过
- **清单缓存** - 支持 ETag/Last-Modified 条件请求
- **多格式解压** - 支持 VSIX、MSI、CAB
- **哈希校验** - SHA256 验证

### 安装

- **crates.io**：`cargo install msvc-kit`
- **源码**：`git clone ... && cargo install --path .`

### CLI 命令

#### 下载

```bash
# 下载最新版本
msvc-kit download

# 指定版本/目录/架构
msvc-kit download \
  --msvc-version 14.44 \
  --sdk-version 10.0.26100.0 \
  --target C:\msvc-kit \
  --arch x64 --host-arch x64

# 仅下载 MSVC（跳过 SDK）
msvc-kit download --no-sdk

# 仅下载 SDK（跳过 MSVC）
msvc-kit download --no-msvc

# 控制并行下载数（默认 4）
msvc-kit download --parallel-downloads 8

# 跳过哈希校验
msvc-kit download --no-verify
```

#### 配置环境

```bash
# PowerShell
msvc-kit setup --script --shell powershell | Invoke-Expression

# CMD
msvc-kit setup --script --shell cmd > setup.bat && setup.bat

# Bash/WSL
eval "$(msvc-kit setup --script --shell bash)"

# 写入注册表（需要管理员权限）
msvc-kit setup --persistent
```

#### 查看版本

```bash
msvc-kit list              # 已安装版本
msvc-kit list --available  # 可用版本
```

#### 清理

```bash
msvc-kit clean --msvc-version 14.44   # 删除指定 MSVC 版本
msvc-kit clean --sdk-version 10.0.26100.0  # 删除指定 SDK 版本
msvc-kit clean --all                  # 删除所有版本
msvc-kit clean --all --cache          # 同时清理下载缓存
```

#### 配置

配置文件位置：`%LOCALAPPDATA%\loonghao\msvc-kit\config\config.json`

```bash
msvc-kit config                        # 显示当前配置
msvc-kit config --set-dir C:\msvc-kit  # 设置安装目录
msvc-kit config --set-msvc 14.44       # 设置默认 MSVC 版本
msvc-kit config --set-sdk 10.0.26100.0 # 设置默认 SDK 版本
msvc-kit config --reset                # 重置为默认值
```

#### 打印环境变量

```bash
msvc-kit env                  # 输出为 shell 脚本
msvc-kit env --format json    # 输出为 JSON
```

### 缓存机制

| 缓存类型 | 位置 | 说明 |
|----------|------|------|
| 下载索引 | `downloads/{msvc\|sdk}/.../index.db` | redb 数据库，跟踪下载状态 |
| 清单缓存 | `cache/manifests/` | VS 清单缓存，支持 ETag/Last-Modified |
| 解压标记 | `.msvc-kit-extracted/` | 跳过已解压的包 |

- **进度显示**：默认单行转圈；设置 `MSVC_KIT_INNER_PROGRESS=1` 显示详细文件进度
- **跳过逻辑**：
  - `cached`：索引中存在且哈希匹配
  - `304`：服务器返回未修改（ETag/Last-Modified 匹配）
  - `size match`：文件大小匹配（尽力而为，代码中有注释说明）

### 库用法

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

    println!("VCINSTALLDIR: {:?}", env.vc_install_dir);
    Ok(())
}
```

### 设置的环境变量

| 变量 | 说明 |
|------|------|
| `VCINSTALLDIR` | VC 安装目录 |
| `VCToolsInstallDir` | VC 工具安装目录 |
| `VCToolsVersion` | VC 工具版本 |
| `WindowsSdkDir` | Windows SDK 目录 |
| `WindowsSDKVersion` | Windows SDK 版本 |
| `WindowsSdkBinPath` | Windows SDK bin 路径 |
| `INCLUDE` | 编译器包含路径 |
| `LIB` | 链接器库路径 |
| `PATH` | 更新后的路径（包含编译器/SDK bin 目录） |
| `Platform` | 目标平台 |

### 架构支持

| 架构 | 主机 | 目标 | 说明 |
|------|------|------|------|
| `x64` | ✓ | ✓ | 64 位 x86 |
| `x86` | ✓ | ✓ | 32 位 x86 |
| `arm64` | ✓ | ✓ | ARM64 |
| `arm` | - | ✓ | ARM 32 位（仅目标） |

### 许可证

MIT，参见 `LICENSE`。
