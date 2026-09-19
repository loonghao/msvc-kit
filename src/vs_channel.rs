//! Visual Studio release channel registry
//!
//! msvc-kit discovers MSVC toolsets and Windows SDK packages from a Visual
//! Studio *channel manifest*. A channel is identified by the Visual Studio
//! major version — the `17` in `https://aka.ms/vs/17/release/channel`.
//!
//! Every piece of channel knowledge lives in the [`VS_CHANNELS`] table, so
//! adding support for a new Visual Studio release is a data change:
//!
//! 1. Add one [`VsChannelEntry`] row (major version, release year, channel URL).
//! 2. Nothing else: version discovery, caching, CLI and library selection all
//!    read the table.
//!
//! Channels whose manifest has not been published upstream yet are handled
//! gracefully — see [`VsManifest::fetch_with_selection`].
//!
//! # Selecting a channel
//!
//! ```rust
//! use msvc_kit::vs_channel::{VsChannelSelection, parse_channel_selector};
//!
//! // Explicit selection: "18", "v18", "vs18", "2026" all select Visual Studio 2026.
//! let spec = parse_channel_selector("2026")?.expect("selector");
//! assert_eq!(spec.major, 18);
//!
//! // "auto"/"latest" (or no value) lets msvc-kit pick the newest channel that
//! // actually serves a manifest.
//! assert!(parse_channel_selector("auto")?.is_none());
//! # Ok::<(), msvc_kit::MsvcKitError>(())
//! ```
//!
//! [`VsManifest::fetch_with_selection`]: crate::downloader::VsManifest::fetch_with_selection

use std::fmt;

use crate::error::{MsvcKitError, Result};

/// URL template used to build a channel manifest URL for any VS major version.
///
/// `{major}` is replaced by the Visual Studio major version. Known channels use
/// the explicit URL in [`VS_CHANNELS`]; the template is the fallback that lets a
/// brand new major version work before a row is added.
pub const VS_CHANNEL_URL_TEMPLATE: &str = "https://aka.ms/vs/{major}/release/channel";

/// Environment variable used to pin the Visual Studio channel.
pub const VS_CHANNEL_ENV_VAR: &str = "MSVC_KIT_VS_CHANNEL";

/// A row of the Visual Studio channel registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VsChannelEntry {
    /// Visual Studio major version, e.g. `18` for Visual Studio 2026
    pub major: u8,
    /// Marketing year of the release, e.g. `2026`
    pub year: u16,
    /// Channel manifest URL
    pub channel_url: &'static str,
}

/// Registry of known Visual Studio release channels.
///
/// **Invariant: the table is ordered newest first.** [`VsChannelSelection::Auto`]
/// walks it top-down and uses the first channel that serves a usable manifest,
/// so a new release becomes the default as soon as upstream publishes it.
pub const VS_CHANNELS: &[VsChannelEntry] = &[
    VsChannelEntry {
        major: 18,
        year: 2026,
        channel_url: "https://aka.ms/vs/18/release/channel",
    },
    VsChannelEntry {
        major: 17,
        year: 2022,
        channel_url: "https://aka.ms/vs/17/release/channel",
    },
];

/// A resolved Visual Studio channel.
///
/// Owned counterpart of [`VsChannelEntry`]: it is also used for channels that
/// are not in the registry yet (built from [`VS_CHANNEL_URL_TEMPLATE`]) and for
/// tests that serve a channel from a local URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VsChannelSpec {
    /// Visual Studio major version
    pub major: u8,
    /// Marketing year, `None` when the channel is not in [`VS_CHANNELS`]
    pub year: Option<u16>,
    /// Channel manifest URL
    pub channel_url: String,
}

impl VsChannelSpec {
    /// Create a channel spec from its parts
    pub fn new(major: u8, year: Option<u16>, channel_url: impl Into<String>) -> Self {
        Self {
            major,
            year,
            channel_url: channel_url.into(),
        }
    }

    /// Build a spec from a registry row
    pub fn from_entry(entry: &VsChannelEntry) -> Self {
        Self::new(entry.major, Some(entry.year), entry.channel_url)
    }

    /// Build a spec for a major version.
    ///
    /// Known majors use the registry URL; unknown ones fall back to
    /// [`VS_CHANNEL_URL_TEMPLATE`] so a newly released Visual Studio works before
    /// the table is updated.
    pub fn from_major(major: u8) -> Self {
        match entry_for_major(major) {
            Some(entry) => Self::from_entry(entry),
            None => Self::new(
                major,
                None,
                VS_CHANNEL_URL_TEMPLATE.replace("{major}", &major.to_string()),
            ),
        }
    }

