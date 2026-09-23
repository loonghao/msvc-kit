# DownloadOptions

下载操作的配置选项。

## 定义

```rust
pub struct DownloadOptions {
    /// 安装目标目录
    pub target_dir: PathBuf,
    
    /// 要下载的 MSVC 版本（None = 最新）
    pub msvc_version: Option<String>,
    
    /// 要下载的 SDK 版本（None = 最新）
    pub sdk_version: Option<String>,
    
    /// 目标架构
    pub arch: Architecture,
    
    /// 主机架构（None = 自动检测）
    pub host_arch: Option<Architecture>,
    
    /// 是否验证文件哈希
    pub verify_hashes: bool,
    
    /// 并行下载数量
    pub parallel_downloads: usize,
    
    /// 自定义 HTTP 客户端（None = 使用默认）
    pub http_client: Option<reqwest::Client>,
    
    /// 自定义进度处理器（None = 使用默认 indicatif）
    pub progress_handler: Option<BoxedProgressHandler>,
    
    /// 自定义缓存管理器（None = 使用默认文件系统缓存）
    pub cache_manager: Option<BoxedCacheManager>,
    
    /// 用于发现包的 Visual Studio channel（None = auto）
    ///
    /// 接受大版本号（`17`、`v17`）、发布年份（`2022`）或关键字 `auto` / `latest`。
    /// `None` 表示选择最新且可用的 channel。
    pub vs_channel: Option<String>,
    
    /// 存放 Visual Studio 清单缓存的目录。
    ///
    /// `None` 表示回退到平台默认位置（注入 cache manager 时则使用其目录）。
    /// 传入 `MsvcKitConfig::manifest_cache_dir()` 可让下载器使用配置中的缓存目录。
    pub manifest_cache_dir: Option<PathBuf>,
    
    /// 预览模式：不实际下载
    pub dry_run: bool,

    /// 额外包含的 MSVC 组件（默认空 = 标准安装）
    ///
    /// 默认只下载标准工具链（Tools、CRT、MFC、ATL）。
    /// 通过该字段可以追加 Spectre 缓解库等可选组件。
    pub include_components: HashSet<MsvcComponent>,

    /// 需要排除的包 ID 模式（大小写不敏感子串匹配）
    pub exclude_patterns: Vec<String>,
}
```

## 默认值

`Default::default()` 会读取环境变量，因此它不是一张固定表：

```rust
impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            // `MSVC_KIT_INSTALL_DIR`，否则是相对路径 "msvc-kit"
            target_dir: ...,
            msvc_version: std::env::var("MSVC_KIT_MSVC_VERSION").ok(),
            sdk_version: std::env::var("MSVC_KIT_SDK_VERSION").ok(),
            arch: Architecture::host(),
            host_arch: None,                 // 自动检测
            verify_hashes: true,             // MSVC_KIT_VERIFY_HASHES
            parallel_downloads: 4,           // MSVC_KIT_PARALLEL_DOWNLOADS
            http_client: None,
            progress_handler: None,
            cache_manager: None,
            vs_channel: std::env::var("MSVC_KIT_VS_CHANNEL").ok(),
            manifest_cache_dir: None,        // 平台默认缓存根目录
            dry_run: false,                  // MSVC_KIT_DRY_RUN
            include_components: ...,         // MSVC_KIT_INCLUDE_COMPONENTS
            exclude_patterns: ...,           // MSVC_KIT_EXCLUDE_PATTERNS
        }
    }
}
```

识别的环境变量：

| 变量 | 作用 | 未设置时的默认值 |
|------|------|------------------|
| `MSVC_KIT_INSTALL_DIR` | 目标目录 | `msvc-kit`（相对当前工作目录） |
| `MSVC_KIT_MSVC_VERSION` | MSVC 版本 | `None`（最新） |
| `MSVC_KIT_SDK_VERSION` | SDK 版本 | `None`（最新） |
| `MSVC_KIT_PARALLEL_DOWNLOADS` | 并发下载数 | `4` |
| `MSVC_KIT_VERIFY_HASHES` | 哈希校验 | `true`（`0`/`false`/`no` 才关闭） |
| `MSVC_KIT_DRY_RUN` | 预览模式 | `false`（`1`/`true`/`yes` 开启） |
| `MSVC_KIT_VS_CHANNEL` | Visual Studio channel | `None`（auto） |
| `MSVC_KIT_INCLUDE_COMPONENTS` | 逗号分隔的组件列表 | 空 |
| `MSVC_KIT_EXCLUDE_PATTERNS` | 逗号分隔的模式列表 | 空 |

这些变量只对库生效。CLI 的 `DownloadOptions` 来自命令行参数和配置文件，不读取它们。

## 使用示例

### 默认选项

```rust
use msvc_kit::{download_msvc, DownloadOptions};

let options = DownloadOptions::default();
let info = download_msvc(&options).await?;
```

### 自定义目录

