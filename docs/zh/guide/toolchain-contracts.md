# 工具链选择与诊断

`setup`、`env`、`query`、`doctor`、`run`、`lock` 和 `cmake` 共用版本选择规则：CLI 优先于配置，未指定时选择已安装的最新数字版本。指定版本不存在会失败；前缀按点分段匹配。`--host-arch` 决定编译器与 SDK 工具的运行架构，`--arch` 决定输出与库的架构。

```powershell
msvc-kit doctor --dir C:/tools/msvc --msvc-version 14.44 --sdk-version 10.0.26100.0 --arch x64 --host-arch x64 --compile --format json
msvc-kit lock --dir C:/tools/msvc --output msvc-kit.lock.json
msvc-kit run --dir C:/tools/msvc --lockfile msvc-kit.lock.json -- cmake --build build
```

`query --format json` 返回 `env_vars`、`tools`、完整版本、host/target 和 fingerprint；`env --format json` 仍是扁平变量映射。fingerprint 仅表示版本与架构身份，不证明文件完整性。

`doctor` 的 JSON schema 为 `msvc-kit.doctor.v1`，失败退出码为 10。默认检查工具、头文件和库；`--compile` 执行 C++ 编译、资源编译、链接与 manifest 嵌入，并在本机架构匹配时运行结果。交叉目标执行明确标记为跳过，各子进程有 30 秒超时。

`run` 只为子进程设置选定环境，保留继承 PATH 和子进程退出码。`cmake --output` 生成 Ninja 使用的工具链文件，配置和构建均应通过相同选择的 `run` 启动。VS/MSBuild 生成器仍需注册的兼容 VS 安装。

版本锁保存精确版本和架构。CLI 下载成功后生成含 URL、大小、SHA256 的来源凭据，锁文件可捕获这些凭据，后续下载在解包前验证。已有 VS 安装没有来源凭据时，生成的是仅选择版本的锁，`receipts` 为空；不宣称来源已固定。微软撤回旧包时锁定下载会失败，来源哈希也不证明解包后的目录未被修改。

当前定位是用户态 MSVC + SDK 工具链。WDK/EWDK 的驱动开发、签名、测试和 VS 扩展要求不纳入默认安装。DCC 消费者分别负责 CRT、OpenMP、Maya ABI、UE 兼容性和 Unity Windows IL2CPP 条件检查。

完整细节见[英文说明](../../guide/toolchain-contracts.md)。
