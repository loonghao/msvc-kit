# Caching Mechanism

msvc-kit uses multiple caching strategies to minimize downloads and speed up operations.

## Cache Types

| Cache | Location | Purpose |
|-------|----------|---------|
| Download Index | `<install dir>/downloads/{msvc\|sdk}/.../index.db` | Track downloaded files |
| Manifest Cache | `<cache root>/manifests/` | VS manifest with ETag |
| Extraction Markers | `<install dir>/.msvc-kit-extracted/` | Skip re-extraction |

Default locations: the installation root is `%LOCALAPPDATA%\loonghao\msvc-kit\data`
on Windows, `$XDG_DATA_HOME/msvc-kit` on Linux and
`~/Library/Application Support/com.loonghao.msvc-kit` on macOS; the cache root is
`%LOCALAPPDATA%\loonghao\msvc-kit\cache` on Windows,
`$XDG_CACHE_HOME/msvc-kit` on Linux and `~/Library/Caches/com.loonghao.msvc-kit`
on macOS.

## Download Index

The download index is a [redb](https://github.com/cberner/redb) database that tracks:
- Downloaded file paths
- File hashes (SHA256)
- Download timestamps

### Skip Logic

When downloading, files are skipped based on:

1. **`cached`** - File exists in index with matching hash
2. **`304`** - Server returns Not Modified (ETag/Last-Modified match)
3. **`size match`** - File size matches expected (best-effort fallback)

::: tip
Size match is a best-effort optimization. Same size doesn't guarantee same content, but it's a reasonable heuristic for large binary packages.
:::

## Manifest Cache

VS manifests are cached under the configured cache directory: an explicit `cache_dir` from the TOML file, otherwise `<install_dir>/cache/manifests/` when the installation directory is not the platform default (`MSVC_KIT_DIR` or `config --set-dir`), otherwise the platform default cache root listed above. Run `msvc-kit config` to print the cache directory in use.

Manifests are cached with HTTP conditional requests:

```
GET /manifest.json
If-None-Match: "abc123"
If-Modified-Since: Mon, 01 Jan 2024 00:00:00 GMT
```

If the manifest hasn't changed, the server returns `304 Not Modified` and the cached version is used.

## Extraction Markers

After extracting a package, a marker file is created:

```
.msvc-kit-extracted/
├── package1.vsix.done
├── package2.msi.done
└── package3.cab.done
```

Re-running extraction skips packages with existing markers.

## Cache Management

### View Cache Size

```powershell
# Download cache
Get-ChildItem "$env:LOCALAPPDATA\loonghao\msvc-kit\data\downloads" -Recurse |
  Measure-Object Length -Sum |
  Select-Object @{N='Size (MB)';E={[math]::Round($_.Sum/1MB, 2)}}
```

### Clear Cache

```bash
# Clear download cache only
msvc-kit clean --cache

# Clear everything including cache
msvc-kit clean --all --cache
```

### Force Re-download

```bash
# Clear cache first
msvc-kit clean --cache

# Then download
msvc-kit download
```

## Environment Variables

| Variable | Effect |
|----------|--------|
| `MSVC_KIT_INNER_PROGRESS` | Set to `1`, `true`, `yes` or `on` to show detailed extraction progress |

`MSVC_KIT_INNER_PROGRESS` is the only cache-related variable the CLI reads.
`MSVC_KIT_PARALLEL_DOWNLOADS`, `MSVC_KIT_VERIFY_HASHES`, `MSVC_KIT_DRY_RUN`,
`MSVC_KIT_MSVC_VERSION`, `MSVC_KIT_SDK_VERSION`, `MSVC_KIT_INSTALL_DIR`,
`MSVC_KIT_INCLUDE_COMPONENTS` and `MSVC_KIT_EXCLUDE_PATTERNS` are read by the
library's `DownloadOptions::default()`; the CLI takes these settings from flags
and the configuration file instead.

## Debugging Cache Issues

Enable tracing to see cache decisions:

```bash
$env:RUST_LOG = "msvc_kit=debug"
msvc-kit download
```

Output shows:
```
DEBUG msvc_kit::downloader: Checking cache for package1.vsix
DEBUG msvc_kit::downloader: Cache hit: package1.vsix (hash match)
DEBUG msvc_kit::downloader: Downloading package2.vsix (not in cache)
```
