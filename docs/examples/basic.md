# Basic Usage Examples

## CLI Examples

### Download and Setup

```bash
# Install msvc-kit
cargo install msvc-kit

# Download latest MSVC and SDK
msvc-kit download

# Setup environment
msvc-kit setup --script --shell powershell | Invoke-Expression

# Verify
cl /?
```

### Compile a Simple Program

```powershell
# Create hello.c
@"
#include <stdio.h>
int main() {
    printf("Hello, World!\n");
    return 0;
}
"@ | Out-File -Encoding utf8 hello.c

# Compile
cl hello.c

# Run
.\hello.exe
```

### Compile C++ with Optimization

```powershell
# Create main.cpp
@"
#include <iostream>
#include <vector>
#include <algorithm>

int main() {
    std::vector<int> nums = {5, 2, 8, 1, 9};
    std::sort(nums.begin(), nums.end());
    for (int n : nums) std::cout << n << " ";
    return 0;
}
"@ | Out-File -Encoding utf8 main.cpp

# Compile with optimizations
cl /O2 /EHsc main.cpp

# Run
.\main.exe
```

## Library Examples

### Basic Download

```rust
use msvc_kit::{download_msvc, download_sdk, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    
    println!("Downloading MSVC...");
    let msvc = download_msvc(&options).await?;
    println!("MSVC installed to: {:?}", msvc.install_path);
    
    println!("Downloading SDK...");
    let sdk = download_sdk(&options).await?;
    println!("SDK installed to: {:?}", sdk.install_path);
    
    Ok(())
}
```

### Setup Environment

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Check tools
    if env.has_cl_exe() {
        println!("cl.exe found at: {:?}", env.cl_exe_path());
    }
    
    // Get environment strings
    println!("INCLUDE: {}", env.include_path_string());
    println!("LIB: {}", env.lib_path_string());
    
    Ok(())
}
```

### Generate Shell Script

```rust
use msvc_kit::{
    download_msvc, download_sdk, setup_environment,
    generate_activation_script, DownloadOptions, ShellType,
};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Generate PowerShell script
    let script = generate_activation_script(&env, ShellType::PowerShell);
    println!("{}", script);
    
    Ok(())
}
```

### Export to JSON

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Export environment to JSON
    let json = env.to_json();
    println!("{}", serde_json::to_string_pretty(&json)?);
    
    // Export install info
    let msvc_json = msvc.to_json();
    println!("{}", serde_json::to_string_pretty(&msvc_json)?);
    
    Ok(())
}
```

### Check Installed Versions

```rust
use msvc_kit::{MsvcVersion, SdkVersion};

fn main() {
    // List installed MSVC versions
    let msvc_versions = MsvcVersion::list_installed();
    println!("Installed MSVC versions:");
    for v in msvc_versions {
        println!("  {} at {:?}", v.version, v.path);
    }
    
    // List installed SDK versions
    let sdk_versions = SdkVersion::list_installed();
    println!("Installed SDK versions:");
    for v in sdk_versions {
        println!("  {} at {:?}", v.version, v.path);
    }
}
```
