# Unreal Engine 5 Integration

Build Unreal Engine 5 plugins and projects using msvc-kit without installing Visual Studio.

## Requirements

| UE Version | MSVC Version | Windows SDK |
|------------|--------------|-------------|
| UE 5.4+ | 14.38.33130+ | 10.0.22621.0 |
| UE 5.3 | 14.36.32532+ | 10.0.22621.0 |
| UE 5.2 | 14.34.31933+ | 10.0.22000.0 |
| UE 5.1 | 14.34.31933+ | 10.0.22000.0 |
| UE 5.0 | 14.32.31326+ | 10.0.19041.0 |

## Setup

### 1. Download MSVC

```bash
# For UE 5.4+
msvc-kit download --msvc-version 14.38 --sdk-version 10.0.22621.0
```

### 2. Setup Environment

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### 3. Verify

```powershell
cl /?
# Should show: Microsoft (R) C/C++ Optimizing Compiler Version 19.38.xxxxx
```

## Building UE5 from Source

### Clone and Setup

```bash
git clone https://github.com/EpicGames/UnrealEngine.git
cd UnrealEngine
```

### Generate Project Files

```powershell
# Setup MSVC environment first
msvc-kit setup --script --shell powershell | Invoke-Expression

# Run setup
.\Setup.bat
.\GenerateProjectFiles.bat
```

### Build

```powershell
# Build Development Editor
.\Engine\Build\BatchFiles\Build.bat UnrealEditor Win64 Development -WaitMutex
```

## Building Plugins

### Plugin Structure

```
MyPlugin/
├── MyPlugin.uplugin
├── Source/
│   └── MyPlugin/
│       ├── MyPlugin.Build.cs
│       ├── Public/
│       │   └── MyPlugin.h
│       └── Private/
│           └── MyPlugin.cpp
```

### Build with UAT

```powershell
# Setup environment
msvc-kit setup --script --shell powershell | Invoke-Expression

# Build plugin
& "C:\UE5\Engine\Build\BatchFiles\RunUAT.bat" `
    BuildPlugin `
    -Plugin="C:\MyPlugin\MyPlugin.uplugin" `
    -Package="C:\Output" `
    -TargetPlatforms=Win64
```

## Native C++ Module

For performance-critical code, you might want a native C++ module:

### CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.20)
project(MyNativeModule)

set(CMAKE_CXX_STANDARD 20)

# UE5 requires specific flags
add_compile_options(
    /W4
    /WX-
    /wd4819
    /EHsc
    /MD
)

add_library(MyNativeModule SHARED
    Source/Module.cpp
)

target_include_directories(MyNativeModule PRIVATE
    ${UE_ROOT}/Engine/Source/Runtime/Core/Public
    ${UE_ROOT}/Engine/Source/Runtime/CoreUObject/Public
)
```

### Build

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

cmake -B build -G "NMake Makefiles" -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

## Library API Usage

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;

async fn build_ue5_plugin() -> msvc_kit::Result<()> {
    // Download UE5.4 compatible MSVC
    let options = DownloadOptions {
        msvc_version: Some("14.38".to_string()),
        sdk_version: Some("10.0.22621.0".to_string()),
        ..Default::default()
    };
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Set environment
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    let path = format!("{};{}", env.bin_path_string(), std::env::var("PATH")?);
    std::env::set_var("PATH", path);
    
    // Run UAT
    Command::new("C:\\UE5\\Engine\\Build\\BatchFiles\\RunUAT.bat")
        .args(["BuildPlugin", "-Plugin=MyPlugin.uplugin"])
        .status()?;
    
    Ok(())
}
```

## CI/CD Example

### GitHub Actions

```yaml
name: Build UE5 Plugin

on: [push]

jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install msvc-kit
        run: cargo install msvc-kit
      
      - name: Download MSVC for UE5.4
        run: msvc-kit download --msvc-version 14.38 --sdk-version 10.0.22621.0
      
      - name: Setup Environment
        run: msvc-kit setup --script --shell powershell | Invoke-Expression
      
      - name: Build Plugin
        run: |
          & "$env:UE_ROOT\Engine\Build\BatchFiles\RunUAT.bat" `
            BuildPlugin `
            -Plugin="${{ github.workspace }}\MyPlugin.uplugin" `
            -Package="${{ github.workspace }}\Output"
```

## Troubleshooting

### "MSVC version mismatch" Error

Ensure your MSVC version matches UE's requirements:

```bash
msvc-kit list  # Check installed version
msvc-kit download --msvc-version 14.38  # Download correct version
```

### Missing Windows SDK

```bash
msvc-kit download --sdk-version 10.0.22621.0
```

### Linker Errors

Make sure both INCLUDE and LIB paths are set:

```powershell
echo $env:INCLUDE
echo $env:LIB
```
