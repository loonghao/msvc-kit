//! Integration tests for the Visual Studio channel registry
//!
//! These cover the public extension surface: adding a Visual Studio release is
//! a data change in `msvc_kit::vs_channel`, and selecting one is a one-liner on
//! [`DownloadOptions`].

use msvc_kit::vs_channel::{
    known_channels, parse_channel_selector, VsChannelSelection, VsChannelSpec, VS_CHANNELS,
};
use msvc_kit::{DownloadOptions, MsvcKitError};

#[test]
fn registry_exposes_vs2026_and_vs2022() {
    let majors: Vec<u8> = VS_CHANNELS.iter().map(|entry| entry.major).collect();
    assert!(
        majors.contains(&18),
        "Visual Studio 2026 (v18) must be registered: {majors:?}"
    );
    assert!(
        majors.contains(&17),
        "Visual Studio 2022 (v17) must be registered: {majors:?}"
    );

    let v18 = VS_CHANNELS
        .iter()
        .find(|entry| entry.major == 18)
        .expect("v18 row");
    assert_eq!(v18.year, 2026);
    assert!(v18.channel_url.starts_with("https://"));
}

#[test]
fn registry_is_newest_first() {
    let channels = known_channels();
    let majors: Vec<u8> = channels.iter().map(|c| c.major).collect();
    let mut sorted = majors.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(majors, sorted, "auto selection walks the table top-down");
}

#[test]
fn users_can_select_a_channel_by_year_or_major() {
    for selector in ["18", "v18", "vs18", "2026"] {
        let spec = parse_channel_selector(selector)
            .unwrap_or_else(|err| panic!("{selector} should parse: {err}"))
            .unwrap_or_else(|| panic!("{selector} should resolve to a channel"));

        assert_eq!(spec.major, 18, "{selector}");
        assert_eq!(spec.display_name(), "Visual Studio 2026 (v18)");
    }

    for selector in ["17", "2022"] {
        let spec = parse_channel_selector(selector).unwrap().unwrap();
        assert_eq!(spec.major, 17, "{selector}");
    }
}

#[test]
fn future_major_versions_resolve_from_the_url_template() {
    // Adding a row is one line; even without it a new major version resolves.
    let spec = parse_channel_selector("19").unwrap().unwrap();
    assert_eq!(spec.major, 19);
    assert_eq!(spec.channel_url, "https://aka.ms/vs/19/release/channel");
    assert_eq!(spec.channel_cache_file(), "channel-v19.json");
}

#[test]
fn invalid_selectors_are_rejected() {
    let err = parse_channel_selector("not-a-channel").unwrap_err();
    assert!(
        matches!(err, MsvcKitError::UnknownVsChannel { .. }),
        "{err:?}"
    );
    assert!(err.to_string().contains("Visual Studio 2026"));
}

#[test]
fn download_options_carry_the_channel_selection() {
    let options = DownloadOptions::builder().vs_channel("2026").build();

    let selection = options.vs_channel_selection().expect("valid selector");
    assert_eq!(
        selection,
        VsChannelSelection::Pinned(VsChannelSpec::from_major(18))
    );
    assert_eq!(
        selection.describe(),
        "Visual Studio 2026 (v18)",
        "selection is reported back to the user"
    );
}

#[test]
fn download_options_default_to_auto_selection() {
    let options = DownloadOptions::builder().build();
    let selection = options.vs_channel_selection().expect("auto is valid");

    match selection {
        VsChannelSelection::Auto => {}
        // Honoured when MSVC_KIT_VS_CHANNEL is set in the environment.
        VsChannelSelection::Pinned(spec) => assert!(spec.major >= 15),
    }
}

#[test]
fn auto_and_latest_mean_newest_available_channel() {
    for selector in ["auto", "latest", "", "  "] {
        let selection = VsChannelSelection::from_optional(Some(selector))
            .unwrap_or_else(|err| panic!("{selector:?} should parse: {err}"));
        assert_eq!(selection, VsChannelSelection::Auto, "{selector:?}");
    }

    assert_eq!(
        VsChannelSelection::from_optional(None).unwrap(),
        VsChannelSelection::Auto
    );
}
