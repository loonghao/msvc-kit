//! Negative cache for Visual Studio channels that are not available (yet)
//!
//! Upstream serves an HTML page instead of a JSON manifest until a channel is
//! published, so auto selection — which walks the registry newest first — probes
//! the newest channel on every call. Without a negative cache every `download`
//! and every `list --available` pays that extra round trip (and downloads an
//! HTML page) just to fall back to the previous release.
//!
//! The cache is deliberately conservative:
//!
//! * entries live next to the other manifest cache files and expire after
//!   [`UNAVAILABLE_TTL`], so a channel that gets published is picked up again
//!   without a code change or a manual cache clear;
//! * only auto selection reads it. An explicitly pinned channel always talks to
//!   the network, so "the release you asked for is not out yet" stays visible;
//! * a channel that serves a usable manifest drops its entry immediately.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::vs_channel::VsChannelSpec;

/// How long a channel stays "known unavailable" before it is probed again.
///
/// Short on purpose: the newest Visual Studio can be published at any time, and
/// this cache must never be the reason a released channel is missed.
pub const UNAVAILABLE_TTL: Duration = Duration::from_secs(10 * 60);

/// A negative cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UnavailableEntry {
    /// Channel manifest URL the entry was recorded for
    url: String,
    /// Why the channel was considered unavailable
    reason: String,
    /// When the channel was probed, in milliseconds since the Unix epoch
    checked_at_ms: u64,
}

/// Cache file holding the unavailability of a channel
pub(crate) fn entry_path(cache_dir: &Path, channel: &VsChannelSpec) -> PathBuf {
    cache_dir.join(format!("channel-{}.unavailable.json", channel.cache_slug()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        // A system clock before 1970 makes every entry stale, which is the
        // safe direction: re-probe instead of trusting the entry.
        .unwrap_or(0)
}

fn is_fresh(checked_at_ms: u64, ttl: Duration) -> bool {
    let checked_at = UNIX_EPOCH + Duration::from_millis(checked_at_ms);

    SystemTime::now()
        .duration_since(checked_at)
        .map(|age| age <= ttl)
        // The clock jumped backwards: do not trust the entry.
        .unwrap_or(false)
}

/// Why `channel` is known to be unavailable, as long as the entry is fresh
pub(crate) async fn lookup(cache_dir: &Path, channel: &VsChannelSpec) -> Option<String> {
    lookup_with_ttl(cache_dir, channel, UNAVAILABLE_TTL).await
}

/// [`lookup`] with an explicit TTL
pub(crate) async fn lookup_with_ttl(
    cache_dir: &Path,
    channel: &VsChannelSpec,
    ttl: Duration,
) -> Option<String> {
    let bytes = tokio::fs::read(entry_path(cache_dir, channel)).await.ok()?;
    let entry: UnavailableEntry = serde_json::from_slice(&bytes).ok()?;

    // The cache file is keyed by channel slug; the URL guards against a channel
    // whose URL changed (for example behind a different mirror).
    if entry.url != channel.channel_url {
        return None;
    }

    is_fresh(entry.checked_at_ms, ttl).then_some(entry.reason)
}

/// Remember that `channel` does not serve a usable manifest
pub(crate) async fn record(cache_dir: &Path, channel: &VsChannelSpec, reason: &str) {
    record_at(cache_dir, channel, reason, now_ms()).await
}

/// [`record`] with an explicit check time
///
/// Only useful to build an entry that is already past its TTL.
pub(crate) async fn record_at(
    cache_dir: &Path,
    channel: &VsChannelSpec,
    reason: &str,
    checked_at_ms: u64,
) {
    let entry = UnavailableEntry {
        url: channel.channel_url.clone(),
        reason: reason.to_string(),
        checked_at_ms,
    };

    let Ok(bytes) = serde_json::to_vec(&entry) else {
        return;
    };

    let path = entry_path(cache_dir, channel);
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    // A read-only cache directory must not turn into a hard failure: the only
    // consequence is that the channel is probed again next time.
    let _ = tokio::fs::write(&path, &bytes).await;
}

/// Forget the entry of a channel that just served a usable manifest
pub(crate) async fn forget(cache_dir: &Path, channel: &VsChannelSpec) {
    let _ = tokio::fs::remove_file(entry_path(cache_dir, channel)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    fn channel(major: u8, url: &str) -> VsChannelSpec {
        VsChannelSpec::new(major, None, url)
    }

    #[tokio::test]
    async fn a_recorded_entry_is_returned_while_fresh() {
        let dir = temp_dir();
        let channel = channel(18, "https://example.com/vs/18/release/channel");

        record(dir.path(), &channel, "not published yet").await;

        assert_eq!(
            lookup(dir.path(), &channel).await.as_deref(),
            Some("not published yet")
        );
        assert!(entry_path(dir.path(), &channel).exists());
    }

    #[tokio::test]
    async fn an_expired_entry_is_ignored() {
        let dir = temp_dir();
        let channel = channel(18, "https://example.com/vs/18/release/channel");

        record(dir.path(), &channel, "not published yet").await;

        // A zero TTL has already passed by the time we look the entry up.
        assert_eq!(
            lookup_with_ttl(dir.path(), &channel, Duration::ZERO).await,
            None
        );
        assert_eq!(
            lookup_with_ttl(dir.path(), &channel, Duration::from_secs(60))
                .await
                .as_deref(),
            Some("not published yet")
        );
    }

    #[tokio::test]
    async fn an_unknown_channel_has_no_entry() {
        let dir = temp_dir();
        let channel = channel(18, "https://example.com/vs/18/release/channel");

        assert_eq!(lookup(dir.path(), &channel).await, None);
    }

    #[tokio::test]
    async fn an_entry_for_another_url_is_ignored() {
        let dir = temp_dir();
        let recorded = channel(18, "https://example.com/vs/18/release/channel");
        let moved = channel(18, "https://mirror.example.com/vs/18/release/channel");

        record(dir.path(), &recorded, "not published yet").await;

        assert_eq!(
            lookup(dir.path(), &moved).await,
            None,
            "the cached URL no longer matches, so the channel must be probed again"
        );
    }

    #[tokio::test]
    async fn forget_drops_the_entry() {
        let dir = temp_dir();
        let channel = channel(18, "https://example.com/vs/18/release/channel");

        record(dir.path(), &channel, "not published yet").await;
        forget(dir.path(), &channel).await;

        assert_eq!(lookup(dir.path(), &channel).await, None);
    }

    #[tokio::test]
    async fn a_corrupted_entry_is_ignored() {
        let dir = temp_dir();
        let channel = channel(18, "https://example.com/vs/18/release/channel");

        tokio::fs::write(entry_path(dir.path(), &channel), b"not json")
            .await
            .expect("write");

        assert_eq!(lookup(dir.path(), &channel).await, None);
    }
}
