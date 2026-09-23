# 库 API 概述

msvc-kit 可以作为 Rust 库使用，以编程方式访问 MSVC 工具链管理功能。

## 安装

添加到你的 `Cargo.toml`：

```toml
[dependencies]
msvc-kit = "0.2"
tokio = { version = "1", features = ["full"] }
```

## 快速示例

```rust
use msvc_kit::{
    download_msvc, download_sdk, extract_and_finalize_msvc, extract_and_finalize_sdk,
    setup_environment, DownloadOptions,
};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    // 使用默认选项下载
    let options = DownloadOptions::default();
    
    let mut msvc_info = download_msvc(&options).await?;
    let sdk_info = download_sdk(&options).await?;
    extract_and_finalize_msvc(&mut msvc_info).await?;
    extract_and_finalize_sdk(&sdk_info).await?;
    
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

### 版本发现函数

```rust
/// 从微软服务器获取可用版本
pub async fn list_available_versions() -> Result<AvailableVersions>;

/// 可用版本信息
pub struct AvailableVersions {
    pub msvc_versions: Vec<String>,  // 例如 ["14.44", "14.43", "14.42"]
    pub sdk_versions: Vec<String>,   // 例如 ["10.0.26100.0", "10.0.22621.0"]
    pub latest_msvc: Option<String>, // 例如 Some("14.44")
    pub latest_sdk: Option<String>,  // 例如 Some("10.0.26100.0")
    /// 提供这些版本的 Visual Studio channel，例如 Some("Visual Studio 2026 (v18)")。
    /// 自动选择时具体 channel 取决于上游已发布的内容。
    pub channel: Option<String>,
}
```

### 下载函数

```rust
/// 下载 MSVC 编译器组件
pub async fn download_msvc(options: &DownloadOptions) -> Result<InstallInfo>;

/// 下载 Windows SDK 组件
pub async fn download_sdk(options: &DownloadOptions) -> Result<InstallInfo>;
```

`download_msvc` 和 `download_sdk` 只负责获取 payload。把返回的
`InstallInfo` 传给 `setup_environment` 前，需要先调用
`extract_and_finalize_msvc` / `extract_and_finalize_sdk` 完成解压和路径确认。

```rust
/// 解压已下载的 MSVC payload，并更新完整 MSVC 版本
pub async fn extract_and_finalize_msvc(info: &mut InstallInfo) -> Result<()>;

/// 解压已下载的 Windows SDK payload
pub async fn extract_and_finalize_sdk(info: &InstallInfo) -> Result<()>;
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

/// 加载已保存的设置，不应用临时环境变量覆盖
pub fn load_persisted_config() -> Result<MsvcKitConfig>;

/// 保存配置到磁盘
pub fn save_config(config: &MsvcKitConfig) -> Result<()>;
```

### 配置文件位置

```rust
/// 配置文件名
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// 切换到便携模式的标记文件
pub const PORTABLE_MARKER_FILE: &str = "msvc-kit.portable";

/// 指定配置文件或目录的环境变量
pub const CONFIG_ENV_VAR: &str = "MSVC_KIT_CONFIG";

/// 启用便携模式的环境变量
pub const PORTABLE_ENV_VAR: &str = "MSVC_KIT_PORTABLE";

/// 解析后的配置文件位置
///
/// 优先级：`--config` / `set_config_path_override` > `MSVC_KIT_CONFIG`
/// > 便携模式（可执行文件旁的 config.toml）> 每用户配置目录。
pub fn get_config_path() -> PathBuf;

/// 可执行文件所在目录
pub fn exe_dir() -> Option<PathBuf>;

/// 是否处于便携模式
pub fn is_portable_mode() -> bool;

/// 为当前进程固定配置文件（CLI 的 `--config`）
pub fn set_config_path_override(path: impl Into<PathBuf>);

/// 已设置的显式配置文件路径
pub fn config_path_override() -> Option<PathBuf>;

/// 清除显式配置文件路径
pub fn clear_config_path_override();

/// 在可执行文件旁创建便携标记文件
pub fn enable_portable_mode() -> Result<()>;

/// 删除便携标记文件
pub fn disable_portable_mode() -> Result<()>;
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
        Err(MsvcKitError::Network(e)) => eprintln!("网络错误: {}", e),
        Err(MsvcKitError::VersionNotFound(v)) => eprintln!("版本未找到: {}", v),
        Err(e) => eprintln!("错误: {}", e),
    }
}
```

## Feature Flags

msvc-kit 提供可选的 features 以减少依赖冲突：

| Feature | 默认 | 引入依赖 |
|---------|------|----------|
| `self-update` | 是 | [axoupdater](https://github.com/axodotdev/axoupdater) |
| `native-tls` | 是 | `reqwest/native-tls`（Windows 上为 SChannel） |
| `rustls-tls` | 否 | `reqwest/rustls` |

### `self-update`（默认启用）

启用 `msvc-kit update` 命令，它通过 `axoupdater` 查询 GitHub Releases。
可执行文件本身只在该 feature 启用时构建，因此库使用者可以去掉它。

```toml
# 包含 self-update（默认）
[dependencies]
msvc-kit = "0.2"

# 或显式启用
[dependencies]
msvc-kit = { version = "0.2", features = ["self-update"] }
```

### 仅库使用（无自更新）

```toml
[dependencies]
msvc-kit = { version = "0.2", default-features = false }
```

## 线程安全

- `DownloadOptions`、`InstallInfo`、`MsvcEnvironment` 是 `Send + Sync`
- 下载函数是异步的，可以从任何运行时调用
- 配置保存在单个 TOML 文件中且没有加锁，因此多个进程同时执行
  `msvc-kit config` 写入时可能互相覆盖

## 下一步

- [DownloadOptions](./download-options.md) - 配置下载
- [InstallInfo](./install-info.md) - 访问安装详情
- [MsvcEnvironment](./msvc-environment.md) - 环境配置
- [ToolPaths](./tool-paths.md) - 访问工具可执行文件
