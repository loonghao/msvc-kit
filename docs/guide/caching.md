# Caching Mechanism

msvc-kit uses multiple caching strategies to minimize downloads and speed up operations.

## Cache Types

| Cache | Location | Purpose |
|-------|----------|---------|
| Download Index | `downloads/{msvc\|sdk}/.../index.db` | Track downloaded files |
| Manifest Cache | `cache/manifests/` | VS manifest with ETag |
| Extraction Markers | `.msvc-kit-extracted/` | Skip re-extraction |

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

VS manifests are cached with HTTP conditional requests:

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
Get-ChildItem "$env:LOCALAPPDATA\loonghao\msvc-kit\downloads" -Recurse |
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
| `MSVC_KIT_INNER_PROGRESS` | Set to `1` to show detailed extraction progress |

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
