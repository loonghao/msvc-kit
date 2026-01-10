//! Constants and configuration values for msvc-kit
//!
//! This module centralizes all magic numbers and hardcoded values
//! to improve maintainability and configurability.

/// User agent string for HTTP requests
pub const USER_AGENT: &str = concat!("msvc-kit/", env!("CARGO_PKG_VERSION"));

/// Visual Studio 2022 channel manifest URL
pub const VS_CHANNEL_URL: &str = "https://aka.ms/vs/17/release/channel";

/// Download configuration
pub mod download {
    /// Maximum number of retry attempts for failed downloads
    pub const MAX_RETRIES: usize = 4;

    /// Default number of parallel downloads
    pub const DEFAULT_PARALLEL_DOWNLOADS: usize = 4;

    /// Low throughput threshold in MB/s for adaptive concurrency
    pub const LOW_THROUGHPUT_MBPS: f64 = 2.0;

    /// High throughput threshold in MB/s for adaptive concurrency
    pub const HIGH_THROUGHPUT_MBPS: f64 = 10.0;

    /// Number of consecutive low-throughput batches before reducing concurrency
    pub const LOW_THROUGHPUT_STREAK_THRESHOLD: usize = 3;

    /// Minimum concurrency level
    pub const MIN_CONCURRENCY: usize = 2;
}

/// Progress display configuration
pub mod progress {
    use std::time::Duration;

    /// Spinner tick interval
    pub const SPINNER_TICK_MS: u64 = 80;

    /// Progress bar tick interval
    pub const PROGRESS_TICK_MS: u64 = 120;

    /// Progress update interval for downloads
    pub const UPDATE_INTERVAL: Duration = Duration::from_millis(200);
}

/// Hash computation configuration
pub mod hash {
    /// Buffer size for file hash computation (4 MB for better throughput)
    pub const HASH_BUFFER_SIZE: usize = 4 * 1024 * 1024;
}

/// Extraction configuration
pub mod extraction {
    /// Buffer size for file extraction (256 KB for better throughput)
    pub const EXTRACT_BUFFER_SIZE: usize = 256 * 1024;

    /// Default number of parallel extractions (based on CPU cores)
    pub const DEFAULT_PARALLEL_EXTRACTIONS: usize = 4;
}
