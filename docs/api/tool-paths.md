# ToolPaths

Collection of paths to MSVC tool executables.

## Definition

```rust
pub struct ToolPaths {
    /// Path to cl.exe (C/C++ compiler)
    pub cl: Option<PathBuf>,
    
    /// Path to link.exe (linker)
    pub link: Option<PathBuf>,
    
    /// Path to lib.exe (static library manager)
    pub lib: Option<PathBuf>,
    
    /// Path to ml64.exe (MASM assembler)
    pub ml64: Option<PathBuf>,
    
    /// Path to nmake.exe (make utility)
    pub nmake: Option<PathBuf>,
    
    /// Path to rc.exe (resource compiler)
    pub rc: Option<PathBuf>,
}
```

## Usage

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

let options = DownloadOptions::default();
let msvc = download_msvc(&options).await?;
let sdk = download_sdk(&options).await?;
let env = setup_environment(&msvc, Some(&sdk))?;

// Get all tool paths
let tools = env.tool_paths();
```

## Tool Descriptions

### cl.exe - C/C++ Compiler

The main compiler executable.

```rust
if let Some(cl) = tools.cl {
    std::process::Command::new(&cl)
        .args(["/c", "main.cpp"])
        .status()?;
}
```

### link.exe - Linker

Links object files into executables or DLLs.

```rust
if let Some(link) = tools.link {
    std::process::Command::new(&link)
        .args(["main.obj", "/OUT:main.exe"])
        .status()?;
}
```

### lib.exe - Library Manager

Creates and manages static libraries (.lib files).

```rust
if let Some(lib) = tools.lib {
    std::process::Command::new(&lib)
        .args(["/OUT:mylib.lib", "a.obj", "b.obj"])
        .status()?;
}
```

### ml64.exe - MASM Assembler

Microsoft Macro Assembler for x64.

```rust
if let Some(ml64) = tools.ml64 {
    std::process::Command::new(&ml64)
        .args(["/c", "asm.asm"])
        .status()?;
}
```

### nmake.exe - Make Utility

Microsoft's make utility for building projects.

```rust
if let Some(nmake) = tools.nmake {
    std::process::Command::new(&nmake)
        .args(["/f", "Makefile"])
        .status()?;
}
```

### rc.exe - Resource Compiler

Compiles Windows resource files (.rc).

```rust
if let Some(rc) = tools.rc {
    std::process::Command::new(&rc)
        .args(["resources.rc"])
        .status()?;
}
```

## Complete Build Example

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;

async fn build_project() -> msvc_kit::Result<()> {
    // Setup environment
    let options = DownloadOptions::default();
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    let tools = env.tool_paths();
    
    // Set environment variables
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    
    let cl = tools.cl.expect("cl.exe not found");
    let link = tools.link.expect("link.exe not found");
    
    // Compile
    Command::new(&cl)
        .args(["/c", "/O2", "main.cpp"])
        .status()?;
    
    // Link
    Command::new(&link)
        .args(["main.obj", "/OUT:main.exe"])
        .status()?;
    
    println!("Build complete!");
    Ok(())
}
```

## Serialization

`ToolPaths` implements `Serialize` and `Deserialize`:

```rust
let tools = env.tool_paths();
let json = serde_json::to_string_pretty(&tools)?;
println!("{}", json);
```

Output:
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
