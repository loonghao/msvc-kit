# Build Script Examples

## Rust build.rs Integration

### Basic cc-rs Usage

After `msvc-kit setup`, the `cc` crate automatically finds MSVC:

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("src/native.c")
        .compile("native");
}
```

### With Custom Flags

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("src/native.cpp")
        .cpp(true)
        .flag("/O2")
        .flag("/EHsc")
        .define("WIN32", None)
        .include("vendor/include")
        .compile("native");
}
```

### Programmatic MSVC Setup

```rust
// build.rs
use std::process::Command;

fn main() {
    // Check if MSVC is available
    let cl_check = Command::new("cl").arg("/?").output();
    
    if cl_check.is_err() {
        // Setup MSVC using msvc-kit
        println!("cargo:warning=MSVC not found, please run: msvc-kit setup");
    }
    
    cc::Build::new()
        .file("src/native.c")
        .compile("native");
}
```

## Standalone Build Script

### PowerShell Build Script

```powershell
# build.ps1
param(
    [string]$Configuration = "Release",
    [string]$Arch = "x64"
)

# Setup MSVC
Write-Host "Setting up MSVC environment..."
msvc-kit setup --script --shell powershell | Invoke-Expression

# Create build directory
$BuildDir = "build\$Configuration"
New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null

# Compile
Write-Host "Compiling..."
$Sources = Get-ChildItem -Path "src" -Filter "*.cpp" -Recurse
foreach ($src in $Sources) {
    $obj = Join-Path $BuildDir ($src.BaseName + ".obj")
    cl /c /O2 /EHsc /MD /Fo"$obj" $src.FullName
}

# Link
Write-Host "Linking..."
$Objects = Get-ChildItem -Path $BuildDir -Filter "*.obj"
$ObjList = $Objects.FullName -join " "
link /OUT:"$BuildDir\app.exe" $ObjList

Write-Host "Build complete: $BuildDir\app.exe"
```

### Batch Build Script

```batch
@echo off
REM build.bat

REM Setup MSVC
for /f "delims=" %%i in ('msvc-kit setup --script --shell cmd') do %%i

REM Create build directory
if not exist build mkdir build

REM Compile
echo Compiling...
cl /c /O2 /EHsc /MD /Fobuild\ src\*.cpp

REM Link
echo Linking...
link /OUT:build\app.exe build\*.obj

echo Build complete: build\app.exe
```

## CMake Integration

### CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.20)
project(MyProject)

set(CMAKE_CXX_STANDARD 17)

# Source files
file(GLOB_RECURSE SOURCES "src/*.cpp")

# Executable
add_executable(myapp ${SOURCES})

# Windows-specific settings
if(WIN32)
    target_compile_options(myapp PRIVATE /O2 /EHsc /MD)
endif()
```

### Build with msvc-kit

```powershell
# setup-and-build.ps1

# Setup MSVC
msvc-kit setup --script --shell powershell | Invoke-Expression

# Configure
cmake -B build -G "NMake Makefiles" -DCMAKE_BUILD_TYPE=Release

# Build
cmake --build build

# Or use Ninja (faster)
cmake -B build -G "Ninja" -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

## Rust Library with Build Script

### Complete Example

```rust
// build.rs
use std::env;
use std::path::PathBuf;

fn main() {
    // Get output directory
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    // Build C library
    cc::Build::new()
        .file("native/math_ops.c")
        .file("native/string_ops.c")
        .include("native/include")
        .opt_level(2)
        .compile("native_ops");
    
    // Generate bindings (optional, with bindgen)
    #[cfg(feature = "bindgen")]
    {
        let bindings = bindgen::Builder::default()
            .header("native/include/ops.h")
            .generate()
            .expect("Unable to generate bindings");
        
        bindings
            .write_to_file(out_dir.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
    
    // Link libraries
    println!("cargo:rustc-link-lib=native_ops");
    println!("cargo:rustc-link-search={}", out_dir.display());
}
```

### Cargo.toml

```toml
[package]
name = "my-native-lib"
version = "0.1.0"
edition = "2021"

[build-dependencies]
cc = "1.0"
bindgen = { version = "0.69", optional = true }

[features]
default = []
bindgen = ["dep:bindgen"]
```

## Multi-Platform Build

### Cross-Platform build.rs

```rust
// build.rs
fn main() {
    let mut build = cc::Build::new();
    
    build
        .file("src/native.c")
        .include("include");
    
    // Platform-specific settings
    #[cfg(target_os = "windows")]
    {
        build
            .flag("/O2")
            .flag("/EHsc")
            .define("WIN32", None);
    }
    
    #[cfg(target_os = "linux")]
    {
        build
            .flag("-O2")
            .flag("-fPIC");
    }
    
    #[cfg(target_os = "macos")]
    {
        build
            .flag("-O2")
            .flag("-mmacosx-version-min=10.14");
    }
    
    build.compile("native");
}
```
