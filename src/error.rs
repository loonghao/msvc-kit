//! Error types for msvc-kit

use thiserror::Error;

/// Main error type for msvc-kit operations
#[derive(Error, Debug)]
pub enum MsvcKitError {
    /// Network-related errors during download
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Network error with download context
    #[error("Download failed for {file} ({url}): {source}")]
    DownloadNetwork {
        file: String,
        url: String,
        #[source]
        source: reqwest::Error,
    },

    /// IO errors during file operations
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing errors
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    /// SIMD JSON parsing errors
    #[error("JSON parsing error: {0}")]
    SimdJson(#[from] simd_json::Error),

    /// TOML deserialization errors
    #[error("TOML parsing error: {0}")]
    TomlDe(#[from] toml::de::Error),

    /// TOML serialization errors
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    /// Database errors
    #[error("Database error: {0}")]
    Database(String),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// ZIP extraction errors
    #[error("ZIP extraction error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// CAB extraction errors
    #[error("CAB extraction error: {0}")]
    Cab(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Version not found
    #[error("Version not found: {0}")]
    VersionNotFound(String),

    /// Component not found
    #[error("Component not found: {0}")]
    ComponentNotFound(String),

    /// Installation path error
    #[error("Installation path error: {0}")]
    InstallPath(String),

    /// Environment setup error
    #[error("Environment setup error: {0}")]
    EnvSetup(String),

    /// Hash verification failed
    #[error("Hash verification failed for {file}: expected {expected}, got {actual}")]
    HashMismatch {
        file: String,
        expected: String,
        actual: String,
    },

    /// Platform not supported
    #[error("Platform not supported: {0}")]
    UnsupportedPlatform(String),

    /// Download cancelled
    #[error("Download cancelled by user")]
    Cancelled,

    /// Generic error with message
    #[error("{0}")]
    Other(String),
}

/// Result type alias for msvc-kit operations
pub type Result<T> = std::result::Result<T, MsvcKitError>;

impl From<String> for MsvcKitError {
    fn from(s: String) -> Self {
        MsvcKitError::Other(s)
    }
}

impl From<&str> for MsvcKitError {
    fn from(s: &str) -> Self {
        MsvcKitError::Other(s.to_string())
    }
}
