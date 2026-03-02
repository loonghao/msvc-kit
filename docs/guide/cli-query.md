# Query Command

The `query` command inspects installed MSVC toolchain components and retrieves paths, environment variables, tool locations, and version information.

## Basic Usage

```bash
# Query all information about the installation
msvc-kit query

# Query with a specific installation directory
msvc-kit query --dir C:\msvc-kit
```

## Options

### Component Selection

```bash
# Query all components (default)
msvc-kit query --component all

# Query only MSVC compiler
msvc-kit query --component msvc

# Query only Windows SDK
msvc-kit query --component sdk
```

### Property Selection

You can filter what information to retrieve:

```bash
# Get all information (default)
msvc-kit query --property all

# Get installation paths only
msvc-kit query --property path

# Get environment variables
msvc-kit query --property env

# Get tool executable paths (cl.exe, link.exe, etc.)
msvc-kit query --property tools

# Get version information
msvc-kit query --property version

# Get include paths
msvc-kit query --property include

# Get library paths
msvc-kit query --property lib
```

**Property aliases:**

| Property | Aliases |
|----------|---------|
| `path` | `paths`, `install-path` |
| `env` | `environment`, `env-vars` |
| `tools` | `tool`, `executables` |
| `version` | `versions`, `ver` |
| `include` | `includes`, `include-paths` |
| `lib` | `libs`, `lib-paths` |

### Architecture

```bash
# Query for specific architecture (default: x64)
msvc-kit query --arch x64
msvc-kit query --arch x86
msvc-kit query --arch arm64
```

### Version Selection

```bash
# Query specific MSVC version
msvc-kit query --msvc-version 14.44

# Query specific SDK version
msvc-kit query --sdk-version 10.0.26100.0

# Query both
msvc-kit query --msvc-version 14.44 --sdk-version 10.0.26100.0
```

### Output Format

```bash
# Human-readable text (default)
msvc-kit query --format text

# JSON output (for scripting)
msvc-kit query --format json
```

## Examples

### Get cl.exe Path

```bash
# Text output
msvc-kit query --property tools --format text
# Output: cl=C:\msvc-kit\VC\Tools\MSVC\14.44.34823\bin\Hostx64\x64\cl.exe

# JSON output
msvc-kit query --property tools --format json
```

### Get Environment Variables for CI/CD

```bash
# JSON format for easy parsing
msvc-kit query --property env --format json
```

Output:
```json
{
  "INCLUDE": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\include;...",
  "LIB": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\lib\\x64;...",
  "PATH": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64;...",
  "VCToolsVersion": "14.44.34823",
  "VCINSTALLDIR": "C:\\msvc-kit\\VC",
  "WindowsSdkDir": "C:\\msvc-kit\\Windows Kits\\10",
  "WindowsSDKVersion": "10.0.26100.0\\"
}
```

### Get Version Information

```bash
msvc-kit query --property version
# Output:
# msvc=14.44.34823
# sdk=10.0.26100.0
```

### Get Installation Paths

```bash
msvc-kit query --property path
# Output:
# install_dir=C:\msvc-kit
# msvc_path=C:\msvc-kit\VC\Tools\MSVC\14.44.34823
# sdk_path=C:\msvc-kit\Windows Kits\10
```

### Get Include Paths for Build Configuration

```bash
msvc-kit query --property include
# Output (one path per line):
# C:\msvc-kit\VC\Tools\MSVC\14.44.34823\include
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\ucrt
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\shared
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\um
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\winrt
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\cppwinrt
```

### Use in Scripts

**PowerShell:**
```powershell
# Get cl.exe path
$tools = msvc-kit query --property tools --format json | ConvertFrom-Json
$clPath = $tools.cl
& $clPath /help

# Set environment variables
$env_vars = msvc-kit query --property env --format json | ConvertFrom-Json
$env_vars.PSObject.Properties | ForEach-Object {
    [Environment]::SetEnvironmentVariable($_.Name, $_.Value, "Process")
}
```

**Bash:**
```bash
# Get MSVC version
msvc-kit query --property version --format text | grep msvc | cut -d= -f2

# Export environment variables
eval $(msvc-kit query --property env --format text | sed 's/^/export /')
```

**CMake:**
```cmake
execute_process(
  COMMAND msvc-kit query --property tools --format json
  OUTPUT_VARIABLE MSVC_TOOLS_JSON
)
```

## Library API

The query functionality is also available as a Rust library API:

```rust
use msvc_kit::query::{QueryOptions, query_installation};
use msvc_kit::Architecture;

let options = QueryOptions::builder()
    .install_dir("C:/msvc-kit")
    .arch(Architecture::X64)
    .build();

let result = query_installation(&options)?;

// Access tool paths
if let Some(cl) = result.tool_path("cl") {
    println!("cl.exe: {}", cl.display());
}

// Access environment variables
for (key, value) in &result.env_vars {
    println!("{}={}", key, value);
}

// Access version information
println!("MSVC: {:?}", result.msvc_version());
println!("SDK: {:?}", result.sdk_version());
```

See [QueryResult API](/api/query-result) for full documentation.

## Complete Reference

```
msvc-kit query [OPTIONS]

Options:
  -d, --dir <DIR>                Installation directory
  -a, --arch <ARCH>              Target architecture [default: x64]
  -c, --component <COMPONENT>    Component to query (all, msvc, sdk) [default: all]
  -p, --property <PROPERTY>      Property to retrieve (all, path, env, tools, version, include, lib) [default: all]
      --msvc-version <VERSION>   Specific MSVC version to query
      --sdk-version <VERSION>    Specific SDK version to query
  -f, --format <FORMAT>          Output format (text, json) [default: text]
```
