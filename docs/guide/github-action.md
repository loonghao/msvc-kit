# GitHub Action

msvc-kit provides an official GitHub Action for easily setting up the MSVC build environment in CI/CD pipelines.

## Basic Usage

```yaml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup MSVC Build Tools
        uses: loonghao/msvc-kit@v1
        with:
          arch: x64

      - name: Build
        run: |
          cl /nologo test.c
```

## Specific Versions

```yaml
      - name: Setup MSVC Build Tools
        uses: loonghao/msvc-kit@v1
        with:
          msvc-version: "14.44"
          sdk-version: "10.0.26100.0"
          arch: x64
```

## Matrix Build

```yaml
name: Matrix Build

on: [push]

jobs:
  build:
    runs-on: windows-latest
    strategy:
      matrix:
        arch: [x64, x86, arm64]
    steps:
      - uses: actions/checkout@v4

      - name: Setup MSVC Build Tools
        id: msvc
        uses: loonghao/msvc-kit@v1
        with:
          arch: ${{ matrix.arch }}

      - name: Show installed versions
        run: |
          echo "MSVC: ${{ steps.msvc.outputs.msvc-version }}"
          echo "SDK: ${{ steps.msvc.outputs.sdk-version }}"
          echo "cl.exe: ${{ steps.msvc.outputs.cl-path }}"
```

## With Caching

```yaml
      - name: Cache MSVC
        uses: actions/cache@v4
        with:
          path: ${{ steps.msvc.outputs.install-dir }}
          key: msvc-${{ matrix.arch }}-${{ steps.msvc.outputs.msvc-version }}

      - name: Setup MSVC Build Tools
        id: msvc
        uses: loonghao/msvc-kit@v1
        with:
          arch: ${{ matrix.arch }}
```

## Rust + cc-rs Integration

The action automatically sets `CC` and `CXX` environment variables for seamless Rust/cc-rs compatibility:

```yaml
      - name: Setup MSVC Build Tools
        uses: loonghao/msvc-kit@v1
        with:
          arch: x64

      - name: Build Rust project with C dependencies
        run: cargo build --release
```

## Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `msvc-version` | MSVC version (empty = latest) | `""` |
| `sdk-version` | Windows SDK version (empty = latest) | `""` |
| `arch` | Target architecture | `x64` |
| `host-arch` | Host architecture (empty = auto-detect) | `""` |
| `install-dir` | Installation directory | `$RUNNER_TEMP/msvc-kit` |
| `msvc-kit-version` | msvc-kit binary version | `latest` |
| `components` | Components: `all`, `msvc`, or `sdk` | `all` |
| `verify-hashes` | Verify file hashes | `true` |
| `export-env` | Export env vars to GITHUB_ENV | `true` |

## Outputs

| Output | Description |
|--------|-------------|
| `msvc-version` | Installed MSVC version |
| `sdk-version` | Installed SDK version |
| `install-dir` | Installation directory |
| `cl-path` | Path to cl.exe |
| `link-path` | Path to link.exe |
| `rc-path` | Path to rc.exe |
| `include-path` | INCLUDE env value |
| `lib-path` | LIB env value |
