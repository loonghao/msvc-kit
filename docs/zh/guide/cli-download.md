# 下载命令

`download` 命令从 Microsoft 服务器下载 MSVC 编译器和/或 Windows SDK。

## 基本用法

```bash
# 下载最新的 MSVC 和 Windows SDK
msvc-kit download
```

## 选项

### 版本选择

```bash
# 指定 MSVC 版本
msvc-kit download --msvc-version 14.44

# 指定 SDK 版本
msvc-kit download --sdk-version 10.0.26100.0

# 同时指定
msvc-kit download --msvc-version 14.44 --sdk-version 10.0.26100.0
```

### 组件选择

```bash
# 仅下载 MSVC（跳过 SDK）
msvc-kit download --no-sdk

# 仅下载 SDK（跳过 MSVC）
msvc-kit download --no-msvc
```

### 目标目录

```bash
# 自定义安装目录
msvc-kit download --target C:\msvc-kit
```

### 架构

```bash
# 目标架构（默认：x64）
msvc-kit download --arch x64

# 主机架构（默认：自动检测）
msvc-kit download --host-arch x64

# 交叉编译：在 x64 主机上构建 ARM64
msvc-kit download --host-arch x64 --arch arm64
```

支持的架构：
- `x64` - 64 位 x86
- `x86` - 32 位 x86
- `arm64` - ARM64
- `arm` - ARM 32 位（仅目标）

### 下载选项

```bash
# 并行下载数（默认：4）
msvc-kit download --parallel-downloads 8

# 跳过哈希验证（不推荐）
msvc-kit download --no-verify
```

## 完整示例

```bash
msvc-kit download \
  --msvc-version 14.44 \
  --sdk-version 10.0.26100.0 \
  --target C:\msvc-kit \
  --arch x64 \
  --host-arch x64 \
  --parallel-downloads 8
```

## 下载进度

下载会显示每个包的进度：

```
正在下载 MSVC 包...
[1/15] Microsoft.VC.14.44.17.14.CRT.Headers.x64 (2.3 MB)
[2/15] Microsoft.VC.14.44.17.14.CRT.Source (1.1 MB)
...
```

## 缓存行为

下载会被缓存，如果已存在则跳过：

| 状态 | 含义 |
|------|------|
| `cached` | 文件存在于索引中且哈希匹配 |
| `304` | 服务器返回未修改（ETag 匹配） |
| `size match` | 文件大小匹配预期（尽力而为） |

要强制重新下载，请先使用 `msvc-kit clean --cache`。

## 可用版本

下载前列出可用版本：

```bash
msvc-kit list --available
```

输出：
```
可用的 MSVC 版本：
  14.44.34823
  14.43.34808
  14.42.34433
  ...

可用的 SDK 版本：
  10.0.26100.0
  10.0.22621.0
  10.0.22000.0
  ...
```
