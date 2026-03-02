# QueryResult API

The `query` module provides a structured API for querying installed MSVC and Windows SDK components.

## Core Types

### QueryOptions

Configuration for a query operation.

```rust
use msvc_kit::query::{QueryOptions, QueryComponent, QueryProperty};
use msvc_kit::Architecture;

let options = QueryOptions::builder()
    .install_dir("C:/msvc-kit")
    .arch(Architecture::X64)
    .component(QueryComponent::All)
    .property(QueryProperty::All)
    .msvc_version("14.44")
    .sdk_version("10.0.26100.0")
    .build();
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `install_dir` | `PathBuf` | `"msvc-kit"` | Installation directory to query |
| `arch` | `Architecture` | Host arch | Target architecture |
| `component` | `QueryComponent` | `All` | Which component to query |
| `property` | `QueryProperty` | `All` | What property to retrieve |
| `msvc_version` | `Option<String>` | `None` | Specific MSVC version (None = latest) |
| `sdk_version` | `Option<String>` | `None` | Specific SDK version (None = latest) |

### QueryComponent

```rust
pub enum QueryComponent {
    All,   // Query both MSVC and SDK
    Msvc,  // Query only MSVC compiler
    Sdk,   // Query only Windows SDK
}
```

Parsed from strings: `"all"`, `"msvc"`, `"sdk"`, `"winsdk"`

### QueryProperty

```rust
pub enum QueryProperty {
    All,      // Return all information
    Path,     // Installation paths
    Env,      // Environment variables
    Tools,    // Tool executable paths
    Version,  // Version information
    Include,  // Include paths
    Lib,      // Library paths
}
```

Parsed from strings with aliases:
- `"path"` / `"paths"` / `"install-path"`
- `"env"` / `"environment"` / `"env-vars"`
- `"tools"` / `"tool"` / `"executables"`
- `"version"` / `"versions"` / `"ver"`
- `"include"` / `"includes"` / `"include-paths"`
- `"lib"` / `"libs"` / `"lib-paths"`

### QueryResult

The result of a query operation, containing all discovered information.

```rust
pub struct QueryResult {
    pub install_dir: PathBuf,
    pub arch: String,
    pub msvc: Option<ComponentInfo>,
    pub sdk: Option<ComponentInfo>,
    pub env_vars: HashMap<String, String>,
    pub tools: HashMap<String, PathBuf>,
}
```

#### Methods

| Method | Return Type | Description |
|--------|------------|-------------|
| `tool_path(name)` | `Option<&PathBuf>` | Get path to a specific tool |
| `env_var(name)` | `Option<&String>` | Get a specific environment variable |
| `msvc_version()` | `Option<&str>` | Get MSVC version string |
| `sdk_version()` | `Option<&str>` | Get SDK version string |
| `msvc_install_path()` | `Option<&Path>` | Get MSVC installation path |
| `sdk_install_path()` | `Option<&Path>` | Get SDK installation path |
| `all_include_paths()` | `Vec<&PathBuf>` | Get all include paths |
| `all_lib_paths()` | `Vec<&PathBuf>` | Get all library paths |
| `to_json()` | `serde_json::Value` | Export as JSON |
| `format_summary()` | `String` | Human-readable summary |

### ComponentInfo

Information about a single installed component.

```rust
pub struct ComponentInfo {
    pub component_type: String,
    pub version: String,
    pub install_path: PathBuf,
    pub include_paths: Vec<PathBuf>,
    pub lib_paths: Vec<PathBuf>,
    pub bin_paths: Vec<PathBuf>,
}
```

## Functions

### query_installation

```rust
pub fn query_installation(options: &QueryOptions) -> Result<QueryResult>
```

Query an existing installation for component information.

**Example:**

```rust
use msvc_kit::query::{QueryOptions, query_installation};

let options = QueryOptions::builder()
    .install_dir("C:/msvc-kit")
    .build();

let result = query_installation(&options)?;

// Get cl.exe path
if let Some(cl) = result.tool_path("cl") {
    println!("cl.exe: {}", cl.display());
}

// Get all environment variables
for (key, value) in &result.env_vars {
    println!("{}={}", key, value);
}
```

## Available Tools

The following tool names can be queried via `tool_path()`:

| Name | Executable | Description |
|------|-----------|-------------|
| `cl` | `cl.exe` | C/C++ compiler |
| `link` | `link.exe` | Linker |
| `lib` | `lib.exe` | Static library manager |
| `ml64` | `ml64.exe` | MASM assembler (x64) |
| `nmake` | `nmake.exe` | Make utility |
| `rc` | `rc.exe` | Resource compiler |
| `mt` | `mt.exe` | Manifest tool |
| `dumpbin` | `dumpbin.exe` | Binary file dumper |
| `editbin` | `editbin.exe` | Binary file editor |

## Environment Variables

The `env_vars` field contains these standard variables:

| Variable | Example |
|----------|---------|
| `INCLUDE` | `C:\msvc-kit\VC\Tools\MSVC\14.44\include;...` |
| `LIB` | `C:\msvc-kit\VC\Tools\MSVC\14.44\lib\x64;...` |
| `PATH` | `C:\msvc-kit\VC\Tools\MSVC\14.44\bin\Hostx64\x64;...` |
| `VCToolsVersion` | `14.44.34823` |
| `VCToolsInstallDir` | `C:\msvc-kit\VC\Tools\MSVC\14.44.34823` |
| `VCINSTALLDIR` | `C:\msvc-kit\VC` |
| `WindowsSdkDir` | `C:\msvc-kit\Windows Kits\10` |
| `WindowsSDKVersion` | `10.0.26100.0\` |
| `WindowsSdkBinPath` | `C:\msvc-kit\Windows Kits\10\bin\10.0.26100.0` |
| `Platform` | `x64` |
