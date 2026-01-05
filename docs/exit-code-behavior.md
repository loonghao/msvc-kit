# Exit Code Behavior

This document describes the exit code behavior of `msvc-kit` and explains why it matters for package manager compatibility.

## Overview

`msvc-kit` follows standard Unix/Windows conventions for exit codes:

- **Exit code 0**: Success or help information displayed
- **Exit code 1** (or non-zero): Error occurred

## winget Compatibility

The Windows Package Manager (winget) has specific requirements for package installers:

### Requirement: Zero Exit Code for Help

When winget validates a package installer, it runs the executable **without arguments** and expects:
1. Exit code **0** (success)
2. Help or usage information to be printed

This is documented in the [winget validation guide](https://github.com/microsoft/winget-pkgs/blob/master/AUTHORING_MANIFESTS.md).

### Implementation in msvc-kit

In `src/bin/msvc-kit.rs`, lines 228-235:

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

**Key points:**
- When no subcommand is provided, we print the help text
- We explicitly call `std::process::exit(0)` to ensure exit code 0
- This behavior is specifically designed for winget compatibility

## Exit Code Matrix

### Success Cases (Exit Code 0)

| Command | Behavior |
|---------|----------|
| (no args) | Print help and exit 0 |
| `--help` | Print help and exit 0 |
| `--version` | Print version and exit 0 |
| `<subcommand> --help` | Print subcommand help and exit 0 |
| `config` | Display current config and exit 0 |
| `config --reset` | Reset config and exit 0 |
| `list --dir <empty>` | Show "No installations found" and exit 0 |
| `clean --msvc-version <nonexistent>` | Exit 0 (idempotent operation) |

### Error Cases (Exit Code != 0)

| Command | Reason |
|---------|--------|
| `invalid-command` | Unknown subcommand |
| `bundle` (without `--accept-license`) | Missing required flag |
| `setup --dir <nonexistent>` | No MSVC installation found |
| `env --dir <nonexistent>` | No MSVC installation found |
| `download --arch invalid` | Invalid architecture |

## Testing

Exit code behavior is validated in `tests/cli_exit_code_tests.rs`:

```bash
# Run all exit code tests
cargo test --test cli_exit_code_tests

# Run specific test
cargo test test_no_subcommand_exits_zero
```

### Key Tests

1. **`test_no_subcommand_exits_zero`** - Critical for winget
2. **`test_help_flag_exits_zero`** - Standard behavior
3. **`test_version_flag_exits_zero`** - Standard behavior
4. **`test_bundle_without_license_exits_nonzero`** - Error handling
5. **`test_setup_without_installation_exits_nonzero`** - Error handling

## CI Validation

These tests run automatically in GitHub Actions to ensure:
- winget compatibility is maintained across changes
- Exit codes remain consistent
- Error handling works correctly

See `.github/workflows/ci.yml` for the CI configuration.

## Best Practices

When adding new commands or modifying existing ones:

1. **Always return appropriate exit codes**:
   - Use `Ok(())` for success (exits with 0)
   - Use `anyhow::bail!()` or `Err()` for errors (exits with 1)
   - Use `std::process::exit(0)` only when explicitly needed

2. **Add tests for new commands**:
   - Test success cases exit with 0
   - Test error cases exit with non-zero
   - Update `cli_exit_code_tests.rs` accordingly

3. **Document expected behavior**:
   - Add entries to the exit code matrix above
   - Note any special cases or requirements

## References

- [winget Manifest Authoring](https://github.com/microsoft/winget-pkgs/blob/master/AUTHORING_MANIFESTS.md)
- [Exit Status (Unix)](https://en.wikipedia.org/wiki/Exit_status)
- [Windows Exit Codes](https://docs.microsoft.com/en-us/windows/win32/debug/system-error-codes)
