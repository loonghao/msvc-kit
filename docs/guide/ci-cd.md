# CI/CD Integration

msvc-kit is designed for CI/CD pipelines where installing Visual Studio is impractical.

## GitHub Actions

### Basic Setup

```yaml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install msvc-kit
        run: cargo install msvc-kit

      - name: Download MSVC
        run: msvc-kit download

      - name: Setup Environment
        run: msvc-kit setup --script --shell powershell | Invoke-Expression

      - name: Build
        run: cargo build --release
```

### With Caching

```yaml
name: Build with Cache

on: [push, pull_request]

jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - name: Cache msvc-kit
        uses: actions/cache@v4
        with:
          path: |
            ~\AppData\Local\loonghao\msvc-kit
          key: msvc-kit-${{ runner.os }}-v14.44

      - name: Install msvc-kit
        run: cargo install msvc-kit

      - name: Download MSVC (cached)
        run: msvc-kit download --msvc-version 14.44 --sdk-version 10.0.26100.0

      - name: Setup Environment
        run: msvc-kit setup --script --shell powershell | Invoke-Expression

      - name: Build
        run: cargo build --release
```

### Matrix Build

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

      - name: Install msvc-kit
        run: cargo install msvc-kit

      - name: Download MSVC for ${{ matrix.arch }}
        run: msvc-kit download --arch ${{ matrix.arch }}

      - name: Setup Environment
        run: msvc-kit setup --script --shell powershell | Invoke-Expression

      - name: Build
        run: cargo build --release --target ${{ matrix.arch }}-pc-windows-msvc
```

## Azure Pipelines

```yaml
trigger:
  - main

pool:
  vmImage: 'windows-latest'

steps:
  - task: RustInstaller@1
    inputs:
      rustVersion: 'stable'

  - script: cargo install msvc-kit
    displayName: 'Install msvc-kit'

  - script: msvc-kit download
    displayName: 'Download MSVC'

  - powershell: msvc-kit setup --script --shell powershell | Invoke-Expression
    displayName: 'Setup Environment'

  - script: cargo build --release
    displayName: 'Build'
```

## GitLab CI

```yaml
build:
  image: mcr.microsoft.com/windows/servercore:ltsc2022
  tags:
    - windows
  script:
    - choco install rust -y
    - cargo install msvc-kit
    - msvc-kit download
    - $env = msvc-kit env --format json | ConvertFrom-Json
    - $env.PSObject.Properties | ForEach-Object { Set-Item "env:$($_.Name)" $_.Value }
    - cargo build --release
```

## Docker

### Dockerfile

```dockerfile
FROM mcr.microsoft.com/windows/servercore:ltsc2022

# Install Rust
RUN powershell -Command \
    Invoke-WebRequest -Uri https://win.rustup.rs -OutFile rustup-init.exe; \
    ./rustup-init.exe -y --default-toolchain stable

# Install msvc-kit
RUN cargo install msvc-kit

# Download MSVC (cached in image layer)
RUN msvc-kit download

# Setup environment
SHELL ["powershell", "-Command"]
RUN msvc-kit setup --persistent

WORKDIR /app
```

## Tips

### Reduce Download Time

1. **Cache aggressively** - MSVC downloads are large but stable
2. **Pin versions** - Avoid re-downloading when versions change
3. **Use parallel downloads** - `--parallel-downloads 8`

### Reduce Image Size

1. **Clear cache after install** - `msvc-kit clean --cache`
2. **Download only what you need** - `--no-sdk` if SDK not needed

### Debugging

Enable verbose logging:

```yaml
- name: Download MSVC (verbose)
  run: |
    $env:RUST_LOG = "msvc_kit=debug"
    msvc-kit download
```
