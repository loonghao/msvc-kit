# 性能优化

msvc-kit 包含多项性能优化，以加速 MSVC 和 Windows SDK 包的下载和解压。

## 概述

| 优化项 | 描述 | 预期提升 |
|--------|------|---------|
| 并行下载 | MSVC 和 SDK 同时下载 | 总时间减少 30-50% |
| 并行解压 | 多线程包解压 | 解压速度提升 2-4 倍 |
| 流式哈希 | 下载时同时计算哈希 | 消除二次文件读取 |
| 连接池 | HTTP 连接复用 | 减少连接开销 |
| 优化缓冲区 | 更大的 I/O 缓冲区 | 减少系统调用 |
| 读写锁索引 | 下载索引使用读写锁 | 减少锁竞争 |

## 并行下载

MSVC 和 Windows SDK 包使用 `tokio::join!` 并行下载：

```rust
// 同时下载
let (msvc_result, sdk_result) = tokio::join!(
    download_msvc(options),
    download_sdk(options)
);
```

与顺序下载相比，总下载时间减少 30-50%。

## 并行解压

包解压使用 `futures::stream::buffer_unordered` 进行并行处理：

- 自动检测 CPU 核心数
- 同时解压多个包
- 尊重已缓存包的解压标记

```rust
// 基于 CPU 核心数的并行解压
let parallel_count = std::thread::available_parallelism()
    .map(|n| n.get())
    .unwrap_or(4)
    .min(DEFAULT_PARALLEL_EXTRACTIONS);
```

## 流式哈希计算

msvc-kit 在下载时同时计算 SHA256 哈希，而不是下载后再读取文件进行验证：

```rust
// 下载时计算哈希 - 无需二次读取文件
while let Some(chunk) = stream.next().await {
    file.write_all(&chunk).await?;
    hasher.update(&chunk);  // 同时计算哈希
}
```

这消除了每个下载文件的完整文件读取操作。

## 连接池

HTTP 客户端配置了连接池以提高性能：

```rust
Client::builder()
    .pool_max_idle_per_host(10)
    .pool_idle_timeout(Duration::from_secs(90))
```

## 优化的缓冲区大小

缓冲区大小经过调优以获得更好的吞吐量：

| 缓冲区 | 大小 | 用途 |
|--------|------|------|
| 哈希计算 | 4 MB | 减少哈希计算时的系统调用 |
| 文件解压 | 256 KB | 更快的解压速度 |

## 自适应并发

下载并发度根据吞吐量自动调整：

- 监控每批次的吞吐量
- 吞吐量低时（< 2 MB/s）降低并发度
- 吞吐量高时（> 10 MB/s）提高并发度
- 最小并发度：2 个连接

## 配置

### 环境变量

| 变量 | 默认值 | 描述 |
|------|--------|------|
| `MSVC_KIT_PARALLEL_DOWNLOADS` | 4 | 并行下载数 |
| `MSVC_KIT_VERIFY_HASHES` | true | 启用/禁用哈希验证 |

### 库 API

```rust
use msvc_kit::DownloadOptions;

let options = DownloadOptions::builder()
    .parallel_downloads(8)  // 增加并行下载数
    .verify_hashes(true)    // 保持哈希验证启用
    .target_dir("C:/msvc-kit")
    .build();
```

## 性能基准

在 100 Mbps 连接上的典型性能：

| 操作 | 优化前 | 优化后 | 提升 |
|------|--------|--------|------|
| 下载 MSVC + SDK | ~8 分钟 | ~5 分钟 | 快约 40% |
| 解压包 | ~3 分钟 | ~1 分钟 | 快约 3 倍 |
| 总安装时间 | ~11 分钟 | ~6 分钟 | 快约 45% |

*结果因网络速度、CPU 和磁盘 I/O 而异。*

## 缓存

msvc-kit 使用多层缓存：

1. **清单缓存**：VS 清单使用 ETag/Last-Modified 缓存
2. **下载索引**：SQLite 数据库跟踪已下载文件
3. **解压标记**：`.done` 文件标记已解压的包
