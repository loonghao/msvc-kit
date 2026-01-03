# Configuration

The `config` command manages persistent configuration settings.

## Config File Location

```
%LOCALAPPDATA%\loonghao\msvc-kit\config\config.json
```

## View Configuration

```bash
msvc-kit config
```

Output:
```json
{
  "install_dir": "C:\\Users\\user\\AppData\\Local\\loonghao\\msvc-kit",
  "default_msvc_version": null,
  "default_sdk_version": null,
  "default_arch": "x64",
  "default_host_arch": "x64"
}
```

## Set Options

### Installation Directory

```bash
msvc-kit config --set-dir C:\msvc-kit
```

### Default MSVC Version

```bash
msvc-kit config --set-msvc 14.44
```

### Default SDK Version

```bash
msvc-kit config --set-sdk 10.0.26100.0
```

### Multiple Options

```bash
msvc-kit config \
  --set-dir C:\msvc-kit \
  --set-msvc 14.44 \
  --set-sdk 10.0.26100.0
```

## Reset Configuration

```bash
msvc-kit config --reset
```

## Config File Format

```json
{
  "install_dir": "C:\\msvc-kit",
  "default_msvc_version": "14.44",
  "default_sdk_version": "10.0.26100.0",
  "default_arch": "x64",
  "default_host_arch": "x64"
}
```

## Environment Variable Override

Configuration can be overridden via environment variables:

| Variable | Description |
|----------|-------------|
| `MSVC_KIT_DIR` | Override installation directory |
| `MSVC_KIT_INNER_PROGRESS` | Show detailed extraction progress |

```bash
$env:MSVC_KIT_DIR = "D:\msvc-kit"
msvc-kit download  # Uses D:\msvc-kit
```

## Use Cases

### Team Configuration

Share a config file across team:

```bash
# Export config
msvc-kit config > team-config.json

# Import config (manually copy to config location)
copy team-config.json "$env:LOCALAPPDATA\loonghao\msvc-kit\config\config.json"
```

### CI/CD Configuration

```yaml
# GitHub Actions
- name: Configure msvc-kit
  run: |
    msvc-kit config --set-dir ${{ github.workspace }}/msvc-kit
    msvc-kit download
```
