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
