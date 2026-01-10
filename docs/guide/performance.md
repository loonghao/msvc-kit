# Performance Optimizations

msvc-kit includes several performance optimizations to speed up downloading and extracting MSVC and Windows SDK packages.

## Overview

| Optimization | Description | Expected Improvement |
|--------------|-------------|---------------------|
| Parallel Download | MSVC and SDK download simultaneously | 30-50% faster total time |
| Parallel Extraction | Multi-threaded package extraction | 2-4x faster extraction |
| Streaming Hash | Hash computed during download | Eliminates second file read |
| Connection Pooling | HTTP connection reuse | Reduced connection overhead |
| Optimized Buffers | Larger I/O buffers | Fewer system calls |
| RwLock Index | Read-write lock for download index | Reduced lock contention |

## Parallel Download

MSVC and Windows SDK packages are downloaded in parallel using `tokio::join!`:

```rust
// Downloads happen simultaneously
let (msvc_result, sdk_result) = tokio::join!(
    download_msvc(options),
    download_sdk(options)
);
```

This reduces total download time by 30-50% compared to sequential downloads.

## Parallel Extraction

Package extraction uses `futures::stream::buffer_unordered` for parallel processing:

- Automatically detects CPU core count
- Extracts multiple packages simultaneously
- Respects extraction markers for cached packages

```rust
// Parallel extraction with CPU-based concurrency
let parallel_count = std::thread::available_parallelism()
    .map(|n| n.get())
    .unwrap_or(4)
    .min(DEFAULT_PARALLEL_EXTRACTIONS);
```

## Streaming Hash Computation

Instead of downloading files and then reading them again for hash verification, msvc-kit computes SHA256 hashes while downloading:

```rust
// Hash computed during download - no second file read needed
while let Some(chunk) = stream.next().await {
    file.write_all(&chunk).await?;
    hasher.update(&chunk);  // Compute hash simultaneously
}
```

This eliminates a complete file read operation for every downloaded file.

## Connection Pooling

HTTP client is configured with connection pooling for better performance:

```rust
Client::builder()
    .pool_max_idle_per_host(10)
    .pool_idle_timeout(Duration::from_secs(90))
```

## Optimized Buffer Sizes

Buffer sizes are tuned for better throughput:

| Buffer | Size | Purpose |
|--------|------|---------|
| Hash computation | 4 MB | Reduces system calls during hashing |
| File extraction | 256 KB | Faster decompression |

## Adaptive Concurrency

Download concurrency automatically adjusts based on throughput:

- Monitors throughput per batch
- Reduces concurrency when throughput is low (< 2 MB/s)
- Increases concurrency when throughput is high (> 10 MB/s)
- Minimum concurrency: 2 connections

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MSVC_KIT_PARALLEL_DOWNLOADS` | 4 | Number of parallel downloads |
| `MSVC_KIT_VERIFY_HASHES` | true | Enable/disable hash verification |

### Library API

```rust
use msvc_kit::DownloadOptions;

let options = DownloadOptions::builder()
    .parallel_downloads(8)  // Increase parallel downloads
    .verify_hashes(true)    // Keep hash verification enabled
    .target_dir("C:/msvc-kit")
    .build();
```

## Benchmarks

Typical performance on a 100 Mbps connection:

| Operation | Before | After | Improvement |
|-----------|--------|-------|-------------|
| Download MSVC + SDK | ~8 min | ~5 min | ~40% faster |
| Extract packages | ~3 min | ~1 min | ~3x faster |
| Total installation | ~11 min | ~6 min | ~45% faster |

*Results vary based on network speed, CPU, and disk I/O.*

## Caching

msvc-kit uses multiple caching layers:

1. **Manifest Cache**: VS manifests cached with ETag/Last-Modified
2. **Download Index**: SQLite database tracking downloaded files
3. **Extraction Markers**: `.done` files marking extracted packages

See [Caching Guide](./caching.md) for more details.
