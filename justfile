# msvc-kit Development Commands
# Usage: vx just <recipe>

# Default recipe - show available commands
default:
    @just --list

# ============== Build Commands ==============

# Build debug version
build:
    cargo build

# Build release version
build-release:
    cargo build --release

# Build for all Windows targets
build-all:
    cargo build --release --target x86_64-pc-windows-msvc
    cargo build --release --target i686-pc-windows-msvc
    cargo build --release --target aarch64-pc-windows-msvc

# ============== Quality Commands ==============

# Run all checks (format, lint, test)
check: fmt-check lint test

# Check code formatting
fmt-check:
    cargo fmt --all -- --check

# Format code
fmt:
    cargo fmt --all

# Run clippy linter
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run clippy with auto-fix
lint-fix:
    cargo clippy --all-targets --all-features --fix --allow-dirty

# ============== Test Commands ==============

# Run all tests
test:
    cargo test --verbose

# Run tests with output
test-nocapture:
    cargo test -- --nocapture

# Run specific test
test-one NAME:
    cargo test {{NAME}} -- --nocapture

# ============== Documentation ==============

# Generate documentation
doc:
    cargo doc --no-deps --open

# Build docs without opening
doc-build:
    cargo doc --no-deps

# Start VitePress dev server
docs-dev:
    cd docs && npm run dev

# Build VitePress documentation
docs-build:
    cd docs && npm run build

# Preview VitePress documentation
docs-preview:
    cd docs && npm run preview

# Install docs dependencies
docs-install:
    cd docs && npm install

# ============== Development ==============

# Run the CLI with arguments
run *ARGS:
    cargo run -- {{ARGS}}

# Run release version with arguments
run-release *ARGS:
    cargo run --release -- {{ARGS}}

# Watch for changes and rebuild
watch:
    cargo watch -x build

# Watch for changes and run tests
watch-test:
    cargo watch -x test

# ============== Release ==============

# Prepare release (run all checks)
release-check: fmt-check lint test
    cargo build --release
    @echo "✅ All checks passed! Ready for release."

# Publish to crates.io (dry run)
publish-dry:
    cargo publish --dry-run

# Publish to crates.io
publish:
    cargo publish

# ============== Cleanup ==============

# Clean build artifacts
clean:
    cargo clean

# Clean and rebuild
rebuild: clean build

# ============== Utilities ==============

# Show project info
info:
    @echo "Project: msvc-kit"
    @echo "Version: $(cargo pkgid | cut -d# -f2)"
    @cargo --version
    @rustc --version

# Update dependencies
update:
    cargo update

# Check for outdated dependencies
outdated:
    cargo outdated

# Security audit
audit:
    cargo audit

# Generate dependency tree
tree:
    cargo tree

# ============== CI Simulation ==============

# Run full CI pipeline locally
ci: fmt-check lint test build-release
    @echo "✅ CI pipeline completed successfully!"
