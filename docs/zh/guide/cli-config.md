# 配置

`config` 命令管理持久化配置设置。

## 配置文件位置

```
%LOCALAPPDATA%\loonghao\msvc-kit\config\config.json
```

## 查看配置

```bash
msvc-kit config
```

输出：
```json
{
  "install_dir": "C:\\Users\\user\\AppData\\Local\\loonghao\\msvc-kit",
  "default_msvc_version": null,
  "default_sdk_version": null,
  "default_arch": "x64",
  "default_host_arch": "x64"
}
```

## 设置选项

### 安装目录

```bash
msvc-kit config --set-dir C:\msvc-kit
```

### 默认 MSVC 版本

```bash
msvc-kit config --set-msvc 14.44
```

### 默认 SDK 版本

```bash
msvc-kit config --set-sdk 10.0.26100.0
```

### 多个选项

```bash
msvc-kit config \
  --set-dir C:\msvc-kit \
  --set-msvc 14.44 \
  --set-sdk 10.0.26100.0
```

## 重置配置

```bash
msvc-kit config --reset
```

## 配置文件格式

```json
{
  "install_dir": "C:\\msvc-kit",
  "default_msvc_version": "14.44",
  "default_sdk_version": "10.0.26100.0",
  "default_arch": "x64",
  "default_host_arch": "x64"
}
```

## 环境变量覆盖

配置可以通过环境变量覆盖：

| 变量 | 说明 |
|------|------|
| `MSVC_KIT_DIR` | 覆盖安装目录 |
| `MSVC_KIT_INNER_PROGRESS` | 显示详细解压进度 |

```bash
$env:MSVC_KIT_DIR = "D:\msvc-kit"
msvc-kit download  # 使用 D:\msvc-kit
```

## 使用场景

### 团队配置

在团队间共享配置文件：

```bash
# 导出配置
msvc-kit config > team-config.json

# 导入配置（手动复制到配置位置）
copy team-config.json "$env:LOCALAPPDATA\loonghao\msvc-kit\config\config.json"
```

### CI/CD 配置

```yaml
# GitHub Actions
- name: 配置 msvc-kit
  run: |
    msvc-kit config --set-dir ${{ github.workspace }}/msvc-kit
    msvc-kit download
```
