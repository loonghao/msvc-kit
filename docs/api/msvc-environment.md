# MsvcEnvironment

Complete environment configuration for the MSVC toolchain.

## Definition

```rust
pub struct MsvcEnvironment {
    /// Visual C++ installation directory (VCINSTALLDIR)
    pub vc_install_dir: PathBuf,
    
    /// VC Tools installation directory (VCToolsInstallDir)
    pub vc_tools_install_dir: PathBuf,
    
    /// VC Tools version (VCToolsVersion)
    pub vc_tools_version: String,
    
    /// Windows SDK directory (WindowsSdkDir)
    pub windows_sdk_dir: PathBuf,
    
    /// Windows SDK version (WindowsSDKVersion)
    pub windows_sdk_version: String,
    
    /// Include paths for compiler
    pub include_paths: Vec<PathBuf>,
    
    /// Library paths for linker
    pub lib_paths: Vec<PathBuf>,
    
    /// Binary paths (for cl.exe, link.exe, etc.)
    pub bin_paths: Vec<PathBuf>,
    
    /// Target architecture
    pub arch: Architecture,
    
    /// Host architecture
    pub host_arch: Architecture,
}
```

## Creation

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

let options = DownloadOptions::default();
let msvc = download_msvc(&options).await?;
let sdk = download_sdk(&options).await?;

// Create environment from install info
let env = setup_environment(&msvc, Some(&sdk))?;
```

## Tool Path Methods

```rust
/// Check if cl.exe is available
pub fn has_cl_exe(&self) -> bool;

/// Get path to cl.exe (C/C++ compiler)
pub fn cl_exe_path(&self) -> Option<PathBuf>;

/// Get path to link.exe (linker)
pub fn link_exe_path(&self) -> Option<PathBuf>;

/// Get path to lib.exe (static library manager)
pub fn lib_exe_path(&self) -> Option<PathBuf>;

/// Get path to ml64.exe (MASM assembler)
pub fn ml64_exe_path(&self) -> Option<PathBuf>;

/// Get path to nmake.exe (make utility)
pub fn nmake_exe_path(&self) -> Option<PathBuf>;

/// Get path to rc.exe (resource compiler)
pub fn rc_exe_path(&self) -> Option<PathBuf>;

/// Get all tool paths as a struct
pub fn tool_paths(&self) -> ToolPaths;
```

## Environment String Methods

```rust
/// Get INCLUDE environment variable value
pub fn include_path_string(&self) -> String;

/// Get LIB environment variable value
pub fn lib_path_string(&self) -> String;

/// Get PATH additions
pub fn bin_path_string(&self) -> String;
```

## Export Methods

```rust
/// Export environment to JSON
pub fn to_json(&self) -> serde_json::Value;
```

## Usage Examples

### Access Tool Paths

```rust
let env = setup_environment(&msvc, Some(&sdk))?;

if let Some(cl) = env.cl_exe_path() {
    println!("cl.exe: {:?}", cl);
    
    // Run cl.exe
    std::process::Command::new(&cl)
        .arg("/help")
        .status()?;
}
```

### Get All Tools

```rust
let tools = env.tool_paths();

println!("Compiler: {:?}", tools.cl);
println!("Linker: {:?}", tools.link);
println!("Lib: {:?}", tools.lib);
println!("Assembler: {:?}", tools.ml64);
println!("Make: {:?}", tools.nmake);
println!("Resource Compiler: {:?}", tools.rc);
```

### Set Environment Variables

```rust
use std::env;

let msvc_env = setup_environment(&msvc, Some(&sdk))?;

// Set INCLUDE
env::set_var("INCLUDE", msvc_env.include_path_string());

// Set LIB
env::set_var("LIB", msvc_env.lib_path_string());

// Prepend to PATH
let current_path = env::var("PATH").unwrap_or_default();
env::set_var("PATH", format!("{};{}", msvc_env.bin_path_string(), current_path));
```

### Export to JSON

```rust
let env = setup_environment(&msvc, Some(&sdk))?;
let json = env.to_json();

// Save to file for external tools
std::fs::write("msvc-env.json", serde_json::to_string_pretty(&json)?)?;
```

Output:
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

### Generate Shell Script

```rust
use msvc_kit::env::{generate_activation_script, ShellType};

let script = generate_activation_script(&env, ShellType::PowerShell);
println!("{}", script);
```
