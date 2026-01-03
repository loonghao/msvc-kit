# 3ds Max Plugin Development

Build 3ds Max plugins using msvc-kit without installing Visual Studio.

## Requirements

| 3ds Max Version | MSVC Version | Windows SDK |
|-----------------|--------------|-------------|
| 3ds Max 2025 | 14.38 | 10.0.22621.0 |
| 3ds Max 2024 | 14.34 | 10.0.22000.0 |
| 3ds Max 2023 | 14.32 | 10.0.19041.0 |
| 3ds Max 2022 | 14.29 | 10.0.19041.0 |

## Setup

### 1. Download MSVC

```bash
# For 3ds Max 2025
msvc-kit download --msvc-version 14.38 --sdk-version 10.0.22621.0
```

### 2. Setup Environment

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### 3. Set Max SDK Path

```powershell
$env:MAXSDK = "C:\Program Files\Autodesk\3ds Max 2025 SDK\maxsdk"
```

## Plugin Types

3ds Max supports several plugin types:

| Type | Extension | Description |
|------|-----------|-------------|
| Utility | .dlu | Utility plugins |
| Modifier | .dlm | Geometry modifiers |
| Object | .dlo | Procedural objects |
| Material | .dlt | Materials and textures |
| Controller | .dlc | Animation controllers |
| Import/Export | .dle | File format handlers |

## Building a Simple Plugin

### Utility Plugin (HelloWorld.cpp)

```cpp
#include <utilapi.h>
#include <istdplug.h>
#include <iparamb2.h>
#include <maxscript/maxscript.h>

#define HELLO_CLASS_ID Class_ID(0x12345678, 0x87654321)

class HelloWorld : public UtilityObj {
public:
    void BeginEditParams(Interface* ip, IUtil* iu) override;
    void EndEditParams(Interface* ip, IUtil* iu) override;
    void DeleteThis() override { }
};

void HelloWorld::BeginEditParams(Interface* ip, IUtil* iu) {
    MessageBox(ip->GetMAXHWnd(), L"Hello, 3ds Max!", L"Hello", MB_OK);
}

void HelloWorld::EndEditParams(Interface* ip, IUtil* iu) {}

// Class descriptor
class HelloWorldClassDesc : public ClassDesc2 {
public:
    int IsPublic() override { return TRUE; }
    void* Create(BOOL loading = FALSE) override { return new HelloWorld(); }
    const TCHAR* ClassName() override { return _T("HelloWorld"); }
    SClass_ID SuperClassID() override { return UTILITY_CLASS_ID; }
    Class_ID ClassID() override { return HELLO_CLASS_ID; }
    const TCHAR* Category() override { return _T("My Plugins"); }
};

static HelloWorldClassDesc helloWorldDesc;

// DLL entry points
BOOL WINAPI DllMain(HINSTANCE hinstDLL, ULONG fdwReason, LPVOID lpvReserved) {
    return TRUE;
}

__declspec(dllexport) const TCHAR* LibDescription() { return _T("Hello World Plugin"); }
__declspec(dllexport) int LibNumberClasses() { return 1; }
__declspec(dllexport) ClassDesc* LibClassDesc(int i) { return &helloWorldDesc; }
__declspec(dllexport) ULONG LibVersion() { return VERSION_3DSMAX; }
```

### Compile

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

# Compile
cl /c /O2 /MD /EHsc /DWIN32 /D_WINDOWS /DUNICODE /D_UNICODE `
   /I"$env:MAXSDK\include" `
   HelloWorld.cpp

# Link
link /DLL /OUT:HelloWorld.dlu `
     HelloWorld.obj `
     /LIBPATH:"$env:MAXSDK\lib\x64\Release" `
     core.lib geom.lib gfx.lib mesh.lib maxutil.lib `
     paramblk2.lib bmm.lib