    /// Human readable name, e.g. `Visual Studio 2026 (v18)`
    pub fn display_name(&self) -> String {
        match self.year {
            Some(year) => format!("Visual Studio {} (v{})", year, self.major),
            None => format!("Visual Studio v{}", self.major),
        }
    }

    /// Short slug used for cache file names, e.g. `18`
    pub fn cache_slug(&self) -> String {
        format!("v{}", self.major)
    }

    /// Cache file name for the channel manifest, e.g. `channel-v18.json`
    pub fn channel_cache_file(&self) -> String {
        format!("channel-{}.json", self.cache_slug())
    }
}

impl fmt::Display for VsChannelSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_name())
    }
}

/// All known channels, newest first
pub fn known_channels() -> Vec<VsChannelSpec> {
    VS_CHANNELS.iter().map(VsChannelSpec::from_entry).collect()
}

/// Look up a registry row by major version
pub fn entry_for_major(major: u8) -> Option<&'static VsChannelEntry> {
    VS_CHANNELS.iter().find(|entry| entry.major == major)
}

/// Look up a registry row by release year
pub fn entry_for_year(year: u16) -> Option<&'static VsChannelEntry> {
    VS_CHANNELS.iter().find(|entry| entry.year == year)
}

/// Comma separated list of known channels, used in error messages
pub fn known_channel_summary() -> String {
    VS_CHANNELS
        .iter()
        .map(|entry| VsChannelSpec::from_entry(entry).display_name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Which Visual Studio channel to use for manifest discovery.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum VsChannelSelection {
    /// Use the newest channel that serves a usable manifest
    #[default]
    Auto,
    /// Use exactly this channel (no fallback)
    Pinned(VsChannelSpec),
}

impl VsChannelSelection {
    /// Auto-select the newest available channel
    pub fn auto() -> Self {
        Self::Auto
    }

    /// Pin a specific channel
    pub fn pinned(spec: VsChannelSpec) -> Self {
        Self::Pinned(spec)
    }

    /// Resolve an optional user provided selector.
    ///
    /// `None`, an empty string and the keywords `auto` / `latest` / `default`
    /// resolve to [`VsChannelSelection::Auto`].
    pub fn from_optional(selector: Option<&str>) -> Result<Self> {
        match parse_channel_selector(selector.unwrap_or_default())? {
            Some(spec) => Ok(Self::Pinned(spec)),
            None => Ok(Self::Auto),
        }
    }

    /// Resolve the channel from the [`VS_CHANNEL_ENV_VAR`] environment variable
    pub fn from_env() -> Result<Self> {
        match std::env::var(VS_CHANNEL_ENV_VAR) {
            Ok(value) => Self::from_optional(Some(&value)),
            Err(_) => Ok(Self::Auto),
        }
    }

    /// Human readable description of the selection
    pub fn describe(&self) -> String {
        match self {
            Self::Auto => "auto (newest available channel)".to_string(),
            Self::Pinned(spec) => spec.display_name(),
        }
    }
}

/// Parse a user supplied channel selector.
///
/// Accepted forms: `18`, `v18`, `vs18`, `2026`, `visual studio 2026`, plus the
/// keywords `auto`, `latest` and `default` (which return `Ok(None)` meaning
/// "newest available channel").
///
/// An unknown major version is *not* rejected: it resolves to the URL built from
/// [`VS_CHANNEL_URL_TEMPLATE`], so msvc-kit can target a Visual Studio release
/// before its row is added to [`VS_CHANNELS`].
pub fn parse_channel_selector(selector: &str) -> Result<Option<VsChannelSpec>> {
    let raw = selector.trim();
    if raw.is_empty() {
        return Ok(None);
    }

    let normalized = raw.to_ascii_lowercase();
    if matches!(normalized.as_str(), "auto" | "latest" | "default") {
        return Ok(None);
    }

    // Accept "18", "v18", "vs18", "visual studio 2026", ...
    let token = ["visualstudio", "visual studio", "vs", "v"]
        .iter()
        .fold(normalized.as_str(), |acc, prefix| {
            acc.trim_start_matches(prefix)
        })
        .trim();

    if let Ok(year) = token.parse::<u16>() {
        if (1900..=2200).contains(&year) {
            return match entry_for_year(year) {
                Some(entry) => Ok(Some(VsChannelSpec::from_entry(entry))),
                None => Err(MsvcKitError::UnknownVsChannel {
                    selector: raw.to_string(),
                    known: known_channel_summary(),
                }),
            };
        }
    }

    match token.parse::<u8>() {
        Ok(major) if major > 0 => Ok(Some(VsChannelSpec::from_major(major))),
        _ => Err(MsvcKitError::UnknownVsChannel {
            selector: raw.to_string(),
            known: known_channel_summary(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_ordered_newest_first() {
        let majors: Vec<u8> = VS_CHANNELS.iter().map(|entry| entry.major).collect();
        let mut sorted = majors.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(majors, sorted, "VS_CHANNELS must be sorted newest first");
    }

    #[test]
    fn registry_covers_vs2026_and_vs2022() {
        let v18 = entry_for_major(18).expect("VS 2026 row");
        assert_eq!(v18.year, 2026);
        assert!(v18.channel_url.contains("/vs/18/"));

        let v17 = entry_for_major(17).expect("VS 2022 row");
        assert_eq!(v17.year, 2022);
        assert!(v17.channel_url.contains("/vs/17/"));
    }

    #[test]
    fn known_channels_are_newest_first() {
        let channels = known_channels();
        assert_eq!(channels.first().map(|c| c.major), Some(18));
        assert_eq!(channels.last().map(|c| c.major), Some(17));
        assert!(channels
            .iter()
            .all(|c| c.channel_url.starts_with("https://")));
    }

    #[test]
    fn selectors_resolve_to_the_same_channel() {
        for selector in ["18", "v18", "V18", "vs18", "VS18", "2026", "vs 2026"] {
            let spec = parse_channel_selector(selector)
                .unwrap_or_else(|e| panic!("selector {selector}: {e}"))
                .unwrap_or_else(|| panic!("selector {selector} should resolve"));
            assert_eq!(spec.major, 18, "selector {selector}");
            assert_eq!(spec.year, Some(2026));
            assert_eq!(
                spec.channel_url, "https://aka.ms/vs/18/release/channel",
                "selector {selector}"
            );
        }

        for selector in ["17", "v17", "2022", "visual studio 2022"] {
            let spec = parse_channel_selector(selector)
                .unwrap()
                .expect("should resolve");
            assert_eq!(spec.major, 17, "selector {selector}");
        }
    }

    #[test]
    fn auto_keywords_and_empty_select_auto() {
        for selector in ["", "  ", "auto", "AUTO", "latest", "default"] {
            assert!(
                parse_channel_selector(selector)
                    .unwrap_or_else(|e| panic!("selector {selector:?}: {e}"))
                    .is_none(),
                "selector {selector:?} should mean auto"
            );
        }
    }

    #[test]
    fn unknown_major_falls_back_to_the_url_template() {
        let spec = parse_channel_selector("19")
            .unwrap()
            .expect("should resolve");
        assert_eq!(spec.major, 19);
        assert_eq!(spec.year, None);
        assert_eq!(spec.channel_url, "https://aka.ms/vs/19/release/channel");
    }

    #[test]
    fn unknown_selector_is_rejected_with_a_helpful_error() {
        let err = parse_channel_selector("banana").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("banana"), "{message}");
        assert!(message.contains("Visual Studio 2026"), "{message}");
        assert!(message.contains("Visual Studio 2022"), "{message}");
    }

    #[test]
    fn unknown_year_is_rejected() {
        assert!(parse_channel_selector("1999").is_err());
    }

    #[test]
    fn selection_from_optional_maps_keywords_to_auto() {
        assert_eq!(
            VsChannelSelection::from_optional(None).unwrap(),
            VsChannelSelection::Auto
        );
        assert_eq!(
            VsChannelSelection::from_optional(Some("latest")).unwrap(),
            VsChannelSelection::Auto
        );
        assert_eq!(
            VsChannelSelection::from_optional(Some("2026")).unwrap(),
            VsChannelSelection::Pinned(VsChannelSpec::from_major(18))
        );
    }

    #[test]
    fn display_names_and_cache_names() {
        let known = VsChannelSpec::from_major(18);
        assert_eq!(known.display_name(), "Visual Studio 2026 (v18)");
        assert_eq!(known.cache_slug(), "v18");
        assert_eq!(known.channel_cache_file(), "channel-v18.json");

        let future = VsChannelSpec::from_major(19);
        assert_eq!(future.display_name(), "Visual Studio v19");
        assert_eq!(future.channel_cache_file(), "channel-v19.json");
    }

    #[test]
    fn describe_reports_selection() {
        assert_eq!(
            VsChannelSelection::Auto.describe(),
            "auto (newest available channel)"
        );
        assert_eq!(
            VsChannelSelection::Pinned(VsChannelSpec::from_major(17)).describe(),
            "Visual Studio 2022 (v17)"
        );
    }
}
