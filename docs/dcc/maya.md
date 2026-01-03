# Maya Plugin Development

Build Maya plugins using msvc-kit without installing Visual Studio.

## Requirements

| Maya Version | MSVC Version | Windows SDK |
|--------------|--------------|-------------|
| Maya 2025 | 14.38 | 10.0.22621.0 |
| Maya 2024 | 14.34 | 10.0.22000.0 |
| Maya 2023 | 14.32 | 10.0.19041.0 |
| Maya 2022 | 14.29 | 10.0.19041.0 |

## Setup

### 1. Download MSVC

```bash
# For Maya 2025
msvc-kit download --msvc-version 14.38 --sdk-version 10.0.22621.0

# For Maya 2024
msvc-kit download --msvc-version 14.34 --sdk-version 10.0.22000.0
```

### 2. Setup Environment

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### 3. Set Maya DevKit Path

```powershell
$env:MAYA_DEVKIT = "C:\Program Files\Autodesk\Maya2025\devkit"
```

## Building a Simple Plugin

### Plugin Source (helloWorld.cpp)

```cpp
#include <maya/MSimple.h>
#include <maya/MGlobal.h>

DeclareSimpleCommand(helloWorld, "Autodesk", "1.0");

MStatus helloWorld::doIt(const MArgList&) {
    MGlobal::displayInfo("Hello, World!");
    return MS::kSuccess;
}
```

### Compile

```powershell
# Setup environment
msvc-kit setup --script --shell powershell | Invoke-Expression

# Compile
cl /c /O2 /MD /EHsc /DWIN32 /D_WINDOWS /DNT_PLUGIN `
   /I"$env:MAYA_DEVKIT\include" `
   helloWorld.cpp

# Link
link /DLL /OUT:helloWorld.mll `
     helloWorld.obj `
     /LIBPATH:"$env:MAYA_DEVKIT\lib" `
     OpenMaya.lib OpenMayaAnim.lib OpenMayaUI.lib Foundation.lib
```

## CMake Build

### CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.20)
project(MayaPlugin)

set(CMAKE_CXX_STANDARD 17)

# Maya settings
set(MAYA_VERSION "2025" CACHE STRING "Maya version")
set(MAYA_DEVKIT "C:/Program Files/Autodesk/Maya${MAYA_VERSION}/devkit" 
    CACHE PATH "Maya DevKit path")

# Compiler flags for Maya
add_compile_options(
    /O2 /MD /EHsc
    /DWIN32 /D_WINDOWS /DNT_PLUGIN
    /D_USRDLL /D_WINDLL
)

# Plugin library
add_library(myPlugin SHARED
    src/plugin.cpp
    src/commands.cpp
)

target_include_directories(myPlugin PRIVATE
    ${MAYA_DEVKIT}/include
)

target_link_directories(myPlugin PRIVATE
    ${MAYA_DEVKIT}/lib
)

target_link_libraries(myPlugin
    OpenMaya
    OpenMayaAnim
    OpenMayaFX
    OpenMayaRender
    OpenMayaUI
    Foundation
)

# Output as .mll
set_target_properties(myPlugin PROPERTIES
    SUFFIX ".mll"
    PREFIX ""
)
```

### Build

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

cmake -B build -G "NMake Makefiles" -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

## Python API Extension

For Python-based Maya tools that need C++ performance:

### pybind11 Extension

```cpp
#include <pybind11/pybind11.h>
#include <maya/MFnMesh.h>
#include <maya/MPointArray.h>

namespace py = pybind11;

std::vector<std::array<double, 3>> get_vertex_positions(const char* meshName) {
    // ... Maya API code ...
}

PYBIND11_MODULE(maya_native, m) {
    m.def("get_vertex_positions", &get_vertex_positions);
}
```

### Build

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

cl /c /O2 /MD /EHsc `
   /I"$env:MAYA_DEVKIT\include" `
   /I"path\to\pybind11\include" `
   /I"$env:MAYA_DEVKIT\include\Python" `
   maya_native.cpp

link /DLL /OUT:maya_native.pyd `
     maya_native.obj `
     /LIBPATH:"$env:MAYA_DEVKIT\lib" `
     OpenMaya.lib python310.lib
```

## Library API Usage

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;
use std::path::PathBuf;

async fn build_maya_plugin(maya_version: &str) -> msvc_kit::Result<()> {
    // Version mapping
    let msvc_version = match maya_version {
        "2025" => "14.38",
        "2024" => "14.34",
        "2023" => "14.32",
        _ => "14.38",
    };
    
    let options = DownloadOptions {
        msvc_version: Some(msvc_version.to_string()),
        ..Default::default()
    };
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Setup environment
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    
    let maya_devkit = format!("C:\\Program Files\\Autodesk\\Maya{}\\devkit", maya_version);
    
    // Compile
    let cl = env.cl_exe_path().expect("cl.exe not found");
    Command::new(&cl)
        .args([
            "/c", "/O2", "/MD", "/EHsc",
            "/DWIN32", "/D_WINDOWS", "/DNT_PLUGIN",
            &format!("/I{}\\include", maya_devkit),
            "plugin.cpp"
        ])
        .status()?;
    
    // Link
    let link = env.link_exe_path().expect("link.exe not found");
    Command::new(&link)
        .args([
            "/DLL", "/OUT:plugin.mll",
            "plugin.obj",
            &format!("/LIBPATH:{}\\lib", maya_devkit),
            "OpenMaya.lib", "Foundation.lib"
        ])
        .status()?;
    
    Ok(())
}
```

## CI/CD Example

```yaml
name: Build Maya Plugin

on: [push]

jobs:
  build:
    runs-on: windows-latest
    strategy:
      matrix:
        maya: ['2024', '2025']
        include:
          - maya: '2024'
            msvc: '14.34'
          - maya: '2025'
            msvc: '14.38'
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Install msvc-kit
        run: cargo install msvc-kit
      
      - name: Download MSVC
        run: msvc-kit download --msvc-version ${{ matrix.msvc }}
      
      - name: Setup Environment
        run: msvc-kit setup --script --shell powershell | Invoke-Expression
      
      - name: Build Plugin
        run: |
          cmake -B build -G "NMake Makefiles" `
            -DMAYA_VERSION=${{ matrix.maya }}
          cmake --build build
```

## Troubleshooting

### "Cannot find Maya headers"

Ensure MAYA_DEVKIT is set correctly:

```powershell
$env:MAYA_DEVKIT = "C:\Program Files\Autodesk\Maya2025\devkit"
Test-Path "$env:MAYA_DEVKIT\include\maya\MFn.h"
```

### "Unresolved external symbol"

Check that you're linking all required Maya libraries:

```
OpenMaya.lib OpenMayaAnim.lib OpenMayaFX.lib 
OpenMayaRender.lib OpenMayaUI.lib Foundation.lib
```

### Runtime Crash

Ensure MSVC version matches Maya's build. Check Maya's `about` dialog for compiler info.
