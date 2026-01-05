# Exit Code Tests - Verification Checklist

## ✅ Pre-Commit Checklist

Before committing these changes, verify:

### 1. Tests Compile
```bash
cargo test --test cli_exit_code_tests --no-run
```
- [ ] Tests compile without errors
- [ ] No warnings from clippy

### 2. Tests Pass Locally
```bash
cargo test --test cli_exit_code_tests
```
- [ ] All 15+ tests pass
- [ ] No panics or unexpected failures
- [ ] Test output is clear and informative

### 3. Key Behaviors Verified Manually
```powershell
# Build the binary
cargo build --release

# Critical: No args exits with 0
./target/release/msvc-kit.exe
echo $LASTEXITCODE  # Should be 0

# Help flag exits with 0
./target/release/msvc-kit.exe --help
echo $LASTEXITCODE  # Should be 0

# Version flag exits with 0
./target/release/msvc-kit.exe --version
echo $LASTEXITCODE  # Should be 0

# Invalid command exits non-zero
./target/release/msvc-kit.exe invalid-command
echo $LASTEXITCODE  # Should be non-zero (usually 2)
```
- [ ] No args: exit code 0, help printed
- [ ] `--help`: exit code 0, help printed
- [ ] `--version`: exit code 0, version printed
- [ ] Invalid command: exit code non-zero

### 4. Documentation Complete
- [ ] `tests/cli_exit_code_tests.rs` - Well commented
- [ ] `docs/exit-code-behavior.md` - Technical reference
- [ ] `tests/README.md` - Test overview
- [ ] `TEST_EXIT_CODES.md` - Quick guide
- [ ] `EXIT_CODE_TESTS_SUMMARY.md` - Implementation details
- [ ] `CHANGES.md` - Change summary
- [ ] `COMMIT_MESSAGE.txt` - Commit message ready

### 5. Code Quality
```bash
# Format code
cargo fmt

# Check with clippy
cargo clippy --all-targets --all-features -- -D warnings
```
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings
- [ ] No unused imports
- [ ] Test names are descriptive

### 6. Integration
```bash
# Run all tests
cargo test --all-features
```
- [ ] All existing tests still pass
- [ ] No regressions introduced
- [ ] New tests integrate with existing suite

## 📋 Post-Commit Checklist

After committing:

### 1. PR Creation
- [ ] Create PR with clear title: "test: add exit code tests for winget compatibility"
- [ ] Use `COMMIT_MESSAGE.txt` as PR description
- [ ] Reference any related issues
- [ ] Add labels: `testing`, `winget`, `documentation`

### 2. CI Verification
- [ ] PR checks pass (pr-checks.yml)
- [ ] All tests pass in CI
- [ ] No platform-specific failures
- [ ] Build succeeds for all targets (x64, x86, arm64)

### 3. Review Preparation
- [ ] Tests are easy to understand
- [ ] Documentation is clear
- [ ] Examples are provided
- [ ] Common issues are documented

### 4. Optional: Local winget Test
If you have winget configured for local testing:
```powershell
# Create a local winget manifest
# Test with: winget install --manifest path\to\manifest
```
- [ ] winget accepts the installer
- [ ] No validation errors

## 🔍 Common Issues & Solutions

### Issue: Tests can't find binary
**Solution:** Build first: `cargo build --release`

### Issue: Exit code is None
**Solution:** Process terminated by signal; check for panics

### Issue: Tests pass locally but fail in CI
**Solution:** Check path handling in `get_binary_path()`

### Issue: Clippy warnings
**Solution:** Run `cargo clippy --fix --allow-dirty`

## 📝 Final Review

Before marking as ready for review:

- [ ] All items in Pre-Commit Checklist are ✅
- [ ] Tests demonstrate clear value
- [ ] Documentation is comprehensive
- [ ] Code is production-ready
- [ ] No TODOs or FIXMEs in code
- [ ] Examples work as shown

## 🎯 Success Criteria

This PR is successful if:

1. ✅ All exit code tests pass consistently
2. ✅ winget compatibility is verified
3. ✅ No regressions in existing tests
4. ✅ Documentation is clear and complete
5. ✅ CI pipeline passes
6. ✅ Code review feedback is positive

## 📚 References

- [winget Validation](https://github.com/microsoft/winget-pkgs/blob/master/AUTHORING_MANIFESTS.md)
- [Test Documentation](./TEST_EXIT_CODES.md)
- [Implementation Details](./EXIT_CODE_TESTS_SUMMARY.md)
- [Exit Code Behavior](./docs/exit-code-behavior.md)

---

**Note:** This checklist ensures the exit code tests are production-ready
and meet winget compatibility requirements.
