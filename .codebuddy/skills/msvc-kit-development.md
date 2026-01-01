# msvc-kit Development Skill

## Description
This skill provides guidance for developing and maintaining the msvc-kit project - a portable MSVC Build Tools installer and manager for Rust development.

## Project Structure
```
msvc-kit/
├── src/
│   ├── lib.rs              # Library entry point
│   ├── bin/msvc-kit.rs     # CLI entry point
│   ├── config/mod.rs       # Configuration management
│   ├── downloader/         # Download functionality
│   │   ├── mod.rs
│   │   ├── manifest.rs     # VS manifest parsing
│   │   ├── msvc.rs         # MSVC downloader
│   │   └── sdk.rs          # SDK downloader
│   ├── env/                # Environment setup
│   │   ├── mod.rs
│   │   └── setup.rs
│   ├── installer/          # Installation/extraction
│   │   ├── mod.rs
│   │   └── extractor.rs
│   ├── version/mod.rs      # Version management
│   └── error.rs            # Error types
├── tests/                  # Integration tests
├── .github/workflows/      # CI/CD
└── pkg/winget/             # Winget manifest
```

## Key Commands

### Build and Test
```bash
cargo build           # Build debug
cargo build --release # Build release
cargo test            # Run tests
cargo clippy          # Lint
cargo fmt             # Format
```

### CLI Usage
```bash
msvc-kit download                    # Download latest MSVC + SDK
msvc-kit download --msvc-version 14.40
msvc-kit setup --script --shell powershell
msvc-kit list
msvc-kit clean --all
```

## Development Guidelines

### Adding New Features
1. Update relevant module in `src/`
2. Add public API to `src/lib.rs` if needed
3. Update CLI in `src/bin/msvc-kit.rs`
4. Add tests in `tests/`
5. Update README.md

### Environment Variables (cc-rs compatible)
- `VCINSTALLDIR` - VC installation directory
- `VCToolsInstallDir` - VC tools directory
- `VCToolsVersion` - VC tools version
- `WindowsSdkDir` - Windows SDK directory
- `WindowsSDKVersion` - Windows SDK version
- `INCLUDE` - Include paths
- `LIB` - Library paths
- `PATH` - Binary paths

### Error Handling
Use `MsvcKitError` enum for all errors:
```rust
use crate::error::{MsvcKitError, Result};
```

### Async Pattern
All download/install operations are async:
```rust
pub async fn download_msvc(options: &DownloadOptions) -> Result<InstallInfo>
```

## Release Process
1. Update version in `Cargo.toml`
2. Update CHANGELOG
3. Create git tag: `git tag v0.1.0`
4. Push: `git push origin main --tags`
5. CI will build and create GitHub release
6. Update winget manifest with SHA256 hashes
