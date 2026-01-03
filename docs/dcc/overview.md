# DCC Integration Overview

msvc-kit is particularly useful for building plugins for Digital Content Creation (DCC) applications. These applications often require specific MSVC versions that match their build environment.

## Why Use msvc-kit for DCC Development?

### Version Matching

DCC applications are compiled with specific MSVC versions. Using a mismatched compiler can cause:
- ABI incompatibilities
- Crashes at runtime
- Undefined behavior

### No Visual Studio Required

You don't need to install multiple Visual Studio versions. Just download the specific MSVC version you need.

### CI/CD Friendly

Build plugins in automated pipelines without heavy VS installations.

## MSVC Version Requirements

| Application | Version | Recommended MSVC |
|-------------|---------|------------------|
| Unreal Engine 5.4+ | 2024 | 14.38+ |
| Unreal Engine 5.0-5.3 | 2022-2023 | 14.34-14.36 |
| Maya 2025 | 2024 | 14.38 |
| Maya 2024 | 2023 | 14.34 |
| Maya 2023 | 2022 | 14.32 |
| Houdini 20.5 | 2024 | 14.38 |
| Houdini 20.0 | 2023 | 14.34 |
| 3ds Max 2025 | 2024 | 14.38 |
| 3ds Max 2024 | 2023 | 14.34 |
| Blender 4.x | 2024 | 14.38 |

::: warning
Always check the official documentation for your specific DCC version. Compiler requirements may change between releases.
:::

## General Workflow

### 1. Identify Required MSVC Version

Check your DCC's documentation or SDK for the required compiler version.

### 2. Download Matching MSVC

```bash
# Example: Maya 2025 requires MSVC 14.38
msvc-kit download --msvc-version 14.38
```

### 3. Setup Environment

```bash
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### 4. Build Your Plugin

Use CMake, MSBuild, or direct compiler invocation.

## Common Patterns

### CMake Build

```bash
msvc-kit setup --script --shell powershell | Invoke-Expression
cmake -B build -G "NMake Makefiles"
cmake --build build --config Release
```

### Direct Compilation

```bash
msvc-kit setup --script --shell powershell | Invoke-Expression
cl /c /O2 /EHsc plugin.cpp
link plugin.obj /DLL /OUT:plugin.dll
```

### With SDK Headers

Most DCC plugins need the application's SDK headers:

```bash
cl /c /O2 /EHsc /I"C:\Path\To\SDK\include" plugin.cpp
```

## Next Steps

- [Unreal Engine 5](./unreal-engine.md) - UE5 plugin development
- [Maya](./maya.md) - Maya plugin development
- [Houdini](./houdini.md) - Houdini plugin development
- [3ds Max](./3dsmax.md) - 3ds Max plugin development
- [Blender](./blender.md) - Blender addon development
