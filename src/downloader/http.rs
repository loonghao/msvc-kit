//! HTTP client configuration and request utilities
//!
//! Provides configurable HTTP client creation and common request patterns.

use std::time::Duration;

use reqwest::Client;

use crate::constants::USER_AGENT;

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
    let mut builder = Client::builder().user_agent(&config.user_agent);

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
        let request = client
            .get("http://example.com")
            .build()
            .expect("request build");

        let user_agent = request
            .headers()
            .get(reqwest::header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .unwrap();

        assert_eq!(user_agent, "msvc-kit/test");
        
        // Test that config values were applied by verifying the config itself
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(5)));
        assert_eq!(config.timeout, Some(Duration::from_secs(15)));
    }
}

