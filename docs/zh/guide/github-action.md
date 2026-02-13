# GitHub Action

msvc-kit 提供了官方的 GitHub Action，让你在 CI/CD 中轻松设置 MSVC 编译环境。

## 基本用法

```yaml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup MSVC Build Tools
        uses: loonghao/msvc-kit@v1
        with:
          arch: x64

      - name: Build
        run: |
          cl /nologo test.c
```

## 指定版本

```yaml
      - name: Setup MSVC Build Tools
        uses: loonghao/msvc-kit@v1
        with:
          msvc-version: "14.44"
          sdk-version: "10.0.26100.0"
          arch: x64
```

## 矩阵构建

```yaml
name: Matrix Build

on: [push]

jobs:
  build:
    runs-on: windows-latest
    strategy:
      matrix:
        arch: [x64, x86, arm64]
    steps:
      - uses: actions/checkout@v4

      - name: Setup MSVC Build Tools
        id: msvc
        uses: loonghao/msvc-kit@v1
        with:
          arch: ${{ matrix.arch }}

      - name: 显示安装版本
        run: |
          echo "MSVC: ${{ steps.msvc.outputs.msvc-version }}"
          echo "SDK: ${{ steps.msvc.outputs.sdk-version }}"
          echo "cl.exe: ${{ steps.msvc.outputs.cl-path }}"
```

## 配合缓存使用

```yaml
      - name: Cache MSVC
        uses: actions/cache@v4
        with:
          path: ${{ steps.msvc.outputs.install-dir }}
          key: msvc-${{ matrix.arch }}-${{ steps.msvc.outputs.msvc-version }}

      - name: Setup MSVC Build Tools
        id: msvc
        uses: loonghao/msvc-kit@v1
        with:
          arch: ${{ matrix.arch }}
```

## Rust + cc-rs 集成

Action 会自动设置 `CC` 和 `CXX` 环境变量，实现与 Rust/cc-rs 的无缝兼容：

```yaml
      - name: Setup MSVC Build Tools
        uses: loonghao/msvc-kit@v1
        with:
          arch: x64

      - name: 构建包含 C 依赖的 Rust 项目
        run: cargo build --release
```

## 输入参数

| 参数 | 描述 | 默认值 |
|------|------|--------|
| `msvc-version` | MSVC 版本（空 = 最新） | `""` |
| `sdk-version` | Windows SDK 版本（空 = 最新） | `""` |
| `arch` | 目标架构 | `x64` |
| `host-arch` | 主机架构（空 = 自动检测） | `""` |
| `install-dir` | 安装目录 | `$RUNNER_TEMP/msvc-kit` |
| `msvc-kit-version` | msvc-kit 二进制版本 | `latest` |
| `components` | 组件：`all`、`msvc` 或 `sdk` | `all` |
| `verify-hashes` | 验证文件哈希 | `true` |
| `export-env` | 导出环境变量到 GITHUB_ENV | `true` |

## 输出参数

| 输出 | 描述 |
|------|------|
| `msvc-version` | 已安装的 MSVC 版本 |
| `sdk-version` | 已安装的 SDK 版本 |
| `install-dir` | 安装目录 |
| `cl-path` | cl.exe 路径 |
| `link-path` | link.exe 路径 |
| `rc-path` | rc.exe 路径 |
| `include-path` | INCLUDE 环境变量值 |
| `lib-path` | LIB 环境变量值 |
