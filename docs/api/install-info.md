# InstallInfo

Information about an installed component (MSVC or SDK).

## Definition

```rust
pub struct InstallInfo {
    /// Component type: "msvc" or "sdk"
    pub component_type: String,
    
    /// Installed version
    pub version: String,
    
    /// Installation path
    pub install_path: PathBuf,
    
    /// List of downloaded files
    pub downloaded_files: Vec<PathBuf>,
    
    /// Target architecture
    pub arch: Architecture,
}
```

## Methods

### Validation

```rust
/// Check if the installation is valid (path exists)
pub fn is_valid(&self) -> bool;

/// Get the total size of downloaded files
pub fn total_size(&self) -> u64;
```

### Path Accessors

```rust
/// Get the bin directory for this component
pub fn bin_dir(&self) -> PathBuf;

/// Get the include directory for this component
pub fn include_dir(&self) -> PathBuf;

/// Get the lib directory for this component
pub fn lib_dir(&self) -> PathBuf;
```

### Export

```rust
/// Export install info to JSON
pub fn to_json(&self) -> serde_json::Value;
```

## Usage Examples

### Basic Usage

```rust
use msvc_kit::{download_msvc, DownloadOptions};

let options = DownloadOptions::default();
let info = download_msvc(&options).await?;

println!("Component: {}", info.component_type);  // "msvc"
println!("Version: {}", info.version);           // "14.44.34823"
println!("Path: {:?}", info.install_path);
println!("Valid: {}", info.is_valid());
println!("Size: {} bytes", info.total_size());
```

### Access Directories

```rust
let info = download_msvc(&options).await?;

// Get specific directories
let bin = info.bin_dir();      // .../bin/Hostx64/x64
let include = info.include_dir();  // .../include
let lib = info.lib_dir();      // .../lib/x64

println!("cl.exe should be at: {:?}", bin.join("cl.exe"));
```

### Export to JSON

```rust
let info = download_msvc(&options).await?;
let json = info.to_json();

println!("{}", serde_json::to_string_pretty(&json)?);
```

Output:
```json
{
  "component_type": "msvc",
  "version": "14.44.34823",
  "install_path": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823",
  "bin_dir": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64",
  "include_dir": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\include",
  "lib_dir": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\lib\\x64",
  "arch": "x64",
  "is_valid": true,
  "total_size": 1234567890
}
```

### MSVC vs SDK Paths

The path structure differs between MSVC and SDK:

**MSVC:**
```
install_path: VC/Tools/MSVC/14.xx.xxxxx/
bin_dir:      VC/Tools/MSVC/14.xx.xxxxx/bin/Hostx64/x64/
include_dir:  VC/Tools/MSVC/14.xx.xxxxx/include/
lib_dir:      VC/Tools/MSVC/14.xx.xxxxx/lib/x64/
```

**SDK:**
```
install_path: Windows Kits/10/
bin_dir:      Windows Kits/10/bin/10.0.xxxxx.0/x64/
include_dir:  Windows Kits/10/Include/10.0.xxxxx.0/
lib_dir:      Windows Kits/10/Lib/10.0.xxxxx.0/um/x64/
```

## Serialization

`InstallInfo` implements `Serialize` and `Deserialize`:

```rust
use serde_json;

let info = download_msvc(&options).await?;

// Serialize
let json = serde_json::to_string(&info)?;

// Deserialize
let restored: InstallInfo = serde_json::from_str(&json)?;
```
