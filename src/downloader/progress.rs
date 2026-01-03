//! Progress handling traits and implementations
//!
//! This module provides abstractions for progress reporting,
//! allowing external integrations (like vx) to implement custom UI.

use std::sync::Arc;

/// Progress handler trait for download operations
///
/// Implement this trait to provide custom progress UI.
/// The default implementation uses `indicatif` for terminal progress bars.
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::downloader::ProgressHandler;
///
/// struct MyProgressHandler;
///
/// impl ProgressHandler for MyProgressHandler {
///     fn on_start(&self, component: &str, total_files: usize, total_bytes: u64) {
///         println!("Starting {} download: {} files, {} bytes", component, total_files, total_bytes);
///     }
///
///     fn on_file_start(&self, file_name: &str, file_size: u64) {
///         println!("Downloading: {} ({} bytes)", file_name, file_size);
///     }
///
///     fn on_progress(&self, bytes: u64) {
///         // Update progress
///     }
///
///     fn on_file_complete(&self, file_name: &str, outcome: &str) {
///         println!("Completed: {} ({})", file_name, outcome);
///     }
///
///     fn on_complete(&self, downloaded: usize, skipped: usize) {
///         println!("Done: {} downloaded, {} skipped", downloaded, skipped);
///     }
///
///     fn on_error(&self, error: &str) {
///         eprintln!("Error: {}", error);
///     }
/// }
/// ```
pub trait ProgressHandler: Send + Sync {
    /// Called when download starts
    ///
    /// # Arguments
    /// * `component` - Component name (e.g., "MSVC", "Windows SDK")
    /// * `total_files` - Total number of files to download
    /// * `total_bytes` - Total size in bytes
    fn on_start(&self, component: &str, total_files: usize, total_bytes: u64);

    /// Called when a file download starts
    ///
    /// # Arguments
    /// * `file_name` - Name of the file being downloaded
    /// * `file_size` - Size of the file in bytes
    fn on_file_start(&self, file_name: &str, file_size: u64);

    /// Called to report download progress
    ///
    /// # Arguments
    /// * `bytes` - Number of bytes transferred (incremental)
    fn on_progress(&self, bytes: u64);

    /// Called when a file download completes
    ///
    /// # Arguments
    /// * `file_name` - Name of the completed file
    /// * `outcome` - Outcome description ("downloaded", "skipped", "cached")
    fn on_file_complete(&self, file_name: &str, outcome: &str);

    /// Called when all downloads complete
    ///
    /// # Arguments
    /// * `downloaded` - Number of files downloaded
    /// * `skipped` - Number of files skipped (cached)
    fn on_complete(&self, downloaded: usize, skipped: usize);

    /// Called when an error occurs
    ///
    /// # Arguments
    /// * `error` - Error description
    fn on_error(&self, error: &str);

    /// Called to update summary message
    ///
    /// # Arguments
    /// * `message` - Summary message
    fn on_message(&self, message: &str) {
        // Default: no-op
        let _ = message;
    }
}

/// Default progress handler using indicatif
pub struct IndicatifProgressHandler {
    progress_bar: indicatif::ProgressBar,
}

impl IndicatifProgressHandler {
    /// Create a new indicatif progress handler
    pub fn new(total_bytes: u64) -> Self {
        use indicatif::{ProgressBar, ProgressStyle};

        let pb = ProgressBar::new(total_bytes);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] {wide_bar:.cyan/blue} {bytes}/{total_bytes} @ {bytes_per_sec} ETA {eta} | {msg}")
                .unwrap()
                .progress_chars("##-"),
        );

        Self { progress_bar: pb }
    }

    /// Get the underlying progress bar
    pub fn progress_bar(&self) -> &indicatif::ProgressBar {
        &self.progress_bar
    }
}

impl ProgressHandler for IndicatifProgressHandler {
    fn on_start(&self, component: &str, total_files: usize, total_bytes: u64) {
        self.progress_bar.set_message(format!(
            "{}: {} files, total {}",
            component,
            total_files,
            humansize::format_size(total_bytes, humansize::BINARY)
        ));
    }

    fn on_file_start(&self, file_name: &str, _file_size: u64) {
        self.progress_bar.set_message(file_name.to_string());
    }

    fn on_progress(&self, bytes: u64) {
        self.progress_bar.inc(bytes);
    }

    fn on_file_complete(&self, _file_name: &str, _outcome: &str) {
        // Progress bar already updated via on_progress
    }

    fn on_complete(&self, downloaded: usize, skipped: usize) {
        self.progress_bar
            .finish_with_message(format!("Done: dl {} | skip {}", downloaded, skipped));
    }

    fn on_error(&self, error: &str) {
        self.progress_bar
            .abandon_with_message(format!("Error: {}", error));
    }

    fn on_message(&self, message: &str) {
        self.progress_bar.set_message(message.to_string());
    }
}

/// No-op progress handler for silent operation
pub struct NoopProgressHandler;

impl ProgressHandler for NoopProgressHandler {
    fn on_start(&self, _component: &str, _total_files: usize, _total_bytes: u64) {}
    fn on_file_start(&self, _file_name: &str, _file_size: u64) {}
    fn on_progress(&self, _bytes: u64) {}
    fn on_file_complete(&self, _file_name: &str, _outcome: &str) {}
    fn on_complete(&self, _downloaded: usize, _skipped: usize) {}
    fn on_error(&self, _error: &str) {}
}

/// Type alias for boxed progress handler
pub type BoxedProgressHandler = Arc<dyn ProgressHandler>;

/// Create a default progress handler
pub fn default_progress_handler(total_bytes: u64) -> BoxedProgressHandler {
    Arc::new(IndicatifProgressHandler::new(total_bytes))
}

/// Create a no-op progress handler
pub fn noop_progress_handler() -> BoxedProgressHandler {
    Arc::new(NoopProgressHandler)
}
