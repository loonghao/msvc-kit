//! Trait abstractions for component downloaders
//!
//! This module provides trait-based abstractions for MSVC and SDK downloaders,
//! enabling unified handling and easier integration with external tools like vx.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::error::Result;
use crate::installer::InstallInfo;

/// Component type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentType {
    /// MSVC compiler toolset
    Msvc,
    /// Windows SDK
    Sdk,
}

impl ComponentType {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            ComponentType::Msvc => "msvc",
            ComponentType::Sdk => "sdk",
        }
    }
}

impl std::fmt::Display for ComponentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Trait for component downloaders
///
/// This trait provides a unified interface for downloading different components
/// (MSVC, SDK), making it easier for external tools to handle them uniformly.
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::downloader::{ComponentDownloader, MsvcDownloader, SdkDownloader, DownloadOptions};
///
/// async fn download_component(downloader: &dyn ComponentDownloader) -> anyhow::Result<()> {
///     println!("Downloading {} component...", downloader.component_type());
///     let info = downloader.download().await?;
///     println!("Installed to: {:?}", info.install_path);
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait ComponentDownloader: Send + Sync {
    /// Download the component
    ///
    /// This method downloads and extracts the component to the configured target directory.
    async fn download(&self) -> Result<InstallInfo>;

    /// Get the component type
    fn component_type(&self) -> ComponentType;

    /// Get the component name for display
    fn component_name(&self) -> &'static str {
        match self.component_type() {
            ComponentType::Msvc => "MSVC",
            ComponentType::Sdk => "Windows SDK",
        }
    }
}

/// Cache manager trait for shared caching support
///
/// This trait allows vx and other external tools to share cache directories
/// and implement custom caching strategies.
///
/// # Example
///
/// ```rust,no_run
/// use msvc_kit::downloader::CacheManager;
/// use std::path::PathBuf;
///
/// struct SharedCache {
///     cache_dir: PathBuf,
/// }
///
/// impl CacheManager for SharedCache {
///     fn get(&self, key: &str) -> Option<Vec<u8>> {
///         let path = self.cache_dir.join(key);
///         std::fs::read(&path).ok()
///     }
///
///     fn set(&self, key: &str, value: &[u8]) -> msvc_kit::Result<()> {
///         let path = self.cache_dir.join(key);
///         if let Some(parent) = path.parent() {
///             std::fs::create_dir_all(parent)?;
///         }
///         std::fs::write(&path, value)?;
///         Ok(())
///     }
///
///     fn invalidate(&self, key: &str) -> msvc_kit::Result<()> {
///         let path = self.cache_dir.join(key);
///         if path.exists() {
///             std::fs::remove_file(&path)?;
///         }
///         Ok(())
///     }
///
///     fn clear(&self) -> msvc_kit::Result<()> {
///         if self.cache_dir.exists() {
///             std::fs::remove_dir_all(&self.cache_dir)?;
///         }
///         Ok(())
///     }
///
///     fn cache_dir(&self) -> &std::path::Path {
///         &self.cache_dir
///     }
/// }
/// ```
pub trait CacheManager: Send + Sync {
    /// Get cached data by key
    ///
    /// Returns `None` if the key doesn't exist or cache is invalid.
    fn get(&self, key: &str) -> Option<Vec<u8>>;

    /// Store data in cache
    ///
    /// The key should be a unique identifier (e.g., URL hash, file path).
    fn set(&self, key: &str, value: &[u8]) -> Result<()>;

    /// Invalidate a specific cache entry
    fn invalidate(&self, key: &str) -> Result<()>;

    /// Clear all cache entries
    fn clear(&self) -> Result<()>;

    /// Get the cache directory path
    fn cache_dir(&self) -> &Path;

    /// Check if a key exists in cache
    fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Get cache entry path for a key
    fn entry_path(&self, key: &str) -> PathBuf {
        self.cache_dir().join(key)
    }
}

/// File system based cache manager
///
/// Default implementation that stores cache entries as files on disk.
#[derive(Debug, Clone)]
pub struct FileSystemCacheManager {
    cache_dir: PathBuf,
}

impl FileSystemCacheManager {
    /// Create a new file system cache manager
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    /// Create with default cache directory
    pub fn default_cache_dir() -> Self {
        let cache_dir =
            if let Some(proj) = directories::ProjectDirs::from("com", "loonghao", "msvc-kit") {
                proj.cache_dir().to_path_buf()
            } else {
                std::env::temp_dir().join("msvc-kit").join("cache")
            };
        Self::new(cache_dir)
    }
}

impl CacheManager for FileSystemCacheManager {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        let path = self.cache_dir.join(key);
        std::fs::read(&path).ok()
    }

    fn set(&self, key: &str, value: &[u8]) -> Result<()> {
        let path = self.cache_dir.join(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, value)?;
        Ok(())
    }

    fn invalidate(&self, key: &str) -> Result<()> {
        let path = self.cache_dir.join(key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        if self.cache_dir.exists() {
            std::fs::remove_dir_all(&self.cache_dir)?;
            std::fs::create_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

/// Boxed cache manager type for dynamic dispatch
pub type BoxedCacheManager = Box<dyn CacheManager>;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_filesystem_cache_manager() {
        let temp_dir = TempDir::new().unwrap();
        let cache = FileSystemCacheManager::new(temp_dir.path());

        // Test set and get
        cache.set("test_key", b"test_value").unwrap();
        assert_eq!(cache.get("test_key"), Some(b"test_value".to_vec()));

        // Test contains
        assert!(cache.contains("test_key"));
        assert!(!cache.contains("nonexistent"));

        // Test invalidate
        cache.invalidate("test_key").unwrap();
        assert!(!cache.contains("test_key"));

        // Test nested keys
        cache.set("nested/path/key", b"nested_value").unwrap();
        assert_eq!(cache.get("nested/path/key"), Some(b"nested_value".to_vec()));

        // Test clear
        cache.set("key1", b"value1").unwrap();
        cache.set("key2", b"value2").unwrap();
        cache.clear().unwrap();
        assert!(!cache.contains("key1"));
        assert!(!cache.contains("key2"));
    }

    #[test]
    fn test_entry_path() {
        let temp_dir = TempDir::new().unwrap();
        let cache = FileSystemCacheManager::new(temp_dir.path());

        let path = cache.entry_path("some/key");
        assert_eq!(path, temp_dir.path().join("some/key"));
    }
}
