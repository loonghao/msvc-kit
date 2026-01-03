# MsvcEnvironment

MSVC 工具链的完整环境配置。

## 定义

```rust
pub struct MsvcEnvironment {
    /// Visual C++ 安装目录 (VCINSTALLDIR)
    pub vc_install_dir: PathBuf,
    
    /// VC 工具安装目录 (VCToolsInstallDir)
    pub vc_tools_install_dir: PathBuf,
    
    /// VC 工具版本 (VCToolsVersion)
    pub vc_tools_version: String,
    
    /// Windows SDK 目录 (WindowsSdkDir)
    pub windows_sdk_dir: PathBuf,
    
    /// Windows SDK 版本 (WindowsSDKVersion)
    pub windows_sdk_version: String,
    
    /// 编译器包含路径
    pub include_paths: Vec<PathBuf>,
    
    /// 链接器库路径
    pub lib_paths: Vec<PathBuf>,
    
    /// 二进制路径（cl.exe、link.exe 等）
    pub bin_paths: Vec<PathBuf>,
    
    /// 目标架构
    pub arch: Architecture,
    
    /// 主机架构
    pub host_arch: Architecture,
}
```

## 创建

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

let options = DownloadOptions::default();
let msvc = download_msvc(&options).await?;
let sdk = download_sdk(&options).await?;

// 从安装信息创建环境
let env = setup_environment(&msvc, Some(&sdk))?;
```

## 工具路径方法

```rust
/// 检查 cl.exe 是否可用
pub fn has_cl_exe(&self) -> bool;

/// 获取 cl.exe（C/C++ 编译器）路径
pub fn cl_exe_path(&self) -> Option<PathBuf>;

/// 获取 link.exe（链接器）路径
pub fn link_exe_path(&self) -> Option<PathBuf>;

/// 获取 lib.exe（静态库管理器）路径
pub fn lib_exe_path(&self) -> Option<PathBuf>;

/// 获取 ml64.exe（MASM 汇编器）路径
pub fn ml64_exe_path(&self) -> Option<PathBuf>;

/// 获取 nmake.exe（make 工具）路径
pub fn nmake_exe_path(&self) -> Option<PathBuf>;

/// 获取 rc.exe（资源编译器）路径
pub fn rc_exe_path(&self) -> Option<PathBuf>;

/// 获取所有工具路径
pub fn tool_paths(&self) -> ToolPaths;
```

## 环境字符串方法

```rust
/// 获取 INCLUDE 环境变量值
pub fn include_path_string(&self) -> String;

/// 获取 LIB 环境变量值
pub fn lib_path_string(&self) -> String;

/// 获取 PATH 附加值
pub fn bin_path_string(&self) -> String;
```

## 导出方法

```rust
/// 导出环境为 JSON
pub fn to_json(&self) -> serde_json::Value;
```

## 使用示例

### 访问工具路径

```rust
let env = setup_environment(&msvc, Some(&sdk))?;

if let Some(cl) = env.cl_exe_path() {
    println!("cl.exe: {:?}", cl);
    
    // 运行 cl.exe
    std::process::Command::new(&cl)
        .arg("/help")
        .status()?;
}
```

### 获取所有工具

```rust
let tools = env.tool_paths();

println!("编译器: {:?}", tools.cl);
println!("链接器: {:?}", tools.link);
println!("库管理器: {:?}", tools.lib);
println!("汇编器: {:?}", tools.ml64);
println!("Make: {:?}", tools.nmake);
println!("资源编译器: {:?}", tools.rc);
```

### 设置环境变量

```rust
use std::env;

let msvc_env = setup_environment(&msvc, Some(&sdk))?;

// 设置 INCLUDE
env::set_var("INCLUDE", msvc_env.include_path_string());

// 设置 LIB
env::set_var("LIB", msvc_env.lib_path_string());

// 添加到 PATH
let current_path = env::var("PATH").unwrap_or_default();
env::set_var("PATH", format!("{};{}", msvc_env.bin_path_string(), current_path));
```

### 导出为 JSON

```rust
let env = setup_environment(&msvc, Some(&sdk))?;
let json = env.to_json();

// 保存到文件供外部工具使用
std::fs::write("msvc-env.json", serde_json::to_string_pretty(&json)?)?;
```

输出：
```json
{
  "vc_install_dir": "C:\\msvc-kit\\VC",
  "vc_tools_install_dir": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823",
  "vc_tools_version": "14.44.34823",
  "windows_sdk_dir": "C:\\msvc-kit\\Windows Kits\\10",
  "windows_sdk_version": "10.0.26100.0",
  "include_paths": [...],
  "lib_paths": [...],
  "bin_paths": [...],
  "arch": "x64",
  "host_arch": "x64",
  "tools": {
    "cl": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64\\cl.exe",
    "link": "...",
    "lib": "...",
    "ml64": "...",
    "nmake": "...",
    "rc": "..."
  }
}
```

### 生成 Shell 脚本

```rust
use msvc_kit::env::{generate_activation_script, ShellType};

let script = generate_activation_script(&env, ShellType::PowerShell);
println!("{}", script);
```
