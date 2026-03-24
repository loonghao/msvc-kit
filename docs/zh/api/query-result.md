# QueryResult API

`query` 模块提供了用于查询已安装 MSVC 和 Windows SDK 组件的结构化 API。

## 核心类型

### QueryOptions

查询操作的配置选项。

```rust
use msvc_kit::query::{QueryOptions, QueryComponent, QueryProperty};
use msvc_kit::Architecture;

let options = QueryOptions::builder()
    .install_dir("C:/msvc-kit")
    .arch(Architecture::X64)
    .component(QueryComponent::All)
    .property(QueryProperty::All)
    .msvc_version("14.44")
    .sdk_version("10.0.26100.0")
    .build();
```

| 字段 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `install_dir` | `PathBuf` | `"msvc-kit"` | 要查询的安装目录 |
| `arch` | `Architecture` | 主机架构 | 目标架构 |
| `component` | `QueryComponent` | `All` | 要查询的组件 |
| `property` | `QueryProperty` | `All` | 要获取的属性 |
| `msvc_version` | `Option<String>` | `None` | 指定 MSVC 版本（None = 最新） |
| `sdk_version` | `Option<String>` | `None` | 指定 SDK 版本（None = 最新） |

### QueryComponent

```rust
pub enum QueryComponent {
    All,   // 查询 MSVC 和 SDK
    Msvc,  // 仅查询 MSVC 编译器
    Sdk,   // 仅查询 Windows SDK
}
```

可从字符串解析：`"all"`、`"msvc"`、`"sdk"`、`"winsdk"`

### QueryProperty

```rust
pub enum QueryProperty {
    All,      // 返回所有信息
    Path,     // 安装路径
    Env,      // 环境变量
    Tools,    // 工具可执行文件路径
    Version,  // 版本信息
    Include,  // include 路径
    Lib,      // 库路径
}
```

支持别名的字符串解析：
- `"path"` / `"paths"` / `"install-path"`
- `"env"` / `"environment"` / `"env-vars"`
- `"tools"` / `"tool"` / `"executables"`
- `"version"` / `"versions"` / `"ver"`
- `"include"` / `"includes"` / `"include-paths"`
- `"lib"` / `"libs"` / `"lib-paths"`

### QueryResult

查询操作的结果，包含所有发现的信息。

```rust
pub struct QueryResult {
    pub install_dir: PathBuf,
    pub arch: String,
    pub msvc: Option<ComponentInfo>,
    pub sdk: Option<ComponentInfo>,
    pub env_vars: HashMap<String, String>,
    pub tools: HashMap<String, PathBuf>,
}
```

#### 方法

| 方法 | 返回类型 | 描述 |
|------|---------|------|
| `tool_path(name)` | `Option<&PathBuf>` | 获取指定工具的路径 |
| `env_var(name)` | `Option<&String>` | 获取指定环境变量的值 |
| `msvc_version()` | `Option<&str>` | 获取 MSVC 版本字符串 |
| `sdk_version()` | `Option<&str>` | 获取 SDK 版本字符串 |
| `msvc_install_path()` | `Option<&Path>` | 获取 MSVC 安装路径 |
| `sdk_install_path()` | `Option<&Path>` | 获取 SDK 安装路径 |
| `all_include_paths()` | `Vec<&PathBuf>` | 获取所有 include 路径 |
| `all_lib_paths()` | `Vec<&PathBuf>` | 获取所有库路径 |
| `to_json()` | `serde_json::Value` | 导出为 JSON |
| `format_summary()` | `String` | 人类可读的摘要 |

### ComponentInfo

单个已安装组件的信息。

```rust
pub struct ComponentInfo {
    pub component_type: String,
    pub version: String,
    pub install_path: PathBuf,
    pub include_paths: Vec<PathBuf>,
    pub lib_paths: Vec<PathBuf>,
    pub bin_paths: Vec<PathBuf>,
}
```

## 函数

### query_installation

```rust
pub fn query_installation(options: &QueryOptions) -> Result<QueryResult>
```

查询已有安装的组件信息。

**示例：**

```rust
use msvc_kit::query::{QueryOptions, query_installation};

let options = QueryOptions::builder()
    .install_dir("C:/msvc-kit")
    .build();

let result = query_installation(&options)?;

// 获取 cl.exe 路径
if let Some(cl) = result.tool_path("cl") {
    println!("cl.exe: {}", cl.display());
}

// 获取所有环境变量
for (key, value) in &result.env_vars {
    println!("{}={}", key, value);
}
```

## 可查询的工具

以下工具名可通过 `tool_path()` 查询：

| 名称 | 可执行文件 | 描述 |
|------|-----------|------|
| `cl` | `cl.exe` | C/C++ 编译器 |
| `link` | `link.exe` | 链接器 |
| `lib` | `lib.exe` | 静态库管理器 |
| `ml64` | `ml64.exe` | MASM 汇编器 (x64) |
| `nmake` | `nmake.exe` | Make 工具 |
| `rc` | `rc.exe` | 资源编译器 |
| `mt` | `mt.exe` | 清单工具 |
| `dumpbin` | `dumpbin.exe` | 二进制文件转储工具 |
| `editbin` | `editbin.exe` | 二进制文件编辑工具 |

## 环境变量

`env_vars` 字段包含以下标准变量：

| 变量 | 示例 |
|------|------|
| `INCLUDE` | `C:\msvc-kit\VC\Tools\MSVC\14.44\include;...` |
| `LIB` | `C:\msvc-kit\VC\Tools\MSVC\14.44\lib\x64;...` |
| `PATH` | `C:\msvc-kit\VC\Tools\MSVC\14.44\bin\Hostx64\x64;...` |
| `VCToolsVersion` | `14.44.34823` |
| `VCToolsInstallDir` | `C:\msvc-kit\VC\Tools\MSVC\14.44.34823` |
| `VCINSTALLDIR` | `C:\msvc-kit\VC` |
| `WindowsSdkDir` | `C:\msvc-kit\Windows Kits\10` |
| `WindowsSDKVersion` | `10.0.26100.0\` |
| `WindowsSdkBinPath` | `C:\msvc-kit\Windows Kits\10\bin\10.0.26100.0` |
| `Platform` | `x64` |
