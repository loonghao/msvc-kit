# Custom Paths Examples

## Custom Installation Directory

### CLI

```bash
# Install to custom directory
msvc-kit download --target D:\BuildTools

# Setup from custom directory
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### Library

```rust
use msvc_kit::{download_msvc, download_sdk, DownloadOptions};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions {
        target_dir: PathBuf::from("D:\\BuildTools"),
        ..Default::default()
    };
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    
    println!("MSVC: {:?}", msvc.install_path);
    println!("SDK: {:?}", sdk.install_path);
    
    Ok(())
}
```

## Access Specific Paths

### Get Tool Executables

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Get individual tool paths
    let tools = env.tool_paths();
    
    println!("Compiler: {:?}", tools.cl);
    println!("Linker: {:?}", tools.link);
    println!("Lib Manager: {:?}", tools.lib);
    println!("Assembler: {:?}", tools.ml64);
    println!("Make: {:?}", tools.nmake);
    println!("Resource Compiler: {:?}", tools.rc);
    
    Ok(())
}
```

### Get Directory Paths

```rust
use msvc_kit::{download_msvc, download_sdk, DownloadOptions};

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    
    let msvc = download_msvc(&options).await?;
    println!("MSVC paths:");
    println!("  Install: {:?}", msvc.install_path);
    println!("  Bin: {:?}", msvc.bin_dir());
    println!("  Include: {:?}", msvc.include_dir());
    println!("  Lib: {:?}", msvc.lib_dir());
    
    let sdk = download_sdk(&options).await?;
    println!("SDK paths:");
    println!("  Install: {:?}", sdk.install_path);
    println!("  Bin: {:?}", sdk.bin_dir());
    println!("  Include: {:?}", sdk.include_dir());
    println!("  Lib: {:?}", sdk.lib_dir());
    
    Ok(())
}
```

## Save Paths for External Tools

### Export to Environment File

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions, get_env_vars};
use std::fs::File;
use std::io::Write;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Export as .env file
    let vars = get_env_vars(&env);
    let mut file = File::create("msvc.env")?;
    for (key, value) in vars {
        writeln!(file, "{}={}", key, value)?;
    }
    
    println!("Environment saved to msvc.env");
    Ok(())
}
```

### Export to JSON Config

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::fs;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Export full environment to JSON
    let json = env.to_json();
    fs::write("msvc-config.json", serde_json::to_string_pretty(&json)?)?;
    
    println!("Config saved to msvc-config.json");
    Ok(())
}
```

## Use Paths in Build Systems

### CMake Integration

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    // Set environment for CMake
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    
    let path = format!("{};{}", env.bin_path_string(), std::env::var("PATH")?);
    std::env::set_var("PATH", path);
    
    // Run CMake
    Command::new("cmake")
        .args(["-B", "build", "-G", "NMake Makefiles"])
        .status()?;
    
    Command::new("cmake")
        .args(["--build", "build"])
        .status()?;
    
    Ok(())
}
```

### Direct Compiler Invocation

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;

#[tokio::main]
async fn main() -> msvc_kit::Result<()> {
    let options = DownloadOptions::default();
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    let cl = env.cl_exe_path().expect("cl.exe not found");
    let link = env.link_exe_path().expect("link.exe not found");
    
    // Set paths
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    
    // Compile
    Command::new(&cl)
        .args(["/c", "/O2", "main.cpp"])
        .status()?;
    
    // Link
    Command::new(&link)
        .args(["main.obj", "/OUT:main.exe"])
        .status()?;
    
    Ok(())
}
```
