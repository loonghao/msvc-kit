# List & Clean Commands

## List Command

The `list` command shows installed and available versions.

### List Installed Versions

```bash
msvc-kit list
```

Output:
```
Installed MSVC versions:
  14.44.34823 (C:\msvc-kit\VC\Tools\MSVC\14.44.34823)

Installed SDK versions:
  10.0.26100.0 (C:\msvc-kit\Windows Kits\10)
```

### List Available Versions

```bash
msvc-kit list --available
```

Output:
```
Available MSVC versions:
  14.44.34823 (latest)
  14.43.34808
  14.42.34433
  14.41.34120
  14.40.33807
  ...

Available SDK versions:
  10.0.26100.0 (latest)
  10.0.22621.0
  10.0.22000.0
  10.0.19041.0
  ...
```

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

To see what would be deleted without actually deleting:

```bash
# Not yet implemented - check files manually
ls "$env:LOCALAPPDATA\loonghao\msvc-kit"
```

## Disk Space

Check disk usage:

```powershell
# PowerShell
Get-ChildItem -Path "$env:LOCALAPPDATA\loonghao\msvc-kit" -Recurse |
  Measure-Object -Property Length -Sum |
  Select-Object @{N='Size (GB)';E={[math]::Round($_.Sum/1GB, 2)}}
```

Typical sizes:
- MSVC compiler: ~1-2 GB
- Windows SDK: ~2-3 GB
- Download cache: ~1-3 GB (can be cleared)
