# 安装

## 系统要求

| 要求 | 最低版本 |
|------|----------|
| 操作系统 | Windows 10 1809+ / Windows 11 |
| 架构 | x64 或 ARM64 |
| 磁盘空间 | ~2 GB（仅 MSVC）/ ~5 GB（MSVC + SDK） |
| Rust | 1.70+（仅 cargo 安装需要） |

## 安装方式

### 通过 Winget（推荐）

在 Windows 上安装最简单的方式：

```powershell
winget install loonghao.msvc-kit
```

### 通过 PowerShell 脚本

一行命令安装：

```powershell
irm https://github.com/loonghao/msvc-kit/releases/latest/download/install.ps1 | iex
```

此脚本会自动下载最新版本并安装到你的 PATH 中。

### 通过 Cargo

如果你已安装 Rust：

```bash
cargo install msvc-kit
```

这会从 crates.io 安装最新稳定版本。

### 预编译二进制文件

从 [GitHub Releases](https://github.com/loonghao/msvc-kit/releases) 下载：

```powershell
# PowerShell - 下载并解压到 cargo bin 目录
Invoke-WebRequest -Uri "https://github.com/loonghao/msvc-kit/releases/latest/download/msvc-kit-x86_64-pc-windows-msvc.zip" -OutFile msvc-kit.zip
Expand-Archive msvc-kit.zip -DestinationPath $env:USERPROFILE\.cargo\bin -Force
```

### 从源码编译

```bash
git clone https://github.com/loonghao/msvc-kit.git
cd msvc-kit
cargo install --path .
```

## 验证安装

```bash
msvc-kit --version
# msvc-kit 0.1.x

msvc-kit --help
```

## 卸载

### 移除 CLI

```bash
# 如果通过 Winget 安装
winget uninstall loonghao.msvc-kit

# 如果通过 Cargo 安装
cargo uninstall msvc-kit
```

### 移除下载的组件

```bash
# 移除所有已安装版本和缓存
msvc-kit clean --all --cache

# 或手动删除数据目录
rm -rf "$env:LOCALAPPDATA\loonghao\msvc-kit"
```

### 移除配置

```powershell
rm "$env:LOCALAPPDATA\loonghao\msvc-kit\config\config.json"
```

## 故障排除

### Cargo 安装失败

如果 `cargo install` 因链接器错误失败，你可能需要 MSVC 来构建 msvc-kit 本身。请改用预编译二进制文件或 Winget。

### 权限被拒绝

如果遇到权限问题，请以管理员身份运行 PowerShell。

### 网络问题

如果下载失败，请检查：
- 防火墙设置（允许 HTTPS 访问 Microsoft CDN）
- 代理配置
- 尝试使用 `--parallel-downloads 1` 减少连接数

```bash
msvc-kit download --parallel-downloads 1
```
