# Exit Code Testing Guide

## Quick Start

```bash
# Run exit code tests
cargo test --test cli_exit_code_tests

# Run with detailed output
cargo test --test cli_exit_code_tests -- --nocapture

# Run specific test
cargo test test_no_subcommand_exits_zero -- --nocapture

# Use the helper script (PowerShell)
./run_exit_code_tests.ps1
```

## What Was Added

### 1. Comprehensive Exit Code Tests
**File:** `tests/cli_exit_code_tests.rs`

Tests all CLI exit code scenarios to ensure:
- ✅ winget compatibility (exit 0 without arguments)
- ✅ Standard help/version behavior
- ✅ Proper error codes for failures
- ✅ Idempotent operations (e.g., clean)

### 2. Documentation
- `docs/exit-code-behavior.md` - Technical reference
- `tests/README.md` - Test suite overview
- `EXIT_CODE_TESTS_SUMMARY.md` - Implementation details

### 3. Helper Scripts
- `run_exit_code_tests.ps1` - Convenient test runner

## Test Categories

### ✅ Success Cases (Exit Code 0)

| Test | Command | Purpose |
|------|---------|---------|
| `test_no_subcommand_exits_zero` | *(no args)* | **Critical for winget** |
| `test_help_flag_exits_zero` | `--help` | Standard behavior |
| `test_version_flag_exits_zero` | `--version` | Standard behavior |
| `test_subcommand_help_exits_zero` | `<cmd> --help` | All subcommands |
| `test_config_command_exits_zero` | `config`, `config --reset` | Configuration |
| `test_list_empty_dir_exits_zero` | `list --dir <empty>` | No installations |
| `test_clean_nonexistent_version_exits_zero` | `clean --msvc-version 99.99` | Idempotent |

### ❌ Error Cases (Exit Code ≠ 0)

| Test | Command | Purpose |
|------|---------|---------|
| `test_invalid_subcommand_exits_nonzero` | `invalid-command` | Unknown command |
| `test_bundle_without_license_exits_nonzero` | `bundle` (no `--accept-license`) | Missing required flag |
| `test_setup_without_installation_exits_nonzero` | `setup --dir <empty>` | No installation |
| `test_env_command_without_installation_exits_nonzero` | `env --dir <empty>` | No installation |
| `test_invalid_architecture_exits_nonzero` | `download --arch invalid` | Invalid input |

## Why This Matters

### winget Package Manager Requirement

Windows Package Manager (winget) validates installers by:
1. Running the executable **without arguments**
2. Expecting **exit code 0**
3. Expecting help/usage text to be printed

**Without this behavior, msvc-kit cannot be published to winget.**

### Implementation Reference

The critical code in `src/bin/msvc-kit.rs`:

```rust
// Handle the case where no subcommand is provided (for winget compatibility)
let command = match cli.command {
    Some(cmd) => cmd,
    None => {
        // Print help and exit with code 0 for winget validation
        Cli::command().print_help().unwrap();
        std::process::exit(0);
    }
};
```

## Verifying Tests Work

### Manual Verification

Test the most critical case manually:

```powershell
# Build the binary
cargo build --release

# Run without arguments - should print help and exit 0
./target/release/msvc-kit.exe
echo $LASTEXITCODE  # Should output: 0

# Test with help flag
./target/release/msvc-kit.exe --help
echo $LASTEXITCODE  # Should output: 0

# Test invalid command
./target/release/msvc-kit.exe invalid
echo $LASTEXITCODE  # Should output: 2 (or non-zero)
```

### Automated Verification

```bash
# Run all tests
cargo test

# Run only exit code tests
cargo test --test cli_exit_code_tests

# Run with verbose output
cargo test --test cli_exit_code_tests -- --nocapture --test-threads=1
```

## CI Integration

Exit code tests run automatically in:

- **PR Checks** (`pr-checks.yml`)
  - Runs on all pull requests
  - Must pass before merge

- **CI Pipeline** (`ci.yml`)
  - Runs on manual trigger
  - Part of `cargo test --all-features`

- **Release** (`release.yml`)
  - Runs before creating releases
  - Ensures quality

## Common Issues

### Issue: Test can't find binary

**Problem:** Tests fail with "Failed to run msvc-kit"

**Solution:**
```bash
# Build the binary first
cargo build --release

# Then run tests
cargo test --test cli_exit_code_tests
```

### Issue: Tests pass locally but fail in CI

**Problem:** Path differences between local and CI

**Solution:**
- The test helper `get_binary_path()` handles this automatically
- Ensure you're using `cargo test` (not running the binary directly)

### Issue: Exit code is None instead of 0

**Problem:** Process was terminated by signal, not normal exit

**Solution:**
- Check for panics in the code
- Ensure `std::process::exit(0)` is called explicitly where needed

## Adding New Tests

When adding new commands:

1. **Add success test** if the command should succeed:
   ```rust
   #[test]
   fn test_new_command_exits_zero() {
       let output = run_command(&["new-command"]).unwrap();
       assert!(output.status.success());
   }
   ```

2. **Add error test** if the command requires arguments:
   ```rust
   #[test]
   fn test_new_command_without_args_exits_nonzero() {
       let output = run_command(&["new-command"]).unwrap();
       assert!(!output.status.success());
   }
   ```

3. **Update documentation:**
   - Add to exit code matrix in `docs/exit-code-behavior.md`
   - Update this guide's test tables

## References

- [winget Manifest Authoring Guide](https://github.com/microsoft/winget-pkgs/blob/master/AUTHORING_MANIFESTS.md)
- [Exit Status Standards](https://tldp.org/LDP/abs/html/exitcodes.html)
- [rstest Documentation](https://docs.rs/rstest/latest/rstest/)

## Questions?

Check:
1. `docs/exit-code-behavior.md` - Technical details
2. `tests/README.md` - Test overview
3. `EXIT_CODE_TESTS_SUMMARY.md` - Implementation notes
