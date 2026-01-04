# 架构支持

msvc-kit 支持多种 CPU 架构，包括主机（构建机器）和目标（输出二进制文件）架构。

## 支持的架构

| 架构 | 作为主机 | 作为目标 | 描述 |
|------|---------|---------|------|
| `x64` | ✅ | ✅ | 64 位 x86 (AMD64) |
| `x86` | ✅ | ✅ | 32 位 x86 |
| `arm64` | ✅ | ✅ | ARM 64 位 |
| `arm` | ❌ | ✅ | ARM 32 位（仅作为目标） |

## 主机架构 vs 目标架构

- **主机架构**：构建机器的 CPU 架构
- **目标架构**：编译后二进制文件的 CPU 架构

### 原生编译

构建机器和输出具有相同的架构：

```bash
# 在 x64 机器上，构建 x64 二进制文件
msvc-kit download --host-arch x64 --arch x64
```

### 交叉编译

在一种架构上为另一种架构构建：

```bash
# 在 x64 机器上，构建 ARM64 二进制文件
msvc-kit download --host-arch x64 --arch arm64

# 在 x64 机器上，构建 x86（32 位）二进制文件
msvc-kit download --host-arch x64 --arch x86
```

## 架构过滤

msvc-kit 会根据您指定的架构智能过滤下载的包，显著减少下载大小和安装时间。

### 下载内容

当您指定架构（例如 `--arch x64`）时，msvc-kit 只下载：

| 包类型 | 过滤行为 |
|--------|---------|
| **工具** | 仅匹配的主机/目标组合（例如 `HostX64.TargetX64`） |
| **CRT 库** | 仅匹配的架构（例如 `CRT.x64.Desktop`） |
| **MFC/ATL** | 仅匹配的架构（例如 `MFC.x64`、`ATL.x64`） |
| **头文件** | 始终包含（架构无关） |
| **Spectre 库** | 默认排除（很少需要） |

### 排除内容

以下内容会自动排除以最小化下载大小：

- **其他架构**：针对 x64 时排除 ARM64、x86、ARM 包
- **Spectre 缓解库**：`.Spectre` 后缀的包
- **冗余包**：重复的架构变体

### 下载大小对比

| 配置 | 大约大小 |
|------|---------|
| 所有架构（旧行为） | ~2.5 GB |
| 单一架构（x64） | ~300-500 MB |
| 最小化（仅工具） | ~150-250 MB |

## 可用工具

### MSVC 编译器工具

下载并设置环境后，以下 MSVC 工具可用：

| 工具 | 描述 |
|------|------|
| `cl.exe` | C/C++ 编译器 |
| `link.exe` | 链接器 |
| `lib.exe` | 静态库管理器 |
| `ml64.exe` | MASM 汇编器（x64） |
| `ml.exe` | MASM 汇编器（x86） |
| `nmake.exe` | Microsoft make 工具 |

### Windows SDK 工具

Windows SDK 包含用于开发和部署的额外工具：

| 工具 | 描述 |
|------|------|
| `rc.exe` | 资源编译器 |
| `signtool.exe` | 代码签名工具 |
| `mt.exe` | 清单工具 |
| `makecat.exe` | 目录创建工具 |
| `makecert.exe` | 证书创建工具 |
| `certutil.exe` | 证书工具 |
| `mc.exe` | 消息编译器 |
| `midl.exe` | MIDL 编译器 |

### 使用 SDK 工具

激活环境后，SDK 工具在 PATH 中可用：

```bash
# 激活环境
msvc-kit setup --script --shell powershell | Invoke-Expression

# 签名可执行文件
signtool sign /a /t http://timestamp.digicert.com myapp.exe

# 编译资源
rc /fo resources.res resources.rc

# 创建清单
mt -manifest app.manifest -outputresource:myapp.exe;1
```

### 工具路径

工具位于以下位置：

```
# MSVC 工具
{install_dir}/VC/Tools/MSVC/{version}/bin/Host{host_arch}/{target_arch}/

# SDK 工具
{install_dir}/Windows Kits/10/bin/{sdk_version}/{arch}/
```

## 目录结构

MSVC 使用特定的目录结构进行交叉编译：

```
VC/Tools/MSVC/14.xx/bin/
├── Hostx64/
│   ├── x64/      # x64 主机 → x64 目标（原生）
│   ├── x86/      # x64 主机 → x86 目标（交叉）
│   └── arm64/    # x64 主机 → ARM64 目标（交叉）
├── Hostx86/
│   ├── x86/      # x86 主机 → x86 目标（原生）
│   └── x64/      # x86 主机 → x64 目标（交叉）
└── Hostarm64/
    ├── arm64/    # ARM64 主机 → ARM64 目标（原生）
    ├── x64/      # ARM64 主机 → x64 目标（交叉）
    └── x86/      # ARM64 主机 → x86 目标（交叉）
```

## 自动检测

默认情况下，msvc-kit 会自动检测您的主机架构：

```bash
# 自动检测主机，目标 x64
msvc-kit download --arch x64
```

## 常见场景

### 在 64 位 Windows 上构建 32 位程序

```bash
msvc-kit download --host-arch x64 --arch x86
msvc-kit setup --script --shell powershell | Invoke-Expression

# cl.exe 现在针对 x86
cl /c myfile.cpp  # 生成 x86 目标文件
```

### 为 Windows on ARM 构建 ARM64 程序

```bash
# 在 x64 机器上
msvc-kit download --host-arch x64 --arch arm64
msvc-kit setup --script --shell powershell | Invoke-Expression

# cl.exe 现在针对 ARM64
cl /c myfile.cpp  # 生成 ARM64 目标文件
```

### 多架构设置

下载多个架构：

```bash
# 为多个目标下载
msvc-kit download --arch x64
msvc-kit download --arch x86
msvc-kit download --arch arm64
```

## 库 API

```rust
use msvc_kit::{DownloadOptions, Architecture};

let options = DownloadOptions::builder()
    .arch(Architecture::X64)
    .host_arch(Architecture::X64)
    .target_dir("C:/msvc-kit")
    .build();

// 只会下载 x64 包
let info = msvc_kit::download_msvc(&options).await?;
```

### 交叉编译示例

```rust
use msvc_kit::{DownloadOptions, Architecture};

// 在 x64 主机上构建 ARM64 二进制文件
let options = DownloadOptions::builder()
    .arch(Architecture::Arm64)
    .host_arch(Architecture::X64)
    .target_dir("C:/msvc-kit")
    .build();

// 下载：HostX64.TargetARM64 工具 + ARM64 CRT/MFC/ATL
let info = msvc_kit::download_msvc(&options).await?;
```
