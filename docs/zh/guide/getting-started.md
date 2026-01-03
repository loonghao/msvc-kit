# 快速开始

本指南将帮助你安装 msvc-kit 并设置你的第一个 MSVC 环境。

## 前提条件

- Windows 10/11（x64 或 ARM64）
- ~2-5 GB 磁盘空间用于 MSVC + SDK

## 安装

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

### 通过 Cargo

如果你已安装 Rust：

```bash
cargo install msvc-kit
```

### 从源码编译

```bash
git clone https://github.com/loonghao/msvc-kit.git
cd msvc-kit
cargo install --path .
```

## 快速开始

### 1. 下载 MSVC 和 Windows SDK

```bash
msvc-kit download
```

这会下载：
- 最新的 MSVC 编译器（cl.exe、link.exe 等）
- 最新的 Windows SDK（头文件、库、工具）

默认位置：`%LOCALAPPDATA%\loonghao\msvc-kit\`

### 2. 配置环境

#### PowerShell（推荐）

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression
```

#### CMD

```cmd
msvc-kit setup --script --shell cmd > setup.bat && setup.bat
```

#### Bash（WSL/Git Bash）

```bash
eval "$(msvc-kit setup --script --shell bash)"
```

### 3. 验证安装

```bash
# 检查 cl.exe 是否可用
cl /?

# 检查版本
cl
# Microsoft (R) C/C++ Optimizing Compiler Version 19.xx.xxxxx for x64
```

## 安装内容

运行 `msvc-kit download` 后，目录结构如下：

```
%LOCALAPPDATA%\loonghao\msvc-kit\
├── VC/
│   └── Tools/
│       └── MSVC/
│           └── 14.xx.xxxxx/
│               ├── bin/
│               │   └── Hostx64/
│               │       └── x64/
│               │           ├── cl.exe
│               │           ├── link.exe
│               │           └── ...
│               ├── include/
│               └── lib/
├── Windows Kits/
│   └── 10/
│       ├── bin/
│       │   └── 10.0.xxxxx.0/
│       ├── Include/
│       │   └── 10.0.xxxxx.0/
│       └── Lib/
│           └── 10.0.xxxxx.0/
└── downloads/
    ├── msvc/
    └── sdk/
```

## 设置的环境变量

执行 `msvc-kit setup` 后，会配置以下环境变量：

| 变量 | 说明 |
|------|------|
| `VCINSTALLDIR` | VC 安装目录 |
| `VCToolsInstallDir` | VC 工具目录 |
| `VCToolsVersion` | VC 工具版本 |
| `WindowsSdkDir` | Windows SDK 目录 |
| `WindowsSDKVersion` | Windows SDK 版本 |
| `INCLUDE` | 编译器包含路径 |
| `LIB` | 链接器库路径 |
| `PATH` | 更新后包含 bin 目录 |

## 下一步

- [下载选项](./cli-download.md) - 自定义下载行为
- [配置](./cli-config.md) - 持久化配置
- [库 API](../api/library.md) - 作为 Rust 库使用
