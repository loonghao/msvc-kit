---
layout: home

hero:
  name: msvc-kit
  text: Portable MSVC Build Tools
  tagline: Download and manage MSVC compiler without Visual Studio installation
  image:
    src: /logo.svg
    alt: msvc-kit
  actions:
    - theme: brand
      text: Get Started
      link: /guide/getting-started
    - theme: alt
      text: View on GitHub
      link: https://github.com/loonghao/msvc-kit

features:
  - icon: 🚀
    title: Fast Download
    details: Download MSVC compiler and Windows SDK directly from Microsoft servers with resumable downloads and parallel processing.
  - icon: 📦
    title: Portable
    details: No Visual Studio installation required. Perfect for CI/CD pipelines and lightweight development environments.
  - icon: 🔧
    title: Easy Setup
    details: One command to configure environment variables for cc-rs, CMake, and other build tools.
  - icon: 🎮
    title: DCC Ready
    details: Pre-configured for Unreal Engine, Maya, Houdini, 3ds Max and other DCC applications.
  - icon: 📚
    title: Library API
    details: Programmatic access via Rust library for custom build pipelines and tooling.
  - icon: ⚡
    title: Smart Caching
    details: ETag/Last-Modified caching for manifests, redb-based download index for fast skip.
---

## Quick Start

```bash
# Install
cargo install msvc-kit

# Download latest MSVC + Windows SDK
msvc-kit download

# Setup environment (PowerShell)
msvc-kit setup --script --shell powershell | Invoke-Expression

# Now you can compile!
cl /help
```

## Use as Library

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Get tool paths
    println!("cl.exe: {:?}", env.cl_exe_path());
    println!("INCLUDE: {}", env.include_path_string());
    
    Ok(())
}
```
