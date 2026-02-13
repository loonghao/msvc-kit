//! HTTP client configuration and request utilities
//!
//! Provides configurable HTTP client creation and common request patterns.
//!
//! ## TLS Backend
//!
//! The TLS backend is selected at compile time via Cargo feature flags:
//! - `native-tls` (default): Uses the platform's native TLS implementation
//!   (SChannel on Windows, OpenSSL on Linux, Secure Transport on macOS).
//!   This avoids the `cmake`/`NASM` build dependency required by `rustls`/`aws-lc-sys`.
//! - `rustls-tls`: Uses `rustls` with `aws-lc-rs` crypto backend.
//!   Requires `cmake` and `NASM` to be installed on Windows.
//!
//! See: <https://github.com/loonghao/msvc-kit/issues/44>

use std::time::Duration;

use reqwest::Client;

use crate::constants::USER_AGENT;

// Compile-time check: at least one TLS backend must be enabled.
#[cfg(not(any(feature = "native-tls", feature = "rustls-tls")))]
compile_error!(
    "At least one TLS backend feature must be enabled: `native-tls` (recommended) or `rustls-tls`. \
     Add `native-tls` to your feature list to use the platform's native TLS, \
     which avoids requiring cmake/NASM on Windows."
);

/// Returns the name of the currently active TLS backend.
///
/// This is determined at compile time based on feature flags.
/// When both backends are enabled, `native-tls` takes precedence.
pub fn tls_backend_name() -> &'static str {
    #[cfg(feature = "native-tls")]
    {
        "native-tls"
    }
    #[cfg(all(feature = "rustls-tls", not(feature = "native-tls")))]
    {
        "rustls-tls"
    }
    // Fallback for the impossible case (compile_error above prevents this)
    #[cfg(not(any(feature = "native-tls", feature = "rustls-tls")))]
    {
        "none"
    }
}

/// HTTP client configuration options

#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// User agent string
    pub user_agent: String,
    /// Connection timeout
    pub connect_timeout: Option<Duration>,
    /// Request timeout
    pub timeout: Option<Duration>,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            user_agent: USER_AGENT.to_string(),
            connect_timeout: Some(Duration::from_secs(30)),
            timeout: Some(Duration::from_secs(300)),
        }
    }
}

impl HttpClientConfig {
    /// Create a new configuration with custom user agent
    pub fn with_user_agent(user_agent: impl Into<String>) -> Self {
        Self {
            user_agent: user_agent.into(),
            ..Default::default()
        }
    }

    /// Set connection timeout
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Set request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Build the HTTP client with these settings
    pub fn build(&self) -> Client {
        create_http_client_with_config(self)
    }
}

/// Create a configured HTTP client with default settings
///
/// Uses the default user agent and timeout values from constants.
///
/// # Returns
///
/// A configured `reqwest::Client` instance
///
/// # Panics
///
/// Panics if the client cannot be created (e.g., TLS initialization failure)
pub fn create_http_client() -> Client {
    create_http_client_with_config(&HttpClientConfig::default())
}

/// Create a configured HTTP client with custom settings
///
/// Uses the selected TLS backend (`native-tls` by default) to avoid
/// requiring cmake/NASM on Windows. See module-level docs for details.
///
/// # Arguments
///
/// * `config` - HTTP client configuration
///
/// # Returns
///
/// A configured `reqwest::Client` instance
///
/// # Panics
///
/// Panics if the client cannot be created
pub fn create_http_client_with_config(config: &HttpClientConfig) -> Client {
    let mut builder = Client::builder()
        .user_agent(&config.user_agent)
        // Enable connection pooling for better performance
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(std::time::Duration::from_secs(90));

    // Explicitly configure TLS backend based on feature flags.
    // native-tls uses SChannel on Windows, avoiding cmake/NASM requirement.
    // See: https://github.com/loonghao/msvc-kit/issues/44
    #[cfg(feature = "native-tls")]
    {
        builder = builder.use_native_tls();
    }
    #[cfg(all(feature = "rustls-tls", not(feature = "native-tls")))]
    {
        builder = builder.use_rustls_tls();
    }

    if let Some(timeout) = config.connect_timeout {
        builder = builder.connect_timeout(timeout);
    }
    if let Some(timeout) = config.timeout {
        builder = builder.timeout(timeout);
    }

    builder.build().expect("Failed to create HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HttpClientConfig::default();
        assert!(config.user_agent.contains("msvc-kit"));
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(30)));
        assert_eq!(config.timeout, Some(Duration::from_secs(300)));
    }

    #[test]
    fn test_custom_config() {
        let config = HttpClientConfig::with_user_agent("custom-agent/1.0")
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60));

        assert_eq!(config.user_agent, "custom-agent/1.0");
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(10)));
        assert_eq!(config.timeout, Some(Duration::from_secs(60)));
    }

    #[test]
    fn test_create_client() {
        let client = create_http_client();
        // Just verify it doesn't panic
        drop(client);
    }

    #[test]
    fn test_build_applies_config() {
        let config = HttpClientConfig::with_user_agent("msvc-kit/test")
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15));

        let client = config.build();

        // Test that the client was built successfully
        // We can't easily test the internal configuration of reqwest::Client
        // so we just verify the config values themselves
        assert_eq!(config.user_agent, "msvc-kit/test");
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(5)));
        assert_eq!(config.timeout, Some(Duration::from_secs(15)));

        // Verify client can create requests without panicking
        let _request = client
            .get("http://example.com")
            .build()
            .expect("request build should succeed");
    }

    #[test]
    fn test_tls_backend_name() {
        let backend = tls_backend_name();
        // With default features, native-tls should be active
        #[cfg(feature = "native-tls")]
        assert_eq!(backend, "native-tls");
        #[cfg(all(feature = "rustls-tls", not(feature = "native-tls")))]
        assert_eq!(backend, "rustls-tls");
        // Backend name should never be empty
        assert!(!backend.is_empty());
    }

    #[test]
    fn test_tls_backend_is_not_none() {
        // Ensure a valid TLS backend is configured
        let backend = tls_backend_name();
        assert_ne!(backend, "none", "A TLS backend must be enabled");
    }

    #[test]
    fn test_create_client_with_tls() {
        // Verify that client creation succeeds with the configured TLS backend.
        // This confirms that native-tls (or rustls) initializes correctly.
        let client = create_http_client();
        let _request = client
            .get("https://example.com")
            .build()
            .expect("HTTPS request build should succeed with TLS backend");
    }

    #[test]
    fn test_client_builder_with_tls_config() {
        // Verify that HttpClientConfig.build() produces a working HTTPS client
        let config = HttpClientConfig::default();
        let client = config.build();
        let _request = client
            .get("https://example.com")
            .build()
            .expect("HTTPS request build should succeed");
    }
}
