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
- **多格式解压** - 支持 VSIX、MSI、CAB
- **哈希校验** - SHA256 验证

## 安装

- **从 crates.io**
  ```bash
  cargo install msvc-kit
  ```
- **从源码**
  ```bash
  git clone https://github.com/loonghao/msvc-kit.git
  cd msvc-kit
  cargo install --path .
  ```

## 快速开始 (CLI)

### 下载

```bash
# 下载最新版本
msvc-kit download

# 指定版本/目录/架构
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

### 配置环境

```bash
# 为当前 shell 生成脚本
msvc-kit setup --script --shell powershell | Invoke-Expression

# 或者 CMD
msvc-kit setup --script --shell cmd > setup.bat && setup.bat

# 或者 Bash/WSL
eval "$(msvc-kit setup --script --shell bash)"

# 持久化到 Windows 注册表（需要管理员权限）
msvc-kit setup --persistent
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
use msvc_kit::version::Architecture;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    // 使用 Builder 模式配置
    let options = DownloadOptions::builder()
        .target_dir("C:/msvc-kit")
        .arch(Architecture::X64)
        .host_arch(Architecture::X64)
        .verify_hashes(true)
        .parallel_downloads(4)
        .build();

    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;

    // 获取安装路径
    println!("MSVC 安装路径: {:?}", msvc.install_path);
    println!("SDK 安装路径: {:?}", sdk.install_path);

    // 获取目录路径
    println!("MSVC bin 目录: {:?}", msvc.bin_dir());
    println!("MSVC include 目录: {:?}", msvc.include_dir());
    println!("MSVC lib 目录: {:?}", msvc.lib_dir());

    // 获取工具路径
    println!("cl.exe: {:?}", env.cl_exe_path());
    println!("link.exe: {:?}", env.link_exe_path());

    // 获取环境变量字符串
    println!("INCLUDE: {}", env.include_path_string());
    println!("LIB: {}", env.lib_path_string());

    // 导出为 JSON（用于外部工具）
    let json = env.to_json();
    std::fs::write("msvc-env.json", serde_json::to_string_pretty(&json)?)?;

    Ok(())
}
```

## 设置的环境变量

执行 `setup_environment()` 或 `msvc-kit setup` 后：

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
| `Platform` | 目标平台 (x64, x86 等) |

## 架构支持

| 架构 | 主机 | 目标 | 说明 |
|------|------|------|------|
| `x64` | ✓ | ✓ | 64 位 x86 |
| `x86` | ✓ | ✓ | 32 位 x86 |
| `arm64` | ✓ | ✓ | ARM64 |
| `arm` | - | ✓ | ARM 32 位（仅目标） |

## 许可证

MIT 许可证 - 参见 `LICENSE`。
