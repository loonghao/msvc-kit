# Configuration

Configuration uses TOML. `msvc-kit config` prints human-readable text, not importable JSON.

## File location

Windows: `%APPDATA%\loonghao\msvc-kit\config\config.toml`

Linux: `$XDG_CONFIG_HOME/msvc-kit/config.toml` (default `~/.config/msvc-kit/config.toml`).

macOS: `~/Library/Application Support/com.loonghao.msvc-kit/config.toml`.

Portable mode: `config.toml` next to `msvc-kit.exe`. See [Portable mode](#portable-mode).

`msvc-kit config` prints the file it resolved, including the reason:

```
Config file: C:\Users\me\AppData\Roaming\loonghao\msvc-kit\config\config.toml (per-user configuration directory)
```

## View and update

```powershell
msvc-kit config
msvc-kit config --set-dir 'D:\msvc-kit'
msvc-kit config --set-msvc 14.44 --set-sdk 10.0.26100.0
msvc-kit config --reset
```

## Portable mode

Portable mode stores `config.toml` next to the `msvc-kit` executable, which keeps
the settings with the binary when it is moved or copied to another machine.

```powershell
msvc-kit config --portable                       # move to the executable directory
msvc-kit config --portable --set-dir 'D:\kit'    # enable and set a value in one go
msvc-kit config --no-portable                    # back to the per-user directory
```

Enabling it creates a `msvc-kit.portable` marker file next to the executable and
copies the current settings into `<executable directory>\config.toml`. Every
later command reads and writes that file. Disabling it only removes the marker:
the portable `config.toml` and the per-user configuration are left untouched.
Creating the marker by hand has the same effect, so a portable directory can be
zipped and shipped:

```
msvc-kit.exe
msvc-kit.portable
config.toml
```

Only the configuration file moves. `install_dir` and `cache_dir` keep their
platform defaults until they are set, so a self-contained setup needs
`--set-dir` (and, if wanted, a `cache_dir` in the TOML).

## Choosing a configuration file

`--config` points a single run at any file or directory; it works with every
subcommand:

```powershell
msvc-kit --config 'D:\portable\my.toml' config --set-dir 'D:\kit'
msvc-kit --config 'D:\portable' config        # directory: uses D:\portable\config.toml
```

`MSVC_KIT_CONFIG` does the same for a whole session:

```powershell
$env:MSVC_KIT_CONFIG = 'D:\portable\my.toml'
msvc-kit config
Remove-Item Env:MSVC_KIT_CONFIG
```

Precedence for the configuration file: `--config` > `MSVC_KIT_CONFIG` > portable
mode > per-user configuration directory. An empty value is ignored. `--config`
cannot be combined with `--portable` or `--no-portable`.

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

Setting `cache_dir` moves the VS manifest cache: manifests are stored in `<cache_dir>/manifests/`.

## Environment and precedence

Directory precedence: explicit command `--target` / `--dir` > nonempty `MSVC_KIT_DIR` > saved `install_dir` > platform default. `MSVC_KIT_DIR` does not relocate the configuration file and is not persisted by configuration update commands. Empty values are ignored.

Cache directory precedence: explicit `cache_dir` from the TOML file > `<install_dir>/cache` when the installation directory is not the platform default (so both `MSVC_KIT_DIR` and `config --set-dir` relocate the cache) > platform default cache directory. `msvc-kit config` prints the cache directory in use.

Configuration file precedence: `--config` > nonempty `MSVC_KIT_CONFIG` > portable mode > per-user configuration directory. Empty values are ignored.

`MSVC_KIT_PORTABLE`: `1`, `true`, `yes` or `on` enables portable mode for a single run without creating the marker file.

```powershell
$env:MSVC_KIT_DIR = 'D:\msvc-kit'
msvc-kit download
Remove-Item Env:MSVC_KIT_DIR
```

`MSVC_KIT_INNER_PROGRESS`: show detailed extraction progress.
