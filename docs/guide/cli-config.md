# Configuration

Configuration uses TOML. `msvc-kit config` prints human-readable text, not importable JSON.

## File location

Windows: `%APPDATA%\loonghao\msvc-kit\config\config.toml`

Linux: `$XDG_CONFIG_HOME/msvc-kit/config.toml` (default `~/.config/msvc-kit/config.toml`).

macOS: `~/Library/Application Support/com.loonghao.msvc-kit/config.toml`.

## View and update

```powershell
msvc-kit config
msvc-kit config --set-dir 'D:\msvc-kit'
msvc-kit config --set-msvc 14.44 --set-sdk 10.0.26100.0
msvc-kit config --reset
```

## TOML

```toml
install_dir = 'D:\msvc-kit'
default_msvc_version = "14.44"
default_sdk_version = "10.0.26100.0"
default_arch = "x64"
verify_hashes = true
parallel_downloads = 4
cache_dir = 'D:\msvc-kit\cache'
```

Optional version and cache fields may be omitted. There is no `default_host_arch` configuration field. To share settings, copy the actual TOML file and adjust machine-specific paths.

## Environment and precedence

Directory precedence: explicit command `--target` / `--dir` > nonempty `MSVC_KIT_DIR` > saved `install_dir` > platform default. The environment variable does not relocate the configuration file and is not persisted by configuration update commands. Empty values are ignored. The default cache follows the installation directory; an explicit custom cache is preserved.

```powershell
$env:MSVC_KIT_DIR = 'D:\msvc-kit'
msvc-kit download
Remove-Item Env:MSVC_KIT_DIR
```

`MSVC_KIT_INNER_PROGRESS`: show detailed extraction progress.
