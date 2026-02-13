# msvc-kit Development Commands
# Usage: vx just <recipe>

# Default recipe - show available commands
default:
    @just --list

# ============== Build Commands ==============

# Build debug version
build:
    vx cargo build

# Build release version
build-release:
    vx cargo build --release

# Build release version (locked)
build-release-locked:
    vx cargo build --release --locked

# Build release for specific target
build-target-release TARGET:
    vx cargo build --release --target {{TARGET}}

# Build for all Windows targets
build-all:
    vx cargo build --release --target x86_64-pc-windows-msvc
    vx cargo build --release --target i686-pc-windows-msvc
    vx cargo build --release --target aarch64-pc-windows-msvc

# ============== Quality Commands ==============

# Run all checks (format, lint, test)
check: fmt-check lint test

# Check code formatting
fmt-check:
    vx cargo fmt --all -- --check

# Format code
fmt:
    vx cargo fmt --all

# Run clippy linter
lint:
    vx cargo clippy --all-targets --all-features -- -D warnings

# Run clippy with auto-fix
lint-fix:
    vx cargo clippy --all-targets --all-features --fix --allow-dirty

# Workspace check
check-workspace:
    vx cargo check --workspace --all-features

# ============== Test Commands ==============

# Run all tests
test:
    vx cargo test --verbose

# Run tests with all features
test-all-features:
    vx cargo test --all-features --verbose

# Run doc tests
test-doc:
    vx cargo test --doc

# Run tests with output
test-nocapture:
    vx cargo test -- --nocapture

# Run specific test
test-one NAME:
    vx cargo test {{NAME}} -- --nocapture

# ============== Documentation ==============

# Generate documentation
doc:
    vx cargo doc --no-deps --open

# Build docs without opening
doc-build:
    vx cargo doc --no-deps

# Build docs for CI (private items)
doc-ci:
    vx cargo doc --no-deps --document-private-items

# Start VitePress dev server
docs-dev:
    cd docs && vx npm run dev

# Build VitePress documentation
docs-build:
    cd docs && vx npm run build

# Preview VitePress documentation
docs-preview:
    cd docs && vx npm run preview

# Install docs dependencies
docs-install:
    cd docs && vx npm ci


# ============== Development ==============

# Run the CLI with arguments
run *ARGS:
    vx cargo run -- {{ARGS}}

# Run release version with arguments
run-release *ARGS:
    vx cargo run --release -- {{ARGS}}

# Watch for changes and rebuild
watch:
    vx cargo watch -x build

# Watch for changes and run tests
watch-test:
    vx cargo watch -x test

# ============== Release ==============

# Prepare release (run all checks)
release-check: fmt-check lint test
    vx cargo build --release
    @echo "✅ All checks passed! Ready for release."

# Publish to crates.io (dry run)
publish-dry:
    vx cargo publish --dry-run

# Publish to crates.io
publish:
    vx cargo publish

# Publish to crates.io (CI)
publish-ci:
    vx cargo publish --allow-dirty

# ============== Cleanup ==============

# Clean build artifacts
clean:
    vx cargo clean

# Clean and rebuild
rebuild: clean build

# ============== Utilities ==============

# Show project info
info:
    @echo "Project: msvc-kit"
    @echo "Version: $(vx cargo pkgid | cut -d# -f2)"
    @vx cargo --version
    @vx rustc --version

# Update dependencies
update:
    vx cargo update

# Check for outdated dependencies
outdated:
    vx cargo outdated

# Security audit
audit:
    vx cargo generate-lockfile || true
    vx cargo audit --deny warnings || true

# Generate dependency tree
tree:
    vx cargo tree

# Coverage report
coverage:
    vx cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

# ============== CI Simulation ==============

# Run full CI pipeline locally
ci: fmt-check lint test build-release
    @echo "✅ CI pipeline completed successfully!"

