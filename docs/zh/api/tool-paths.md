# ToolPaths

MSVC 工具可执行文件路径集合。

## 定义

```rust
pub struct ToolPaths {
    /// cl.exe（C/C++ 编译器）路径
    pub cl: Option<PathBuf>,
    
    /// link.exe（链接器）路径
    pub link: Option<PathBuf>,
    
    /// lib.exe（静态库管理器）路径
    pub lib: Option<PathBuf>,
    
    /// ml64.exe（MASM 汇编器）路径
    pub ml64: Option<PathBuf>,
    
    /// nmake.exe（make 工具）路径
    pub nmake: Option<PathBuf>,
    
    /// rc.exe（资源编译器）路径
    pub rc: Option<PathBuf>,
}
```

## 使用

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

let options = DownloadOptions::default();
let msvc = download_msvc(&options).await?;
let sdk = download_sdk(&options).await?;
let env = setup_environment(&msvc, Some(&sdk))?;

// 获取所有工具路径
let tools = env.tool_paths();
```

## 工具说明

### cl.exe - C/C++ 编译器

主编译器可执行文件。

```rust
if let Some(cl) = tools.cl {
    std::process::Command::new(&cl)
        .args(["/c", "main.cpp"])
        .status()?;
}
```

### link.exe - 链接器

将目标文件链接为可执行文件或 DLL。

```rust
if let Some(link) = tools.link {
    std::process::Command::new(&link)
        .args(["main.obj", "/OUT:main.exe"])
        .status()?;
}
```

### lib.exe - 库管理器

创建和管理静态库（.lib 文件）。

```rust
if let Some(lib) = tools.lib {
    std::process::Command::new(&lib)
        .args(["/OUT:mylib.lib", "a.obj", "b.obj"])
        .status()?;
}
```

### ml64.exe - MASM 汇编器

适用于 x64 的 Microsoft 宏汇编器。

```rust
if let Some(ml64) = tools.ml64 {
    std::process::Command::new(&ml64)
        .args(["/c", "asm.asm"])
        .status()?;
}
```

### nmake.exe - Make 工具

Microsoft 的 make 工具，用于构建项目。

```rust
if let Some(nmake) = tools.nmake {
    std::process::Command::new(&nmake)
        .args(["/f", "Makefile"])
        .status()?;
}
```

### rc.exe - 资源编译器

编译 Windows 资源文件（.rc）。

```rust
if let Some(rc) = tools.rc {
    std::process::Command::new(&rc)
        .args(["resources.rc"])
        .status()?;
}
```

## 完整构建示例

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;

async fn build_project() -> msvc_kit::Result<()> {
    // 设置环境
    let options = DownloadOptions::default();
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    let tools = env.tool_paths();
    
    // 设置环境变量
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    
    let cl = tools.cl.expect("未找到 cl.exe");
    let link = tools.link.expect("未找到 link.exe");
    
    // 编译
    Command::new(&cl)
        .args(["/c", "/O2", "main.cpp"])
        .status()?;
    
    // 链接
    Command::new(&link)
        .args(["main.obj", "/OUT:main.exe"])
        .status()?;
    
    println!("构建完成！");
    Ok(())
}
```

## 序列化

`ToolPaths` 实现了 `Serialize` 和 `Deserialize`：

```rust
let tools = env.tool_paths();
let json = serde_json::to_string_pretty(&tools)?;
println!("{}", json);
```

输出：
```json
{
  "cl": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64\\cl.exe",
  "link": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64\\link.exe",
  "lib": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64\\lib.exe",
  "ml64": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64\\ml64.exe",
  "nmake": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64\\nmake.exe",
  "rc": "C:\\msvc-kit\\Windows Kits\\10\\bin\\10.0.26100.0\\x64\\rc.exe"
}
```
