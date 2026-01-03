# Setup Environment

The `setup` command configures environment variables for the MSVC toolchain.

## Basic Usage

### PowerShell

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression
```

### CMD

```cmd
msvc-kit setup --script --shell cmd > setup.bat && setup.bat
```

### Bash (WSL/Git Bash)

```bash
eval "$(msvc-kit setup --script --shell bash)"
```

## Options

### Shell Type

```bash
--shell <SHELL>  # powershell, cmd, bash
```

### Script Output

```bash
--script         # Output as shell script (for eval)
```

### Persistent Setup (Windows Registry)

```bash
msvc-kit setup --persistent
```

::: warning
`--persistent` requires Administrator privileges and modifies the Windows registry.
:::

## Print Environment Variables

Use the `env` subcommand to print environment variables without applying them:

```bash
# As shell script
msvc-kit env

# As JSON (for parsing)
msvc-kit env --format json
```

### JSON Output Example

```json
{
  "VCINSTALLDIR": "C:\\msvc-kit\\VC",
  "VCToolsInstallDir": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823",
  "VCToolsVersion": "14.44.34823",
  "WindowsSdkDir": "C:\\msvc-kit\\Windows Kits\\10",
  "WindowsSDKVersion": "10.0.26100.0\\",
  "INCLUDE": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\include;...",
  "LIB": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\lib\\x64;...",
  "PATH": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64;..."
}
```

## Integration with Build Tools

### cc-rs (Rust)

After `msvc-kit setup`, the `cc` crate automatically finds the MSVC toolchain:

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("src/native.c")
        .compile("native");
}
```

### CMake

```bash
msvc-kit setup --script --shell powershell | Invoke-Expression
cmake -B build -G "NMake Makefiles"
cmake --build build
```

### MSBuild

```bash
msvc-kit setup --script --shell powershell | Invoke-Expression
msbuild MyProject.vcxproj /p:Configuration=Release
```

## Profile Integration

### PowerShell Profile

Add to `$PROFILE`:

```powershell
# Auto-setup MSVC environment
if (Get-Command msvc-kit -ErrorAction SilentlyContinue) {
    msvc-kit setup --script --shell powershell | Invoke-Expression
}
```

### Bash Profile

Add to `~/.bashrc`:

```bash
# Auto-setup MSVC environment
if command -v msvc-kit &> /dev/null; then
    eval "$(msvc-kit setup --script --shell bash)"
fi
```
