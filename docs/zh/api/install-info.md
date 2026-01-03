# InstallInfo

已安装组件（MSVC 或 SDK）的信息。

## 定义

```rust
pub struct InstallInfo {
    /// 组件类型："msvc" 或 "sdk"
    pub component_type: String,
    
    /// 已安装版本
    pub version: String,
    
    /// 安装路径
    pub install_path: PathBuf,
    
    /// 已下载文件列表
    pub downloaded_files: Vec<PathBuf>,
    
    /// 目标架构
    pub arch: Architecture,
}
```

## 方法

### 验证

```rust
/// 检查安装是否有效（路径存在）
pub fn is_valid(&self) -> bool;

/// 获取已下载文件的总大小
pub fn total_size(&self) -> u64;
```

### 路径访问器

```rust
/// 获取此组件的 bin 目录
pub fn bin_dir(&self) -> PathBuf;

/// 获取此组件的 include 目录
pub fn include_dir(&self) -> PathBuf;

/// 获取此组件的 lib 目录
pub fn lib_dir(&self) -> PathBuf;
```

### 导出

```rust
/// 导出安装信息为 JSON
pub fn to_json(&self) -> serde_json::Value;
```

## 使用示例

### 基本用法

```rust
use msvc_kit::{download_msvc, DownloadOptions};

let options = DownloadOptions::default();
let info = download_msvc(&options).await?;

println!("组件: {}", info.component_type);  // "msvc"
println!("版本: {}", info.version);         // "14.44.34823"
println!("路径: {:?}", info.install_path);
println!("有效: {}", info.is_valid());
println!("大小: {} 字节", info.total_size());
```

### 访问目录

```rust
let info = download_msvc(&options).await?;

// 获取特定目录
let bin = info.bin_dir();          // .../bin/Hostx64/x64
let include = info.include_dir();  // .../include
let lib = info.lib_dir();          // .../lib/x64

println!("cl.exe 应该在: {:?}", bin.join("cl.exe"));
```

### 导出为 JSON

```rust
let info = download_msvc(&options).await?;
let json = info.to_json();

println!("{}", serde_json::to_string_pretty(&json)?);
```

输出：
```json
{
  "component_type": "msvc",
  "version": "14.44.34823",
  "install_path": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823",
  "bin_dir": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64",
  "include_dir": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\include",
  "lib_dir": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\lib\\x64",
  "arch": "x64",
  "is_valid": true,
  "total_size": 1234567890
}
```

### MSVC 与 SDK 路径

MSVC 和 SDK 的路径结构不同：

**MSVC:**
```
install_path: VC/Tools/MSVC/14.xx.xxxxx/
bin_dir:      VC/Tools/MSVC/14.xx.xxxxx/bin/Hostx64/x64/
include_dir:  VC/Tools/MSVC/14.xx.xxxxx/include/
lib_dir:      VC/Tools/MSVC/14.xx.xxxxx/lib/x64/
```

**SDK:**
```
install_path: Windows Kits/10/
bin_dir:      Windows Kits/10/bin/10.0.xxxxx.0/x64/
include_dir:  Windows Kits/10/Include/10.0.xxxxx.0/
lib_dir:      Windows Kits/10/Lib/10.0.xxxxx.0/um/x64/
```

## 序列化

`InstallInfo` 实现了 `Serialize` 和 `Deserialize`：

```rust
use serde_json;

let info = download_msvc(&options).await?;

// 序列化
let json = serde_json::to_string(&info)?;

// 反序列化
let restored: InstallInfo = serde_json::from_str(&json)?;
```
