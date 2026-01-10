//! Core unit tests for msvc-kit
//!
//! Note: Most tests have been split into separate files:
//! - version_tests.rs - Architecture and version tests
//! - downloader_tests.rs - Download options, preview, cache tests
//! - env_tests.rs - Environment and shell script tests
//! - config_tests.rs - Config and error tests
//! - bundle_tests.rs - Bundle layout and options tests
//! - reexports_tests.rs - Library re-export tests

// This file is kept for backwards compatibility and contains any tests
// that don't fit cleanly into the other categories.

use msvc_kit::constants::{download, extraction, hash, progress};

// ============================================================================
// Constants Tests - Verify optimized buffer sizes
// ============================================================================

#[test]
fn test_hash_buffer_size_optimized() {
    // Hash buffer should be 4 MB for better throughput
    assert_eq!(hash::HASH_BUFFER_SIZE, 4 * 1024 * 1024);
}

#[test]
fn test_extract_buffer_size_optimized() {
    // Extract buffer should be 256 KB for better throughput
    assert_eq!(extraction::EXTRACT_BUFFER_SIZE, 256 * 1024);
}

#[test]
fn test_default_parallel_extractions() {
    // Default parallel extractions should be 4
    assert_eq!(extraction::DEFAULT_PARALLEL_EXTRACTIONS, 4);
}

#[test]
fn test_download_constants() {
    // Verify download constants are reasonable
    assert!(download::MAX_RETRIES >= 1);
    assert!(download::DEFAULT_PARALLEL_DOWNLOADS >= 1);
    assert!(download::MIN_CONCURRENCY >= 1);
    assert!(download::LOW_THROUGHPUT_MBPS > 0.0);
    assert!(download::HIGH_THROUGHPUT_MBPS > download::LOW_THROUGHPUT_MBPS);
}

#[test]
fn test_progress_constants() {
    // Verify progress constants are reasonable
    assert!(progress::SPINNER_TICK_MS > 0);
    assert!(progress::PROGRESS_TICK_MS > 0);
    assert!(progress::UPDATE_INTERVAL.as_millis() > 0);
}
