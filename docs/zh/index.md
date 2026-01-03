---
layout: home

hero:
  name: msvc-kit
  text: 便携式 MSVC 构建工具
  tagline: 无需安装 Visual Studio，下载和管理 MSVC 编译器
  image:
    src: /logo.svg
    alt: msvc-kit
  actions:
    - theme: brand
      text: 快速开始
      link: /zh/guide/getting-started
    - theme: alt
      text: 在 GitHub 上查看
      link: https://github.com/loonghao/msvc-kit

features:
  - icon: 🚀
    title: 快速下载
    details: 直接从微软服务器下载 MSVC 编译器和 Windows SDK，支持断点续传和并行下载。
  - icon: 📦
    title: 便携式
    details: 无需安装 Visual Studio。非常适合 CI/CD 流水线和轻量级开发环境。
  - icon: 🔧
    title: 简单配置
    details: 一条命令即可配置 cc-rs、CMake 等构建工具所需的环境变量。
  - icon: 🎮
    title: DCC 就绪
    details: 预配置支持 Unreal Engine、Maya、Houdini、3ds Max 等 DCC 应用程序。
  - icon: 📚
    title: 库 API
    details: 通过 Rust 库进行编程访问，用于自定义构建流水线和工具。
  - icon: ⚡
    title: 智能缓存
    details: 清单支持 ETag/Last-Modified 缓存，基于 redb 的下载索引实现快速跳过。
---

## 快速开始

```powershell
# 通过 Winget 安装（推荐）
winget install loonghao.msvc-kit

# 或通过 PowerShell 脚本安装
irm https://github.com/loonghao/msvc-kit/releases/latest/download/install.ps1 | iex

# 或通过 Cargo 安装
cargo install msvc-kit

# 下载最新的 MSVC + Windows SDK
msvc-kit download

# 配置环境 (PowerShell)
msvc-kit setup --script --shell powershell | Invoke-Expression

# 现在可以编译了！
cl /help
```

## 作为库使用

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // 获取工具路径
    println!("cl.exe: {:?}", env.cl_exe_path());
    println!("INCLUDE: {}", env.include_path_string());
    
    Ok(())
}
```
