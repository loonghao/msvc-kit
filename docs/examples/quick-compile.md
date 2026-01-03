# Quick Compile Examples

Quickly compile C/C++ code without complex build systems.

## One-Liner Compilation

### Simple C Program

```powershell
# Setup and compile in one line
msvc-kit setup --script --shell powershell | iex; cl hello.c
```

### C++ with Standard Library

```powershell
msvc-kit setup --script --shell powershell | iex; cl /EHsc /std:c++20 main.cpp
```

### Optimized Release Build

```powershell
msvc-kit setup --script --shell powershell | iex; cl /O2 /EHsc /DNDEBUG app.cpp
```

## Quick Compile Script

### compile.ps1

```powershell
# compile.ps1 - Quick compile helper
param(
    [Parameter(Mandatory=$true)]
    [string]$Source,
    
    [switch]$Release,
    [switch]$Cpp,
    [string]$Output
)

# Setup MSVC
msvc-kit setup --script --shell powershell | Invoke-Expression

# Build command
$args = @()

if ($Cpp) {
    $args += "/EHsc"
    $args += "/std:c++20"
}

if ($Release) {
    $args += "/O2"
    $args += "/DNDEBUG"
} else {
    $args += "/Od"
    $args += "/Zi"
}

if ($Output) {
    $args += "/Fe:$Output"
}

$args += $Source

# Compile
cl @args
```

Usage:

```powershell
# Debug build
.\compile.ps1 main.cpp -Cpp

# Release build
.\compile.ps1 main.cpp -Cpp -Release -Output app.exe
```

## Common Compilation Patterns

### Console Application

```powershell
msvc-kit setup --script --shell powershell | iex

cl /O2 /EHsc main.cpp
```

### Windows GUI Application

```powershell
msvc-kit setup --script --shell powershell | iex

cl /O2 /EHsc /DWIN32 /D_WINDOWS main.cpp user32.lib gdi32.lib /link /SUBSYSTEM:WINDOWS
```

### DLL Library

```powershell
msvc-kit setup --script --shell powershell | iex

# Compile
cl /c /O2 /EHsc /MD /DDLL_EXPORTS mylib.cpp

# Link as DLL
link /DLL /OUT:mylib.dll mylib.obj
```

### Static Library

```powershell
msvc-kit setup --script --shell powershell | iex

# Compile
cl /c /O2 /EHsc mylib.cpp

# Create static library
lib /OUT:mylib.lib mylib.obj
```

## Multi-File Compilation

### Multiple Source Files

```powershell
msvc-kit setup --script --shell powershell | iex

# Compile all .cpp files
cl /O2 /EHsc /Fe:app.exe src\*.cpp
```

### With Include Directory

```powershell
cl /O2 /EHsc /I"include" /Fe:app.exe src\*.cpp
```

### With Libraries

```powershell
cl /O2 /EHsc /I"include" src\*.cpp /link /LIBPATH:"lib" mylib.lib
```

## Rust Quick Compile Tool

### quick-compile.rs

```rust
//! Quick compile tool using msvc-kit
//! 
//! Usage: quick-compile <source> [options]

use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;
use std::path::Path;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <source.cpp> [--release] [--output name]", args[0]);
        std::process::exit(1);
    }
    
    let source = &args[1];
    let release = args.contains(&"--release".to_string());
    let output = args.iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());
    
    // Setup MSVC
    println!("Setting up MSVC...");
    let options = DownloadOptions::default();
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Set environment
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    let path = format!("{};{}", env.bin_path_string(), std::env::var("PATH")?);
    std::env::set_var("PATH", path);
    
    // Build command
    let cl = env.cl_exe_path().expect("cl.exe not found");
    let mut cmd = Command::new(&cl);
    
    // Add flags
    if release {
        cmd.args(["/O2", "/DNDEBUG"]);
    } else {
        cmd.args(["/Od", "/Zi"]);
    }
    
    // C++ flags if .cpp file
    if source.ends_with(".cpp") || source.ends_with(".cxx") {
        cmd.args(["/EHsc", "/std:c++20"]);
    }
    
    // Output name
    if let Some(out) = output {
        cmd.arg(format!("/Fe:{}", out));
    }
    
    // Source file
    cmd.arg(source);
    
    // Compile
    println!("Compiling {}...", source);
    let status = cmd.status()?;
    
    if status.success() {
        let exe_name = output.unwrap_or_else(|| {
            Path::new(source)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("a")
        });
        println!("Success: {}.exe", exe_name);
    } else {
        std::process::exit(1);
    }
    
    Ok(())
}
```

### Cargo.toml

```toml
[package]
name = "quick-compile"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "qc"
path = "src/main.rs"

[dependencies]
msvc-kit = "0.1"
tokio = { version = "1", features = ["full"] }
```

### Usage

```bash
# Build the tool
cargo install --path .

# Use it
qc main.cpp
qc main.cpp --release
qc main.cpp --release --output myapp
```

## Compiler Flags Reference

### Optimization

| Flag | Description |
|------|-------------|
| `/Od` | Disable optimization (debug) |
| `/O1` | Minimize size |
| `/O2` | Maximize speed |
| `/Ox` | Full optimization |

### Debug

| Flag | Description |
|------|-------------|
| `/Zi` | Generate debug info |
| `/DEBUG` | Link with debug info |

### C++ Standard

| Flag | Description |
|------|-------------|
| `/std:c++14` | C++14 |
| `/std:c++17` | C++17 |
| `/std:c++20` | C++20 |
| `/std:c++latest` | Latest |

### Runtime Library

| Flag | Description |
|------|-------------|
| `/MD` | DLL runtime (release) |
| `/MDd` | DLL runtime (debug) |
| `/MT` | Static runtime (release) |
| `/MTd` | Static runtime (debug) |

### Warnings

| Flag | Description |
|------|-------------|
| `/W0` | No warnings |
| `/W1-W4` | Warning levels |
| `/Wall` | All warnings |
| `/WX` | Warnings as errors |
