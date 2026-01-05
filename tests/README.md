# msvc-kit Tests

This directory contains various test suites for the msvc-kit project.

## Test Files

### CLI Exit Code Tests (`cli_exit_code_tests.rs`)

Tests that verify the CLI exits with correct status codes for winget compatibility:

- **Exit code 0** (success) cases:
  - No subcommand provided (prints help)
  - `--help` flag
  - `--version` flag
  - Subcommand help (e.g., `download --help`)
  - `config` command and `config --reset`
  - `list` with empty directory
  - `clean` with nonexistent versions (idempotent)

- **Exit code != 0** (error) cases:
  - Invalid subcommand
  - `bundle` without `--accept-license`
  - `setup` without prior installation
  - `env` without prior installation
  - Invalid architecture specified

**Why this matters**: winget (Windows Package Manager) validates that installers exit with code 0 when invoked without arguments or with `--help`. These tests ensure msvc-kit meets this requirement.

### Other Test Files

- `bundle_tests.rs` - Bundle creation and layout tests
- `config_tests.rs` - Configuration management tests
- `downloader_tests.rs` - Download functionality tests
- `e2e_tests.rs` - End-to-end workflow tests
- `env_tests.rs` - Environment setup tests
- `integration_test.rs` - Basic integration tests
- `reexports_tests.rs` - Module re-export tests
- `unit_tests.rs` - General unit tests
- `version_tests.rs` - Version detection and parsing tests

## Running Tests

### Run all tests
```bash
cargo test
```

### Run specific test file
```bash
cargo test --test cli_exit_code_tests
```

### Run specific test
```bash
cargo test test_no_subcommand_exits_zero
```

### Run with output
```bash
cargo test -- --nocapture
```

### Run ignored tests (e.g., network-dependent)
```bash
cargo test -- --ignored
```

## Test Dependencies

Tests use the following frameworks and utilities:

- `rstest` - Test parameterization and fixtures
- `tempfile` - Temporary directory creation
- `mockito` - HTTP mocking (for downloader tests)
- `tokio::test` - Async test runtime

## CI Integration

All tests are run automatically in GitHub Actions on:
- Push to main branch
- Pull requests
- Release creation

See `.github/workflows/` for CI configuration.
