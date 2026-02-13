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
    
    /// 预览模式：不实际下载
    pub dry_run: bool,
}
```

## 默认值

```rust
impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            target_dir: default_install_dir(),
            msvc_version: None,      // 最新版本
            sdk_version: None,       // 最新版本
            arch: Architecture::X64,
            host_arch: None,         // 自动检测
            verify_hashes: true,
            parallel_downloads: 4,
        }
    }
}
```

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

安装目录。默认为：
- Windows: `%LOCALAPPDATA%\loonghao\msvc-kit`

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
