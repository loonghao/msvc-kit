#!/usr/bin/env pwsh
# Run exit code tests

$ErrorActionPreference = "Continue"

Write-Host "Building msvc-kit binary..." -ForegroundColor Cyan
cargo build --release

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "`nRunning CLI exit code tests..." -ForegroundColor Cyan
cargo test --test cli_exit_code_tests -- --nocapture

if ($LASTEXITCODE -eq 0) {
    Write-Host "`n✅ All exit code tests passed!" -ForegroundColor Green
} else {
    Write-Host "`n❌ Some tests failed!" -ForegroundColor Red
}

exit $LASTEXITCODE
