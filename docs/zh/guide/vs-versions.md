# Visual Studio 版本与 Channel

msvc-kit 从 Visual Studio 的 **channel manifest**（`https://aka.ms/vs/<大版本号>/release/channel`
返回的 JSON）中发现 MSVC 工具集与 Windows SDK 包。

msvc-kit 认识的每个 channel 都只存在于一张表 `VS_CHANNELS` 里，因此「支持某个
Visual Studio 版本」是**数据**，不是逻辑：

| 大版本 | 发行版本 | Channel URL |
|--------|----------|-------------|
| `18` | Visual Studio 2026 | `https://aka.ms/vs/18/release/channel` |
| `17` | Visual Studio 2022 | `https://aka.ms/vs/17/release/channel` |

## 选择 channel

默认使用 **auto**：从表头（最新）往表尾走，取第一个真正能返回可用 manifest 的
channel。上游还没发布的版本会被跳过并打印警告，所以新版本一旦发布就会被自动
采用。

```bash
# 默认：使用最新且可用的 channel
msvc-kit download
msvc-kit list --available

# 固定到某个 channel（大版本号或发布年份都可以）
msvc-kit download --vs-channel 17
msvc-kit download --vs-channel 2022
msvc-kit list --available --vs-channel 2026
```

可接受的写法：`17`、`v17`、`vs17`、`2022`、`auto`、`latest`。

也可以不改命令行就固定 channel：

```bash
# 环境变量
export MSVC_KIT_VS_CHANNEL=17

# 写入配置
msvc-kit config --set-vs-channel 2022
```

优先级：`--vs-channel` > `MSVC_KIT_VS_CHANNEL` > 配置文件 > auto。

## 上游 channel 尚未发布时

Microsoft 只在该版本可用后才发布 channel manifest。在那之前，
`https://aka.ms/vs/18/release/channel` 返回的是 HTML 页面而不是 JSON。msvc-kit
会识别这种情况并干净降级，而不是抛出解析错误：

- **auto 选择**：跳过该 channel，回退到下一个：

  ```text
  WARN Visual Studio channel(s) skipped as unavailable: Visual Studio 2026 (v18)
       (Visual Studio channel Visual Studio 2026 (v18) is not available
       (https://aka.ms/vs/18/release/channel): the server returned an HTML page
       instead of a JSON manifest (the channel is probably not published yet))
  Visual Studio channel: Visual Studio 2022 (v17)
  ```

- **固定选择**：返回带类型的错误 `ChannelUnavailable`，包含 URL 和原因；绝不
  会悄悄回退到另一个 Visual Studio 版本。

不可用的响应（HTML、空响应、非法 JSON、或没有包的 manifest）不会被留在
manifest 缓存里，因此上游一旦发布就能立刻生效。

## 新增一个 Visual Studio 版本

新增支持只是 `src/vs_channel.rs` 里的**一行数据**：

1. 打开 `src/vs_channel.rs`，在 `VS_CHANNELS` 表中加一条。保持表格**从新到旧**
   排序 —— auto 选择依赖这个顺序：

   ```rust
   pub const VS_CHANNELS: &[VsChannelEntry] = &[
       VsChannelEntry {
           major: 19,
           year: 2027,
           channel_url: "https://aka.ms/vs/19/release/channel",
       },
       VsChannelEntry {
           major: 18,
           year: 2026,
           channel_url: "https://aka.ms/vs/18/release/channel",
       },
       // ...
   ];
   ```

2. 其他什么都不用改。命令行选择（`--vs-channel 19`、`--vs-channel 2027`）、
   库 API、按 channel 隔离的 manifest 缓存（缓存文件为 `channel-v19.json`）以及
   「从新到旧」的自动回退都直接读这张表。

3. 补测试。`src/vs_channel.rs` 里已有表格测试，把新的大版本/年份加进去，例如
   扩展 `selectors_resolve_to_the_same_channel`。

4. 更新本页顶部的表格。

### 零改动逃生通道

**不在**表里的大版本号同样可用：它会通过 `VS_CHANNEL_URL_TEMPLATE`
（`https://aka.ms/vs/{major}/release/channel`）解析。也就是说
`msvc-kit download --vs-channel 19` 在任何代码改动之前就能指向新版本；补一行
只是补上友好的版本名，并让它成为 auto 选择的候选。

## 库 API

```rust
use msvc_kit::downloader::list_available_versions_with_selection;
use msvc_kit::vs_channel::{VsChannelSelection, VsChannelSpec};

// 从指定的 Visual Studio 版本发现包
let selection = VsChannelSelection::Pinned(VsChannelSpec::from_major(18));
let versions = list_available_versions_with_selection(selection).await?;
println!("served by: {:?}", versions.channel);

// 或者取最新可用 channel 提供的版本
let versions = msvc_kit::list_available_versions().await?;
# Ok::<(), msvc_kit::MsvcKitError>(())
```

`DownloadOptions::builder().vs_channel("2026")` 会为 `download_msvc` /
`download_sdk` 固定 channel；`VsManifest::fetch_with_selection` 会连同实际提供
数据的 channel 一起返回。

## 常见问题

| 现象 | 原因 | 处理 |
|------|------|------|
| `Unknown Visual Studio channel 'x'` | 选择器既不是大版本号，也不是发布年份或 `auto`/`latest`。 | 使用 `17`、`2022`、`auto` 等；错误信息里会列出已知 channel。 |
| `Visual Studio channel … is not available`（固定选择时） | 上游尚未发布该 channel manifest。 | 去掉该参数，或固定到旧版本如 `--vs-channel 2022`。 |
| auto 选择打印 "skipped as unavailable" | 更新的 channel 已登记但上游未发布。 | 提示信息，msvc-kit 已改用下一个 channel；固定 channel 即可消除。 |
