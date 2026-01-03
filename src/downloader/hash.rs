//! Hash computation utilities for file verification
//!
//! Provides streaming SHA256 hash computation for downloaded files.

use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::{fs::File, io::AsyncReadExt};

use crate::constants::hash as hash_const;
use crate::error::Result;

/// Compute SHA256 hash of a file using streaming (memory-efficient)
///
/// This function reads the file in chunks to avoid loading the entire file
/// into memory, making it suitable for large files.
///
/// # Arguments
///
/// * `path` - Path to the file to hash
///
/// # Returns
///
/// The lowercase hex-encoded SHA256 hash string
///
/// # Example
///
/// ```rust,no_run
/// use std::path::Path;
/// use msvc_kit::downloader::hash::compute_file_hash;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let hash = compute_file_hash(Path::new("file.zip")).await?;
///     println!("SHA256: {}", hash);
///     Ok(())
/// }
/// ```
pub async fn compute_file_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();

    let mut buf = vec![0u8; hash_const::HASH_BUFFER_SIZE];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let result = hasher.finalize();
    Ok(hex::encode(result))
}

/// Compute SHA256 hash of a byte slice
///
/// Useful for hashing in-memory data like manifest content.
///
/// # Arguments
///
/// * `data` - Byte slice to hash
///
/// # Returns
///
/// The lowercase hex-encoded SHA256 hash string
pub fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Compare two hash strings (case-insensitive)
///
/// # Arguments
///
/// * `hash1` - First hash string
/// * `hash2` - Second hash string
///
/// # Returns
///
/// `true` if the hashes match (case-insensitive), `false` otherwise
pub fn hashes_match(hash1: &str, hash2: &str) -> bool {
    hash1.eq_ignore_ascii_case(hash2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash() {
        let data = b"hello world";
        let hash = compute_hash(data);
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_hashes_match() {
        assert!(hashes_match("ABC123", "abc123"));
        assert!(hashes_match("abc123", "ABC123"));
        assert!(!hashes_match("abc123", "abc124"));
    }
}