```rust
use msvc_kit::{download_msvc, DownloadOptions};
use std::path::PathBuf;

let options = DownloadOptions {
    target_dir: PathBuf::from("C:/my-msvc"),
    ..Default::default()
};
```

### 指定版本

```rust
use msvc_kit::{download_msvc, download_sdk, DownloadOptions};

let options = DownloadOptions {
    msvc_version: Some("14.44".to_string()),
    sdk_version: Some("10.0.26100.0".to_string()),
    ..Default::default()
};

let msvc = download_msvc(&options).await?;
let sdk = download_sdk(&options).await?;
```

### 交叉编译

```rust
use msvc_kit::{download_msvc, DownloadOptions, Architecture};

// 在 x64 主机上构建 ARM64 二进制文件
let options = DownloadOptions {
    arch: Architecture::Arm64,
    host_arch: Some(Architecture::X64),
    ..Default::default()
};
```

### 性能调优

```rust
use msvc_kit::{download_msvc, DownloadOptions};

let options = DownloadOptions {
    parallel_downloads: 8,  // 更多并行下载
    verify_hashes: false,   // 跳过验证（不推荐）
    ..Default::default()
};
```

## 字段详情

### target_dir

安装目录。`DownloadOptions::default()` 优先使用 `MSVC_KIT_INSTALL_DIR`，
未设置时是相对路径 `msvc-kit`；它**不会**读取 CLI 配置。
想装到 CLI 默认位置，请显式传入 `load_config()?.install_dir`：

```rust
let options = DownloadOptions::builder()
    .target_dir(msvc_kit::load_config()?.install_dir)
    .build();
```

### msvc_version

MSVC 版本字符串。示例：
- `"14.44"` - 主版本.次版本
- `"14.44.34823"` - 完整版本
- `None` - 使用最新可用版本

### sdk_version

Windows SDK 版本。示例：
- `"10.0.26100.0"` - 完整版本
- `None` - 使用最新可用版本

### arch

编译二进制文件的目标架构：
- `Architecture::X64` - 64 位 x86
- `Architecture::X86` - 32 位 x86
- `Architecture::Arm64` - ARM 64 位
- `Architecture::Arm` - ARM 32 位

### host_arch

主机架构。设置为 `None` 自动检测。

### verify_hashes

设为 `true` 时，下载的文件会根据清单中的 SHA256 哈希进行验证。

### parallel_downloads

并发下载数量。较高的值可能加快下载速度，但会使用更多带宽。

### http_client

自定义 `reqwest::Client`，用于 HTTP 请求。可用于配置代理或自定义 TLS 设置。

### progress_handler

自定义进度处理器，需实现 `ProgressHandler` trait。使用 `NoopProgressHandler` 可以抑制输出。

### cache_manager

自定义缓存管理器，需实现 `CacheManager` trait。允许多个实例共享缓存。

```rust
use msvc_kit::{DownloadOptions, FileSystemCacheManager};
use std::path::PathBuf;
use std::sync::Arc;

let cache = Arc::new(FileSystemCacheManager::new(
    PathBuf::from("/shared/cache")
));

let options = DownloadOptions::builder()
    .target_dir("C:/msvc")
    .cache_manager(cache)
    .build();
```

### dry_run

设为 `true` 时，显示将要下载的内容但不实际下载。

### include_components / exclude_patterns

`include_components` 在标准工具链之上追加可选组件：`MsvcComponent::Spectre`、`Mfc`、`Atl`、`Asan`、`Uwp`、`Cli`、`Modules`、`Redist` 和 `Custom(String)`。
`exclude_patterns` 会丢弃包 ID 中包含任一模式的包（大小写不敏感）。

```rust
use msvc_kit::{DownloadOptions, MsvcComponent};

let options = DownloadOptions::builder()
    .include_components([MsvcComponent::Spectre, MsvcComponent::Mfc])
    .exclude_pattern("arm64")
    .build();
```

### vs_channel / manifest_cache_dir

`vs_channel` 固定用于发现包的 Visual Studio channel（`"17"`、`"2022"`、`"auto"` 等）。
`manifest_cache_dir` 决定 channel 清单的缓存位置；传 `None` 使用平台默认位置，
传入 `config.manifest_cache_dir()` 则跟随配置中的缓存目录。

## Builder 模式

推荐使用 Builder 模式创建 `DownloadOptions`：

```rust
use msvc_kit::{DownloadOptions, Architecture};

let options = DownloadOptions::builder()
    .msvc_version("14.44")
    .sdk_version("10.0.26100.0")
    .target_dir("C:/msvc-kit")
    .arch(Architecture::X64)
    .host_arch(Architecture::X64)
    .verify_hashes(true)
    .parallel_downloads(8)
    .dry_run(false)
    .build();
```

## download_all

并行下载 MSVC 和 SDK：

```rust
use msvc_kit::{download_all, DownloadOptions};

let options = DownloadOptions::default();
let (msvc_info, sdk_info) = download_all(&options).await?;
```
