# 配置

配置文件采用 TOML。`msvc-kit config` 显示可读文本，不是可重新导入的 JSON。

## 文件位置

Windows: `%APPDATA%\loonghao\msvc-kit\config\config.toml`

Linux: `$XDG_CONFIG_HOME/msvc-kit/config.toml` (default `~/.config/msvc-kit/config.toml`).

macOS: `~/Library/Application Support/com.loonghao.msvc-kit/config.toml`.

便携模式（portable）：`config.toml` 与 `msvc-kit.exe` 同目录，见[便携模式](#便携模式)。

`msvc-kit config` 会打印解析到的文件及来源：

```
Config file: C:\Users\me\AppData\Roaming\loonghao\msvc-kit\config\config.toml (per-user configuration directory)
```

## 查看与修改

```powershell
msvc-kit config
msvc-kit config --set-dir 'D:\msvc-kit'
msvc-kit config --set-msvc 14.44 --set-sdk 10.0.26100.0
msvc-kit config --reset
```

## 便携模式

便携模式把 `config.toml` 放在 `msvc-kit` 可执行文件旁边，配置随程序一起移动或复制到其他机器。

```powershell
msvc-kit config --portable                       # 切换到可执行文件所在目录
msvc-kit config --portable --set-dir 'D:\kit'    # 同时启用并写入设置
msvc-kit config --no-portable                    # 回到每用户配置目录
```

启用时会在可执行文件旁创建 `msvc-kit.portable` 标记文件，并把当前设置复制到
`<可执行文件目录>\config.toml`，之后所有命令都读写该文件。关闭时只删除标记文件，
便携 `config.toml` 与每用户配置都会保留。手动创建同名的标记文件效果相同，因此
便携目录可以直接打包分发：

```
msvc-kit.exe
msvc-kit.portable
config.toml
```

便携模式只移动配置文件。`install_dir` 与 `cache_dir` 仍使用平台默认值，
若要完全自包含，需要 `--set-dir`（以及在 TOML 中设置 `cache_dir`）。

## 指定配置文件

`--config` 可以让单次运行使用任意文件或目录，对所有子命令生效：

```powershell
msvc-kit --config 'D:\portable\my.toml' config --set-dir 'D:\kit'
msvc-kit --config 'D:\portable' config        # 目录：使用 D:\portable\config.toml
```

`MSVC_KIT_CONFIG` 对整个会话生效：

```powershell
$env:MSVC_KIT_CONFIG = 'D:\portable\my.toml'
msvc-kit config
Remove-Item Env:MSVC_KIT_CONFIG
```

配置文件优先级：`--config` > `MSVC_KIT_CONFIG` > 便携模式 > 每用户配置目录。
空值被忽略。`--config` 不能与 `--portable` / `--no-portable` 同时使用。

## TOML

```toml
install_dir = 'D:\msvc-kit'
default_msvc_version = "14.44"
default_sdk_version = "10.0.26100.0"
default_vs_channel = "2022"
default_arch = "x64"
verify_hashes = true
parallel_downloads = 4
cache_dir = 'D:\msvc-kit\cache'
```

可选版本、channel 与缓存字段可省略；没有 `default_host_arch` 配置字段。分享配置时复制实际 TOML 文件，并调整机器相关路径。

设置 `cache_dir` 会移动 VS 清单缓存：清单保存在 `<cache_dir>/manifests/`。

`default_vs_channel` 接受大版本号（`17`）、发布年份（`2022`）或 `auto`。
`msvc-kit config --set-vs-channel 2022` 会在写入前校验取值。
详见 [Visual Studio 版本与 Channel](./vs-versions.md)。

## 环境变量与优先级

目录优先级：命令的显式 `--target` / `--dir` > 非空 `MSVC_KIT_DIR` > 已保存的 `install_dir` > 平台默认值。`MSVC_KIT_DIR` 不改变配置文件位置，也不会通过配置修改命令持久化。空变量被忽略。

缓存目录优先级：TOML 中显式的 `cache_dir` > 安装目录不是平台默认目录时的 `<install_dir>/cache`（因此 `MSVC_KIT_DIR` 与 `config --set-dir` 都会让缓存随之移动）> 平台默认缓存根目录。默认缓存根目录：Windows 为 `%LOCALAPPDATA%\loonghao\msvc-kit\cache`，Linux 为 `$XDG_CACHE_HOME/msvc-kit`（默认 `~/.cache/msvc-kit`），macOS 为 `~/Library/Caches/com.loonghao.msvc-kit`。`msvc-kit config` 会打印正在使用的缓存目录。

配置文件优先级：`--config` > 非空 `MSVC_KIT_CONFIG` > 便携模式 > 每用户配置目录。空变量被忽略。

`MSVC_KIT_PORTABLE`：设为 `1`、`true`、`yes` 或 `on` 时只为单次运行启用便携模式，不创建标记文件。

```powershell
$env:MSVC_KIT_DIR = 'D:\msvc-kit'
msvc-kit download
Remove-Item Env:MSVC_KIT_DIR
```

`MSVC_KIT_INNER_PROGRESS`：设为 `1`、`true`、`yes` 或 `on` 时显示详细解压进度。

`MSVC_KIT_VS_CHANNEL`：固定 Visual Studio channel，优先级与 `--vs-channel` 一致
（flag > 环境变量 > `default_vs_channel` > auto）。
详见 [Visual Studio 版本与 Channel](./vs-versions.md)。
