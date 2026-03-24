# 查询命令

`query` 命令用于检查已安装的 MSVC 工具链组件，并获取路径、环境变量、工具位置和版本信息。

## 基本用法

```bash
# 查询安装的所有信息
msvc-kit query

# 指定安装目录查询
msvc-kit query --dir C:\msvc-kit
```

## 选项

### 组件选择

```bash
# 查询所有组件（默认）
msvc-kit query --component all

# 仅查询 MSVC 编译器
msvc-kit query --component msvc

# 仅查询 Windows SDK
msvc-kit query --component sdk
```

### 属性选择

可以筛选要获取的信息类型：

```bash
# 获取所有信息（默认）
msvc-kit query --property all

# 仅获取安装路径
msvc-kit query --property path

# 获取环境变量
msvc-kit query --property env

# 获取工具可执行文件路径（cl.exe、link.exe 等）
msvc-kit query --property tools

# 获取版本信息
msvc-kit query --property version

# 获取 include 路径
msvc-kit query --property include

# 获取库路径
msvc-kit query --property lib
```

**属性别名：**

| 属性 | 别名 |
|------|------|
| `path` | `paths`、`install-path` |
| `env` | `environment`、`env-vars` |
| `tools` | `tool`、`executables` |
| `version` | `versions`、`ver` |
| `include` | `includes`、`include-paths` |
| `lib` | `libs`、`lib-paths` |

### 架构

```bash
# 查询指定架构（默认：x64）
msvc-kit query --arch x64
msvc-kit query --arch x86
msvc-kit query --arch arm64
```

### 版本选择

```bash
# 查询指定 MSVC 版本
msvc-kit query --msvc-version 14.44

# 查询指定 SDK 版本
msvc-kit query --sdk-version 10.0.26100.0

# 同时指定两者
msvc-kit query --msvc-version 14.44 --sdk-version 10.0.26100.0
```

### 输出格式

```bash
# 人类可读文本（默认）
msvc-kit query --format text

# JSON 输出（适用于脚本）
msvc-kit query --format json
```

## 示例

### 获取 cl.exe 路径

```bash
# 文本输出
msvc-kit query --property tools --format text
# 输出：cl=C:\msvc-kit\VC\Tools\MSVC\14.44.34823\bin\Hostx64\x64\cl.exe

# JSON 输出
msvc-kit query --property tools --format json
```

### 获取 CI/CD 环境变量

```bash
# JSON 格式便于解析
msvc-kit query --property env --format json
```

输出：
```json
{
  "INCLUDE": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\include;...",
  "LIB": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\lib\\x64;...",
  "PATH": "C:\\msvc-kit\\VC\\Tools\\MSVC\\14.44.34823\\bin\\Hostx64\\x64;...",
  "VCToolsVersion": "14.44.34823",
  "VCINSTALLDIR": "C:\\msvc-kit\\VC",
  "WindowsSdkDir": "C:\\msvc-kit\\Windows Kits\\10",
  "WindowsSDKVersion": "10.0.26100.0\\"
}
```

### 获取版本信息

```bash
msvc-kit query --property version
# 输出：
# msvc=14.44.34823
# sdk=10.0.26100.0
```

### 获取安装路径

```bash
msvc-kit query --property path
# 输出：
# install_dir=C:\msvc-kit
# msvc_path=C:\msvc-kit\VC\Tools\MSVC\14.44.34823
# sdk_path=C:\msvc-kit\Windows Kits\10
```

### 获取 include 路径用于构建配置

```bash
msvc-kit query --property include
# 输出（每行一个路径）：
# C:\msvc-kit\VC\Tools\MSVC\14.44.34823\include
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\ucrt
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\shared
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\um
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\winrt
# C:\msvc-kit\Windows Kits\10\Include\10.0.26100.0\cppwinrt
```

### 在脚本中使用

**PowerShell：**
```powershell
# 获取 cl.exe 路径
$tools = msvc-kit query --property tools --format json | ConvertFrom-Json
$clPath = $tools.cl
& $clPath /help

# 设置环境变量
$env_vars = msvc-kit query --property env --format json | ConvertFrom-Json
$env_vars.PSObject.Properties | ForEach-Object {
    [Environment]::SetEnvironmentVariable($_.Name, $_.Value, "Process")
}
```

**Bash：**
```bash
# 获取 MSVC 版本
msvc-kit query --property version --format text | grep msvc | cut -d= -f2

# 导出环境变量
eval $(msvc-kit query --property env --format text | sed 's/^/export /')
```

**CMake：**
```cmake
execute_process(
  COMMAND msvc-kit query --property tools --format json
  OUTPUT_VARIABLE MSVC_TOOLS_JSON
)
```

## 库 API

查询功能也可通过 Rust 库 API 使用：

```rust
use msvc_kit::query::{QueryOptions, query_installation};
use msvc_kit::Architecture;

let options = QueryOptions::builder()
    .install_dir("C:/msvc-kit")
    .arch(Architecture::X64)
    .build();

let result = query_installation(&options)?;

// 访问工具路径
if let Some(cl) = result.tool_path("cl") {
    println!("cl.exe: {}", cl.display());
}

// 访问环境变量
for (key, value) in &result.env_vars {
    println!("{}={}", key, value);
}

// 访问版本信息
println!("MSVC: {:?}", result.msvc_version());
println!("SDK: {:?}", result.sdk_version());
```

完整文档请参阅 [QueryResult API](/zh/api/query-result)。

## 完整参考

```
msvc-kit query [OPTIONS]

选项：
  -d, --dir <DIR>                安装目录
  -a, --arch <ARCH>              目标架构 [默认：x64]
  -c, --component <COMPONENT>    要查询的组件 (all, msvc, sdk) [默认：all]
  -p, --property <PROPERTY>      要获取的属性 (all, path, env, tools, version, include, lib) [默认：all]
      --msvc-version <VERSION>   指定 MSVC 版本
      --sdk-version <VERSION>    指定 SDK 版本
  -f, --format <FORMAT>          输出格式 (text, json) [默认：text]
```
