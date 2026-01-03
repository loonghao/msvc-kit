# 库 API 概述

msvc-kit 可以作为 Rust 库使用，以编程方式访问 MSVC 工具链管理功能。

## 安装

添加到你的 `Cargo.toml`：

```toml
[dependencies]
msvc-kit = "0.1"
tokio = { version = "1", features = ["full"] }
```

## 快速示例

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    // 使用默认选项下载
    let options = DownloadOptions::default();
    
    let msvc_info = download_msvc(&options).await?;
    let sdk_info = download_sdk(&options).await?;
    
    // 设置环境
    let env = setup_environment(&msvc_info, Some(&sdk_info))?;
    
    // 访问路径
    println!("cl.exe: {:?}", env.cl_exe_path());
    println!("INCLUDE: {}", env.include_path_string());
    println!("LIB: {}", env.lib_path_string());
    
    Ok(())
}
```

## 主要类型

### 下载函数

```rust
/// 下载 MSVC 编译器组件
pub async fn download_msvc(options: &DownloadOptions) -> Result<InstallInfo>;

/// 下载 Windows SDK 组件
pub async fn download_sdk(options: &DownloadOptions) -> Result<InstallInfo>;
```

### 环境函数

```rust
/// 从安装信息设置环境
pub fn setup_environment(
    msvc_info: &InstallInfo,
    sdk_info: Option<&InstallInfo>,
) -> Result<MsvcEnvironment>;

/// 生成 shell 激活脚本
pub fn generate_activation_script(
    env: &MsvcEnvironment,
    shell: ShellType,
) -> String;

/// 获取环境变量为 HashMap
pub fn get_env_vars(env: &MsvcEnvironment) -> HashMap<String, String>;
```

### 配置函数

```rust
/// 从磁盘加载配置
pub fn load_config() -> Result<MsvcKitConfig>;

/// 保存配置到磁盘
pub fn save_config(config: &MsvcKitConfig) -> Result<()>;
```

## 重新导出的类型

```rust
pub use config::MsvcKitConfig;
pub use downloader::DownloadOptions;
pub use env::{MsvcEnvironment, ShellType, ToolPaths};
pub use error::{MsvcKitError, Result};
pub use installer::InstallInfo;
pub use version::{Architecture, MsvcVersion, SdkVersion};
```

## 错误处理

所有函数返回 `msvc_kit::Result<T>`：

```rust
use msvc_kit::{download_msvc, DownloadOptions, MsvcKitError};

async fn example() {
    let options = DownloadOptions::default();
    
    match download_msvc(&options).await {
        Ok(info) => println!("安装到 {:?}", info.install_path),
        Err(MsvcKitError::NetworkError(e)) => eprintln!("网络错误: {}", e),
        Err(MsvcKitError::VersionNotFound(v)) => eprintln!("版本未找到: {}", v),
        Err(e) => eprintln!("错误: {}", e),
    }
}
```

## Feature Flags

msvc-kit 提供可选的 features 以减少依赖冲突：

### `self-update`（默认启用）

启用 CLI 自更新功能。此 feature 包含 `self_update` crate，它依赖于 `lzma-sys`。

```toml
# 包含 self-update（默认）
[dependencies]
msvc-kit = "0.1"

# 或显式启用
[dependencies]
msvc-kit = { version = "0.1", features = ["self-update"] }
```

### 仅库使用（无自更新）

如果你将 msvc-kit 作为库使用，并遇到依赖冲突（例如与 `liblzma-sys`），可以禁用默认 features：

```toml
[dependencies]
msvc-kit = { version = "0.1", default-features = false }
```

这在将 msvc-kit 集成到使用不同 LZMA 实现的项目时很有用，可以避免 `lzma-sys` 冲突：

```
error: the crate `lzma` is compiled multiple times, possibly with different configurations
  - crate `liblzma_sys` links to native library `lzma`
  - crate `lzma_sys` links to native library `lzma`
```

## 线程安全

- `DownloadOptions`、`InstallInfo`、`MsvcEnvironment` 是 `Send + Sync`
- 下载函数是异步的，可以从任何运行时调用
- 配置函数使用文件锁以支持并发访问

## 下一步

- [DownloadOptions](./download-options.md) - 配置下载
- [InstallInfo](./install-info.md) - 访问安装详情
- [MsvcEnvironment](./msvc-environment.md) - 环境配置
- [ToolPaths](./tool-paths.md) - 访问工具可执行文件