```

## CMake Build

### CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.20)
project(MaxPlugin)

set(CMAKE_CXX_STANDARD 17)

# 3ds Max SDK
set(MAX_VERSION "2025" CACHE STRING "3ds Max version")
set(MAXSDK "C:/Program Files/Autodesk/3ds Max ${MAX_VERSION} SDK/maxsdk"
    CACHE PATH "3ds Max SDK path")

# Compiler flags
add_compile_options(
    /O2 /MD /EHsc
    /DWIN32 /D_WINDOWS /DUNICODE /D_UNICODE
)

# Plugin
add_library(HelloWorld SHARED
    src/HelloWorld.cpp
    src/HelloWorld.def
)

target_include_directories(HelloWorld PRIVATE
    ${MAXSDK}/include
)

target_link_directories(HelloWorld PRIVATE
    ${MAXSDK}/lib/x64/Release
)

target_link_libraries(HelloWorld
    core geom gfx mesh maxutil paramblk2 bmm
)

# Output as .dlu
set_target_properties(HelloWorld PROPERTIES
    SUFFIX ".dlu"
    PREFIX ""
)
```

### Module Definition File (HelloWorld.def)

```def
LIBRARY HelloWorld
EXPORTS
    LibDescription
    LibNumberClasses
    LibClassDesc
    LibVersion
```

### Build

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

cmake -B build -G "NMake Makefiles" -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

## Modifier Plugin Example

### SimpleModifier.cpp

```cpp
#include <simpmod.h>
#include <iparamb2.h>

#define SIMPLE_MOD_CLASS_ID Class_ID(0xABCDEF01, 0x10FEDCBA)

class SimpleMod : public SimpleMod2 {
public:
    void ModifyObject(TimeValue t, ModContext& mc, ObjectState* os, INode* node) override;
    Interval LocalValidity(TimeValue t) override { return FOREVER; }
    Class_ID ClassID() override { return SIMPLE_MOD_CLASS_ID; }
    void GetClassName(TSTR& s) override { s = _T("SimpleMod"); }
};

void SimpleMod::ModifyObject(TimeValue t, ModContext& mc, ObjectState* os, INode* node) {
    // Modify geometry here
}
```

## Library API Usage

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;

async fn build_max_plugin(max_version: &str) -> msvc_kit::Result<()> {
    let msvc_version = match max_version {
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
    
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    
    let maxsdk = format!("C:\\Program Files\\Autodesk\\3ds Max {} SDK\\maxsdk", max_version);
    
    let cl = env.cl_exe_path().expect("cl.exe not found");
    Command::new(&cl)
        .args([
            "/c", "/O2", "/MD", "/EHsc",
            "/DWIN32", "/D_WINDOWS", "/DUNICODE", "/D_UNICODE",
            &format!("/I{}\\include", maxsdk),
            "HelloWorld.cpp"
        ])
        .status()?;
    
    let link = env.link_exe_path().expect("link.exe not found");
    Command::new(&link)
        .args([
            "/DLL", "/OUT:HelloWorld.dlu",
            "HelloWorld.obj",
            &format!("/LIBPATH:{}\\lib\\x64\\Release", maxsdk),
            "core.lib", "geom.lib", "maxutil.lib"
        ])
        .status()?;
    
    Ok(())
}
```

## Installation

Copy the plugin to 3ds Max's plugin directory:

```powershell
$MAX_PLUGINS = "C:\Program Files\Autodesk\3ds Max 2025\plugins"
Copy-Item HelloWorld.dlu "$MAX_PLUGINS\"
```

Or use a custom plugin path in `3dsmax.ini`:

```ini
[Directories]
Additional Plug-In Path=C:\MyPlugins
```

## Troubleshooting

### "Cannot find Max SDK headers"

```powershell
$env:MAXSDK = "C:\Program Files\Autodesk\3ds Max 2025 SDK\maxsdk"
Test-Path "$env:MAXSDK\include\max.h"
```

### Linker Errors

Ensure you're linking the correct libraries:

```
core.lib geom.lib gfx.lib mesh.lib maxutil.lib paramblk2.lib bmm.lib
```

### Plugin Not Loading

Check 3ds Max's plugin manager and system log for errors.
