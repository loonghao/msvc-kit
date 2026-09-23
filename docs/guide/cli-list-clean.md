# List & Clean Commands

## List Command

The `list` command shows installed and available versions.

### List Installed Versions

```bash
msvc-kit list
```

Output:
```
Installed versions in C:\msvc-kit

MSVC Compiler:
  - 14.44.34823

Windows SDK:
  - 10.0.26100.0
```

### List Available Versions

```bash
msvc-kit list --available

# Query a specific Visual Studio channel
msvc-kit list --available --vs-channel 2026
```

The command prints which Visual Studio channel served the versions. See
[Visual Studio Versions & Channels](./vs-versions.md) for channel selection.

Output (`--available` prints only the newest version of each component, not a
full list):

```
Fetching available versions from Microsoft...

Visual Studio channel: Visual Studio 2026 (v18)

Latest MSVC version: 14.44.34823
Latest Windows SDK version: 10.0.26100.0
```

Use the [library API](../api/library.md) (`list_available_versions`) to get the
full `msvc_versions` / `sdk_versions` vectors.

## Clean Command

The `clean` command removes installed components and cache.

### Remove Specific Version

```bash
# Remove specific MSVC version
msvc-kit clean --msvc-version 14.44

# Remove specific SDK version
msvc-kit clean --sdk-version 10.0.26100.0
```

### Remove All Versions

```bash
msvc-kit clean --all
```

### Clear Download Cache

```bash
# Clear cache only
msvc-kit clean --cache

# Remove all and clear cache
msvc-kit clean --all --cache
```

### What Gets Deleted

| Option | Deletes |
|--------|---------|
| `--msvc-version X` | `VC/Tools/MSVC/X/` directory |
| `--sdk-version X` | SDK files for version X |
| `--all` | All MSVC and SDK installations |
| `--cache` | `downloads/` directory |

### Dry Run

There is no dry-run flag - check files manually:

```powershell
ls "$env:LOCALAPPDATA\loonghao\msvc-kit\data"
```

## Disk Space

Check disk usage:

```powershell
# PowerShell
Get-ChildItem -Path "$env:LOCALAPPDATA\loonghao\msvc-kit\data" -Recurse |
  Measure-Object -Property Length -Sum |
  Select-Object @{N='Size (GB)';E={[math]::Round($_.Sum/1GB, 2)}}
```

Typical sizes:
- MSVC compiler: ~1-2 GB
- Windows SDK: ~2-3 GB
- Download cache: ~1-3 GB (can be cleared)
