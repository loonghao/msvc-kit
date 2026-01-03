# Houdini Plugin Development

Build Houdini Digital Assets (HDAs) and plugins using msvc-kit.

## Requirements

| Houdini Version | MSVC Version | Windows SDK |
|-----------------|--------------|-------------|
| Houdini 20.5 | 14.38 | 10.0.22621.0 |
| Houdini 20.0 | 14.34 | 10.0.22000.0 |
| Houdini 19.5 | 14.32 | 10.0.19041.0 |
| Houdini 19.0 | 14.29 | 10.0.19041.0 |

## Setup

### 1. Download MSVC

```bash
# For Houdini 20.5
msvc-kit download --msvc-version 14.38 --sdk-version 10.0.22621.0
```

### 2. Setup Environment

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### 3. Set Houdini Environment

```powershell
$env:HFS = "C:\Program Files\Side Effects Software\Houdini 20.5.xxx"
$env:PATH = "$env:HFS\bin;$env:PATH"
```

## Building with hcustom

Houdini provides `hcustom` for building plugins:

```powershell
# Setup MSVC environment
msvc-kit setup --script --shell powershell | Invoke-Expression

# Build with hcustom
& "$env:HFS\bin\hcustom.exe" SOP_MyNode.cpp
```

## Manual Build

### Simple SOP Plugin

```cpp
// SOP_Star.cpp
#include <SOP/SOP_Node.h>
#include <OP/OP_Operator.h>
#include <OP/OP_OperatorTable.h>
#include <PRM/PRM_Include.h>
#include <GU/GU_Detail.h>
#include <UT/UT_DSOVersion.h>

class SOP_Star : public SOP_Node {
public:
    static OP_Node* myConstructor(OP_Network*, const char*, OP_Operator*);
    static PRM_Template myTemplateList[];
    
protected:
    SOP_Star(OP_Network* net, const char* name, OP_Operator* op);
    virtual ~SOP_Star();
    virtual OP_ERROR cookMySop(OP_Context& context);
};

// Implementation...

void newSopOperator(OP_OperatorTable* table) {
    table->addOperator(new OP_Operator(
        "star", "Star",
        SOP_Star::myConstructor,
        SOP_Star::myTemplateList,
        0, 0
    ));
}
```

### Compile

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

# Get Houdini compile flags
$HOUDINI_CFLAGS = & "$env:HFS\bin\hcustom.exe" -c
$HOUDINI_LDFLAGS = & "$env:HFS\bin\hcustom.exe" -m

# Compile
cl /c /O2 /MD /EHsc $HOUDINI_CFLAGS SOP_Star.cpp

# Link
link /DLL /OUT:SOP_Star.dll SOP_Star.obj $HOUDINI_LDFLAGS
```

## CMake Build

### CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.20)
project(HoudiniPlugin)

set(CMAKE_CXX_STANDARD 17)

# Find Houdini
set(HOUDINI_ROOT "C:/Program Files/Side Effects Software/Houdini 20.5.xxx"
    CACHE PATH "Houdini installation path")

# Get Houdini configuration
execute_process(
    COMMAND "${HOUDINI_ROOT}/bin/hcustom.exe" -c
    OUTPUT_VARIABLE HOUDINI_CFLAGS
    OUTPUT_STRIP_TRAILING_WHITESPACE
)

execute_process(
    COMMAND "${HOUDINI_ROOT}/bin/hcustom.exe" -m
    OUTPUT_VARIABLE HOUDINI_LDFLAGS
    OUTPUT_STRIP_TRAILING_WHITESPACE
)

# Plugin
add_library(SOP_Star SHARED
    src/SOP_Star.cpp
)

target_include_directories(SOP_Star PRIVATE
    ${HOUDINI_ROOT}/toolkit/include
)

target_compile_options(SOP_Star PRIVATE ${HOUDINI_CFLAGS})
target_link_options(SOP_Star PRIVATE ${HOUDINI_LDFLAGS})

set_target_properties(SOP_Star PROPERTIES
    SUFFIX ".dll"
    PREFIX ""
)
```

### Build

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

cmake -B build -G "NMake Makefiles" -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

## VEX Custom Functions

### VEX DSO

```cpp
#include <UT/UT_DSOVersion.h>
#include <VEX/VEX_VexOp.h>

static void my_vex_func(int argc, void* argv[], void*) {
    float* result = (float*)argv[0];
    float input = *(float*)argv[1];
    *result = input * 2.0f;
}

void newVEXOp(void*) {
    new VEX_VexOp("my_func@&FF", my_vex_func);
}
```

### Build VEX DSO

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

cl /c /O2 /MD /EHsc `
   /I"$env:HFS\toolkit\include" `
   my_vex_func.cpp

link /DLL /OUT:my_vex_func.dll `
     my_vex_func.obj `
     /LIBPATH:"$env:HFS\custom\houdini\dsolib"
```

## Library API Usage

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;

async fn build_houdini_plugin(houdini_version: &str) -> msvc_kit::Result<()> {
    let msvc_version = match houdini_version {
        "20.5" => "14.38",
        "20.0" => "14.34",
        "19.5" => "14.32",
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
    
    let hfs = format!("C:\\Program Files\\Side Effects Software\\Houdini {}.xxx", houdini_version);
    
    // Use hcustom
    Command::new(format!("{}\\bin\\hcustom.exe", hfs))
        .arg("SOP_MyNode.cpp")
        .status()?;
    
    Ok(())
}
```

## Installation

Copy the built DSO to Houdini's plugin directory:

```powershell
$HOUDINI_USER = "$env:USERPROFILE\Documents\houdini20.5"
Copy-Item SOP_Star.dll "$HOUDINI_USER\dso\"
```

Or set `HOUDINI_DSO_PATH`:

```powershell
$env:HOUDINI_DSO_PATH = "C:\MyPlugins;&"
```

## Troubleshooting

### "Cannot find Houdini headers"

Check HFS environment variable:

```powershell
echo $env:HFS
Test-Path "$env:HFS\toolkit\include\SOP\SOP_Node.h"
```

### Version Mismatch

Ensure MSVC version matches Houdini's build:

```powershell
& "$env:HFS\bin\houdini.exe" -version
```

### DSO Not Loading

Check Houdini's console for errors:

```
Houdini > Windows > Shell
```
