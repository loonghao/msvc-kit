# msvc-kit

[![Crates.io](https://img.shields.io/crates/v/msvc-kit.svg)](https://crates.io/crates/msvc-kit)
[![Crates.io Downloads](https://img.shields.io/crates/d/msvc-kit.svg)](https://crates.io/crates/msvc-kit)
[![GitHub Downloads](https://img.shields.io/github/downloads/loonghao/msvc-kit/total.svg)](https://github.com/loonghao/msvc-kit/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/loonghao/msvc-kit/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/loonghao/msvc-kit/graph/badge.svg)](https://codecov.io/gh/loonghao/msvc-kit)
[![docs.rs](https://img.shields.io/docsrs/msvc-kit)](https://docs.rs/msvc-kit)

[English](README.md) | 中文

---

面向 Rust/Windows 的便携式 MSVC 构建工具安装器与管理器。

## 三步上手

```bash
# 安装 CLI
cargo install msvc-kit

# 下载最新 MSVC + Windows SDK 到默认数据目录
msvc-kit download

# 应用环境到当前 shell (PowerShell)
msvc-kit setup --script --shell powershell | Invoke-Expression
```

## 功能特性

- **下载 MSVC 编译器** - 从微软服务器下载
- **下载 Windows SDK** - 到指定目录
- **自动配置环境** - 兼容 `cc-rs`
- **多版本多架构管理** - 支持 x64、x86、arm64、arm
- **提供库 API** - 可编程使用
- **断点续传** - 基于 redb 索引快速跳过
- **清单缓存** - 支持 ETag/Last-Modified 条件请求
- **TLS 后端** - 使用 `native-tls`（Windows schannel），避免 `rustls`/`awslc-sys` 构建问题
- **多格式解压** - 支持 VSIX、MSI、CAB
- **哈希校验** - SHA256 验证
- **自动更新** - 基于 [axoupdater](https://github.com/axodotdev/axoupdater)，兼容 cargo-dist 发布流程


## 安装

- **通过 Winget（推荐）**
  ```powershell
  winget install loonghao.msvc-kit
  ```
- **通过 PowerShell 脚本**
  ```powershell
  irm https://github.com/loonghao/msvc-kit/releases/latest/download/install.ps1 | iex
  ```
- **从 crates.io**
  ```bash
  cargo install msvc-kit
  ```
- **预编译二进制文件**
  ```powershell
  # 下载并解压到 PATH 中的目录
  Invoke-WebRequest -Uri "https://github.com/loonghao/msvc-kit/releases/latest/download/msvc-kit-x86_64-pc-windows-msvc.zip" -OutFile msvc-kit.zip
  Expand-Archive msvc-kit.zip -DestinationPath $env:USERPROFILE\.cargo\bin -Force
  ```
- **从源码**
  ```bash
  git clone https://github.com/loonghao/msvc-kit.git
  cd msvc-kit
  cargo install --path .
  ```

## 发布的 Bundle
- 每次打 tag（或 release-please 生成的发布）时，CI 会为 `x64`、`x86`、`arm64` 架构构建并上传 `msvc-bundle-<msvc>-<sdk>-<arch>.zip` 到对应的 GitHub Release。
- Bundle 由 `msvc-kit bundle --accept-license` 创建，下载即表示你接受 [Microsoft Visual Studio License Terms](https://visualstudio.microsoft.com/license-terms/)。

## 快速开始 (CLI)



### 下载

```bash
# 下载最新版本
msvc-kit download

# 指定版本/目录/架构
# MSVC 版本可以是短格式 (14.44) 或完整格式 (14.44.34823)
msvc-kit download \
  --msvc-version 14.44 \
  --sdk-version 10.0.26100.0 \
  --target C:\msvc-kit \
  --arch x64 \
  --host-arch x64

# 仅下载 MSVC（跳过 SDK）
msvc-kit download --no-sdk

# 仅下载 SDK（跳过 MSVC）
msvc-kit download --no-msvc

# 控制并行下载数（默认 4）
msvc-kit download --parallel-downloads 8

# 跳过哈希校验
msvc-kit download --no-verify
```

> **注意：** MSVC 版本可以使用短格式（如 `14.44`），会自动解析到最新构建版本；也可以使用完整格式（如 `14.44.34823`）指定特定构建。

**版本兼容性速查：**

| 场景 | MSVC | SDK | 命令 |
|------|------|-----|------|
| 最新版（推荐） | `14.44` | `10.0.26100.0` | `msvc-kit download` |
| Windows 11 开发 | `14.42`+ | `10.0.22621.0`+ | `msvc-kit download --sdk-version 10.0.22621.0` |
| 最大 Win10 兼容性 | `14.40` | `10.0.19041.0` | `msvc-kit download --msvc-version 14.40 --sdk-version 10.0.19041.0` |

详见 [版本兼容性指南](docs/guide/cli-download.md#version-compatibility-guide)。

### 配置环境

```bash
# 为当前 shell 生成脚本
msvc-kit setup --script --shell powershell | Invoke-Expression

# 或者 CMD
msvc-kit setup --script --shell cmd > setup.bat && setup.bat

# 生成可移植脚本（将安装根替换为 %~dp0runtime）
msvc-kit setup --script --shell cmd --portable-root "%~dp0runtime" > setup.bat

# 或者 Bash/WSL
eval "$(msvc-kit setup --script --shell bash)"

# 持久化到 Windows 注册表（需要管理员权限）
msvc-kit setup --persistent
```


### 创建可移植 Bundle

创建包含 MSVC 工具链的独立 bundle，可在任何地方使用：

```bash
# 创建 bundle（需要接受微软许可证）
msvc-kit bundle --accept-license

# 指定输出目录和架构
msvc-kit bundle --accept-license --output ./my-msvc-bundle --arch x64

# 交叉编译 bundle（x64 主机编译 ARM64 目标）
msvc-kit bundle --accept-license --host-arch x64 --arch arm64

# 同时创建 zip 压缩包
msvc-kit bundle --accept-license --zip

# 指定版本
msvc-kit bundle --accept-license --msvc-version 14.44 --sdk-version 10.0.26100.0
```

Bundle 包含：
- `msvc-kit.exe` - CLI 工具
- `VC/Tools/MSVC/{version}/` - MSVC 编译器和工具
- `Windows Kits/10/` - Windows SDK
- `setup.bat` - CMD 激活脚本
- `setup.ps1` - PowerShell 激活脚本
- `setup.sh` - Bash/WSL 激活脚本
- `README.txt` - 使用说明

使用方法：
```bash
# 解压并运行 setup 脚本
cd msvc-bundle
setup.bat          # CMD
.\setup.ps1        # PowerShell
source setup.sh    # Bash/WSL

# 现在 cl, link, nmake 可用了
cl /nologo test.c
```


### 查看版本

```bash
msvc-kit list              # 显示已安装版本
msvc-kit list --available  # 显示微软可用版本
```

### 清理

```bash
msvc-kit clean --msvc-version 14.44   # 删除指定 MSVC 版本
msvc-kit clean --sdk-version 10.0.26100.0  # 删除指定 SDK 版本
msvc-kit clean --all                  # 删除所有已安装版本
msvc-kit clean --all --cache          # 同时清理下载缓存
```

### 配置

配置文件位置：`%LOCALAPPDATA%\loonghao\msvc-kit\config\config.toml`

```bash
msvc-kit config                        # 显示当前配置
msvc-kit config --set-dir C:\msvc-kit  # 设置安装目录
msvc-kit config --set-msvc 14.44       # 设置默认 MSVC 版本
msvc-kit config --set-sdk 10.0.26100.0 # 设置默认 SDK 版本
msvc-kit config --reset                # 重置为默认值
```

### 打印环境变量

```bash
msvc-kit env                  # 输出为 shell 脚本
msvc-kit env --format json    # 输出为 JSON
```

### 自动更新

```bash
# 仅检查更新，不安装
msvc-kit update --check

# 更新到最新版本
msvc-kit update

# 更新到指定版本
msvc-kit update --version 0.2.5
```

自动更新功能由 [axoupdater](https://github.com/axodotdev/axoupdater) 驱动，直接查询 GitHub Releases。兼容 cargo-dist 和自定义发布流程。`self-update` 特性默认启用，构建时可通过 `--no-default-features` 禁用。

## 缓存机制

| 缓存类型 | 位置 | 说明 |
|----------|------|------|
| 下载索引 | `downloads/{msvc\|sdk}/.../index.db` | redb 数据库，跟踪下载状态 |
| 清单缓存 | `cache/manifests/` | VS 清单缓存，支持 ETag/Last-Modified |
| 解压标记 | `.msvc-kit-extracted/` | 跳过已解压的包 |

- **进度显示**：默认单行转圈。设置 `MSVC_KIT_INNER_PROGRESS=1` 显示详细文件进度。
- **跳过逻辑**：以下情况会跳过下载：
  - `cached`：索引中存在且哈希匹配
  - `304`：服务器返回未修改（ETag/Last-Modified 匹配）
  - `size match`：文件大小匹配（尽力而为，代码中有注释说明）

## 库用法

```toml
[dependencies]
msvc-kit = "0.1"
```

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use msvc_kit::{list_available_versions, Architecture};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    // 列出微软可用版本
    let versions = list_available_versions().await?;
    println!("最新 MSVC: {:?}", versions.latest_msvc);
    println!("最新 SDK: {:?}", versions.latest_sdk);

    // 使用 Builder 模式下载
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

详见 [库 API 文档](docs/api/library.md)。

## 架构支持

| 架构 | 主机 | 目标 | 说明 |
|------|------|------|------|
| `x64` | ✓ | ✓ | 64 位 x86 |
| `x86` | ✓ | ✓ | 32 位 x86 |
| `arm64` | ✓ | ✓ | ARM64 |
| `arm` | - | ✓ | ARM 32 位（仅目标） |

## 许可证

MIT 许可证 - 参见 `LICENSE`。

**重要：微软软件许可声明**

本工具下载的 MSVC 编译器和 Windows SDK 是微软的财产，
受 [Microsoft Visual Studio 许可条款](https://visualstudio.microsoft.com/license-terms/) 约束。

- **msvc-kit** 本身采用 MIT 许可证
- MSVC Build Tools 和 Windows SDK **不可再分发** - 用户必须直接下载
- 使用 `msvc-kit download` 或 `msvc-kit bundle --accept-license` 即表示您同意微软的许可条款
- 本工具仅自动化下载过程，不分发微软软件
