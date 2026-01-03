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
- **Version management** for host/target architectures
- **Library API** for programmatic usage
- **Resumable downloads & cached extraction** to save time

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

- **Download**
  ```bash
  # Latest
  msvc-kit download

  # Specify versions / dirs / arch
  msvc-kit download \
    --msvc-version 14.44 \
    --sdk-version 10.0.26100.0 \
    --target C:\msvc-kit \
    --arch x64 \
    --host-arch x64
  ```
- **Setup environment**
  ```bash
  msvc-kit setup --script --shell powershell | Invoke-Expression
  # Or persist to registry
  msvc-kit setup --persistent
  ```
- **List**
  ```bash
  msvc-kit list            # installed
  msvc-kit list --available
  ```
- **Clean**
  ```bash
  msvc-kit clean --msvc-version 14.44
  msvc-kit clean --all --cache
  ```
- **Config** (stored under `%LOCALAPPDATA%\loonghao\msvc-kit\config.json`)
  ```bash
  msvc-kit config
  msvc-kit config --set-dir C:\msvc-kit
  msvc-kit config --set-msvc 14.44 --set-sdk 10.0.26100.0
  msvc-kit config --reset
  ```

### Caching & Progress

- **Download cache** lives in `.../downloads/{msvc|sdk}/.../` with `index.db`.
- **Extraction cache** uses markers under `.msvc-kit-extracted/`; reruns skip already-extracted packages.
- **Progress display**: single-line spinner for overall extract. To see inner file progress bars, set `MSVC_KIT_INNER_PROGRESS=1`.
- **Hash verification**: enabled by default; disable with `--no-verify-hashes`.

### Library Usage

```toml
[dependencies]
msvc-kit = "0.1"
```

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = DownloadOptions {
        target_dir: std::path::PathBuf::from("C:/msvc-kit"),
        ..Default::default()
    };
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    println!("cl.exe available: {}", env.has_cl_exe());
    Ok(())
}
```

### Using with VitePress (docs site)

- You can reuse this `README.md` as the VitePress home page by placing a copy at `docs/index.md`.
- Minimal setup example:
  ```bash
  npm create vitepress@latest docs
  # or: pnpm dlx vitepress init docs
  # then copy README
  cp README.md docs/index.md
  npm install && npm run docs:dev
  ```
- Adjust sidebar/nav in `docs/.vitepress/config.ts` as needed; no code changes in `msvc-kit` are required.

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

- 下载 MSVC 编译器
- 下载 Windows SDK
- 自动配置环境，兼容 `cc-rs`
- 多版本、多架构管理
- 提供库 API
- 下载可续传，解压有缓存

### 安装

- **crates.io**：`cargo install msvc-kit`
- **源码**：`git clone ... && cargo install --path .`

### CLI 快速使用

```bash
# 下载最新
msvc-kit download

# 指定版本/目录/架构
msvc-kit download \
  --msvc-version 14.44 \
  --sdk-version 10.0.26100.0 \
  --target C:\msvc-kit \
  --arch x64 --host-arch x64

# 配置环境（当前 PowerShell 会话）
msvc-kit setup --script --shell powershell | Invoke-Expression
# 或写入注册表
msvc-kit setup --persistent

# 查看
msvc-kit list
msvc-kit list --available

# 清理
msvc-kit clean --msvc-version 14.44
msvc-kit clean --all --cache

# 配置文件位置
# %LOCALAPPDATA%\loonghao\msvc-kit\config.json
msvc-kit config --set-dir C:\msvc-kit
msvc-kit config --reset
```

### 缓存与进度

- 下载缓存：`downloads/{msvc|sdk}/.../index.db`，断点续跑会跳过已完成文件。
- 解压缓存：`.msvc-kit-extracted/` 标记，重复执行会直接跳过已解压包。
- 进度：默认仅一条总进度转圈；需要查看内部文件进度时设置环境变量 `MSVC_KIT_INNER_PROGRESS=1`。
- 哈希校验：默认开启，若需要关闭用 `--no-verify-hashes`。

### 库用法

```toml
[dependencies]
msvc-kit = "0.1"
```

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
# tokio::main omitted for brevity
```

### VitePress 配置提示

- 可直接复制 `README.md` 到 `docs/index.md` 作为首页。
- 初始化示例：`npm create vitepress@latest docs`，复制 README 后 `npm install && npm run docs:dev`。
- 侧边栏/导航在 `docs/.vitepress/config.ts` 中按需配置即可。

### 许可证

MIT，参见 `LICENSE`。
