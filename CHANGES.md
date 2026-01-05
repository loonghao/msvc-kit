# 退出代码测试 - 变更清单

## 添加的文件

### 测试文件
1. **`tests/cli_exit_code_tests.rs`** (主要交付物)
   - 15 个测试用例验证 CLI 退出代码行为
   - 使用 `rstest` 框架进行参数化测试
   - 覆盖所有命令的成功和失败场景
   - 特别关注 winget 兼容性（无参数时退出码为 0）

### 文档文件
2. **`docs/exit-code-behavior.md`**
   - 详细说明退出代码行为
   - 解释 winget 兼容性要求
   - 提供退出代码矩阵
   - 包含实现参考和最佳实践

3. **`tests/README.md`**
   - 测试套件总览
   - 所有测试文件的描述
   - 运行测试的命令说明
   - CI 集成信息

4. **`EXIT_CODE_TESTS_SUMMARY.md`**
   - 实现总结
   - 为什么这些测试很重要
   - 测试结构说明
   - 下一步计划

5. **`TEST_EXIT_CODES.md`** (快速参考指南)
   - 快速开始指南
   - 测试分类表格
   - 手动验证步骤
   - 常见问题解答

### 脚本文件
6. **`run_exit_code_tests.ps1`**
   - PowerShell 脚本用于构建和运行测试
   - 彩色输出显示测试结果
   - 错误处理和退出代码传递

7. **`CHANGES.md`** (本文件)
   - 变更清单和文件列表

## 测试覆盖

### ✅ 成功场景（退出码 0）
- 无子命令时打印帮助
- `--help` 和 `--version` 标志
- 所有子命令的帮助页
- `config` 命令及其选项
- `list` 空目录
- `clean` 不存在的版本（幂等操作）

### ❌ 错误场景（退出码 ≠ 0）
- 无效子命令
- `bundle` 缺少 `--accept-license`
- `setup` 没有安装
- `env` 没有安装
- 无效的架构参数

## winget 兼容性

这些测试确保 `msvc-kit` 满足 Windows Package Manager (winget) 的要求：
- ✅ 无参数运行时退出码为 0
- ✅ 打印帮助或使用信息
- ✅ 标准的帮助和版本标志行为

**关键代码** (`src/bin/msvc-kit.rs` 第 228-235 行)：
```rust
let command = match cli.command {
    Some(cmd) => cmd,
    None => {
        // Print help and exit with code 0 for winget validation
        Cli::command().print_help().unwrap();
        std::process::exit(0);
    }
};
```

## 运行测试

```bash
# 运行所有退出代码测试
cargo test --test cli_exit_code_tests

# 详细输出
cargo test --test cli_exit_code_tests -- --nocapture

# 使用 PowerShell 脚本
./run_exit_code_tests.ps1
```

## CI 集成

测试已自动集成到现有 CI 流程：
- ✅ `ci.yml` - 通过 `cargo test --all-features`
- ✅ `pr-checks.yml` - PR 检查的一部分
- ✅ `release.yml` - 发布前验证

无需修改 CI 配置文件。

## 技术细节

### 测试框架
- **rstest** - 参数化测试和 fixtures
- **tempfile** - 临时目录创建
- **std::process::Command** - 生成 CLI 进程

### 测试方法
```rust
fn run_command(args: &[&str]) -> std::io::Result<std::process::Output>
```
- 查找编译后的二进制文件
- 运行命令并捕获输出
- 验证退出码和输出内容

### 断言示例
```rust
assert!(output.status.success(), "Expected exit code 0");
assert!(!output.status.success(), "Expected non-zero exit code");
```

## 文件大小

```
tests/cli_exit_code_tests.rs     ~10 KB  (主测试文件)
docs/exit-code-behavior.md       ~5 KB   (技术文档)
tests/README.md                  ~3 KB   (测试总览)
EXIT_CODE_TESTS_SUMMARY.md       ~4 KB   (实现总结)
TEST_EXIT_CODES.md               ~6 KB   (快速指南)
run_exit_code_tests.ps1          ~500 B  (测试脚本)
CHANGES.md                       ~4 KB   (本文件)
```

**总计：** ~7 个新文件，~32 KB 文档和测试代码

## 下一步

1. ✅ 测试文件已创建
2. ✅ 文档已完善
3. ⏳ 本地运行测试验证
4. ⏳ 提交 PR 并通过 CI
5. ⏳ 更新 CHANGELOG.md
6. ⏳ 考虑添加实际的 winget 清单文件

## 相关资源

- [winget 清单创作指南](https://github.com/microsoft/winget-pkgs/blob/master/AUTHORING_MANIFESTS.md)
- [rstest 文档](https://docs.rs/rstest/latest/rstest/)
- [Rust 测试指南](https://doc.rust-lang.org/book/ch11-00-testing.html)

## 贡献者注意事项

在添加新命令或修改现有命令时：
1. 确保适当的退出代码（0 表示成功，非 0 表示错误）
2. 在 `cli_exit_code_tests.rs` 中添加相应测试
3. 更新 `docs/exit-code-behavior.md` 中的退出代码矩阵
4. 本地运行测试验证行为：`cargo test --test cli_exit_code_tests`
