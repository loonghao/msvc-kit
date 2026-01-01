//! Visual Studio manifest parsing
//!
//! This module handles parsing the Visual Studio channel manifest to extract
//! download URLs for MSVC and Windows SDK components.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{MsvcKitError, Result};

/// Visual Studio channel manifest URL
pub const VS_MANIFEST_URL: &str = 
    "https://aka.ms/vs/17/release/channel";

/// Visual Studio manifest structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VsManifest {
    /// Manifest version
    pub manifest_version: String,

    /// Channel items (packages)
    #[serde(default)]
    pub channel_items: Vec<ChannelItem>,
}

/// Channel item in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelItem {
    /// Item ID
    pub id: String,

    /// Item version
    pub version: Option<String>,

    /// Item type
    #[serde(rename = "type")]
    pub item_type: String,

    /// Payloads for this item
    #[serde(default)]
    pub payloads: Vec<Payload>,

    /// Dependencies
    #[serde(default)]
    pub dependencies: HashMap<String, DependencyInfo>,

    /// Localized resources
    #[serde(default)]
    pub localized_resources: Vec<LocalizedResource>,
}

/// Payload information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    /// File name
    pub file_name: String,

    /// SHA256 hash
    pub sha256: Option<String>,

    /// File size
    pub size: Option<u64>,

    /// Download URL
    pub url: String,
}

/// Dependency information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyInfo {
    /// Dependency version
    pub version: Option<String>,

    /// Dependency type
    #[serde(rename = "type")]
    pub dep_type: Option<String>,
}

/// Localized resource
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedResource {
    /// Language
    pub language: String,

    /// Title
    pub title: Option<String>,

    /// Description
    pub description: Option<String>,
}

/// Package information extracted from manifest
#[derive(Debug, Clone)]
pub struct Package {
    /// Package ID
    pub id: String,

    /// Package version
    pub version: String,

    /// Package type
    pub package_type: String,

    /// Payloads to download
    pub payloads: Vec<PackagePayload>,

    /// Total size in bytes
    pub total_size: u64,
}

/// Package payload for download
#[derive(Debug, Clone)]
pub struct PackagePayload {
    /// File name
    pub file_name: String,

    /// Download URL
    pub url: String,

    /// File size
    pub size: u64,

    /// SHA256 hash for verification
    pub sha256: Option<String>,
}

impl VsManifest {
    /// Fetch the Visual Studio manifest from Microsoft servers
    pub async fn fetch() -> Result<Self> {
        let client = reqwest::Client::new();
        
        // First, get the channel manifest which contains the URL to the actual manifest
        let channel_response = client
            .get(VS_MANIFEST_URL)
            .send()
            .await?;
        
        let channel_data: serde_json::Value = channel_response.json().await?;
        
        // Extract the manifest URL from channel data
        let manifest_url = channel_data["channelItems"]
            .as_array()
            .and_then(|items| {
                items.iter().find(|item| {
                    item["id"].as_str() == Some("Microsoft.VisualStudio.Manifests.VisualStudio")
                })
            })
            .and_then(|item| item["payloads"].as_array())
            .and_then(|payloads| payloads.first())
            .and_then(|payload| payload["url"].as_str())
            .ok_or_else(|| MsvcKitError::Other("Failed to find manifest URL".to_string()))?;

        // Fetch the actual manifest
        let manifest_response = client
            .get(manifest_url)
            .send()
            .await?;

        let manifest: VsManifest = manifest_response.json().await?;
        Ok(manifest)
    }

    /// Find MSVC packages in the manifest
    pub fn find_msvc_packages(&self, version: Option<&str>) -> Vec<Package> {
        let msvc_prefixes = [
            "Microsoft.VC.",
            "Microsoft.VisualCpp.",
        ];

        self.channel_items
            .iter()
            .filter(|item| {
                msvc_prefixes.iter().any(|prefix| item.id.starts_with(prefix))
                    && item.item_type == "Vsix"
            })
            .filter(|item| {
                if let Some(v) = version {
                    item.version.as_ref().map(|iv| iv.contains(v)).unwrap_or(false)
                } else {
                    true
                }
            })
            .map(|item| self.item_to_package(item))
            .collect()
    }

    /// Find Windows SDK packages in the manifest
    pub fn find_sdk_packages(&self, version: Option<&str>) -> Vec<Package> {
        let sdk_prefixes = [
            "Microsoft.VisualStudio.Component.Windows10SDK",
            "Microsoft.VisualStudio.Component.Windows11SDK",
            "Win10SDK",
            "Win11SDK",
        ];

        self.channel_items
            .iter()
            .filter(|item| {
                sdk_prefixes.iter().any(|prefix| item.id.contains(prefix))
            })
            .filter(|item| {
                if let Some(v) = version {
                    item.id.contains(v) || item.version.as_ref().map(|iv| iv.contains(v)).unwrap_or(false)
                } else {
                    true
                }
            })
            .map(|item| self.item_to_package(item))
            .collect()
    }

    /// Convert a channel item to a package
    fn item_to_package(&self, item: &ChannelItem) -> Package {
        let payloads: Vec<PackagePayload> = item
            .payloads
            .iter()
            .map(|p| PackagePayload {
                file_name: p.file_name.clone(),
                url: p.url.clone(),
                size: p.size.unwrap_or(0),
                sha256: p.sha256.clone(),
            })
            .collect();

        let total_size = payloads.iter().map(|p| p.size).sum();

        Package {
            id: item.id.clone(),
            version: item.version.clone().unwrap_or_else(|| "unknown".to_string()),
            package_type: item.item_type.clone(),
            payloads,
            total_size,
        }
    }

    /// Get the latest MSVC version from the manifest
    pub fn get_latest_msvc_version(&self) -> Option<String> {
        self.channel_items
            .iter()
            .filter(|item| item.id.starts_with("Microsoft.VC.") && item.id.contains(".Tools."))
            .filter_map(|item| item.version.clone())
            .max()
    }

    /// Get the latest Windows SDK version from the manifest
    pub fn get_latest_sdk_version(&self) -> Option<String> {
        self.channel_items
            .iter()
            .filter(|item| item.id.contains("Windows10SDK") || item.id.contains("Windows11SDK"))
            .filter_map(|item| {
                // Extract SDK version from ID like "Microsoft.VisualStudio.Component.Windows10SDK.22621"
                item.id.split('.').last().map(|s| format!("10.0.{}.0", s))
            })
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_payload() {
        let payload = PackagePayload {
            file_name: "test.vsix".to_string(),
            url: "https://example.com/test.vsix".to_string(),
            size: 1024,
            sha256: Some("abc123".to_string()),
        };
        assert_eq!(payload.file_name, "test.vsix");
        assert_eq!(payload.size, 1024);
    }
}
