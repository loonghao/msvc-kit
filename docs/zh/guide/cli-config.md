# 配置

配置文件采用 TOML。`msvc-kit config` 显示可读文本，不是可重新导入的 JSON。

## 文件位置

Windows: `%APPDATA%\loonghao\msvc-kit\config\config.toml`

Linux: `$XDG_CONFIG_HOME/msvc-kit/config.toml` (default `~/.config/msvc-kit/config.toml`).

macOS: `~/Library/Application Support/com.loonghao.msvc-kit/config.toml`.

## 查看与修改

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

可选版本和缓存字段可省略；没有 `default_host_arch` 配置字段。分享配置时复制实际 TOML 文件，并调整机器相关路径。

## 环境变量与优先级

目录优先级：命令的显式 `--target` / `--dir` > 非空 `MSVC_KIT_DIR` > 已保存的 `install_dir` > 平台默认值。环境变量不改变配置文件位置，也不会通过配置修改命令持久化。空变量被忽略。默认缓存随安装目录移动，显式自定义缓存保持不变。

```powershell
$env:MSVC_KIT_DIR = 'D:\msvc-kit'
msvc-kit download
Remove-Item Env:MSVC_KIT_DIR
```

`MSVC_KIT_INNER_PROGRESS`: 显示详细解压进度。
