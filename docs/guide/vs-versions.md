# Visual Studio Versions & Channels

msvc-kit discovers MSVC toolsets and Windows SDK packages from a Visual Studio
**channel manifest** — the JSON document served at
`https://aka.ms/vs/<major>/release/channel`.

Every channel msvc-kit knows about lives in one table, `VS_CHANNELS`, so Visual
Studio support is data, not logic:

| Major | Release | Channel URL |
|-------|---------|-------------|
| `18` | Visual Studio 2026 | `https://aka.ms/vs/18/release/channel` |
| `17` | Visual Studio 2022 | `https://aka.ms/vs/17/release/channel` |

## Selecting a channel

By default msvc-kit uses **auto** selection: it walks the table newest first and
uses the first channel that actually serves a usable manifest. A Visual Studio
release that upstream has not published yet is skipped with a warning, so a new
release is picked up automatically once Microsoft publishes its channel.

```bash
# Default: newest channel that serves a manifest
msvc-kit download
msvc-kit list --available

# Pin a channel (major version or release year both work)
msvc-kit download --vs-channel 17
msvc-kit download --vs-channel 2022
msvc-kit list --available --vs-channel 2026
```

Accepted selector forms: `17`, `v17`, `vs17`, `2022`, `auto`, `latest`.

You can also pin the channel without touching the command line:

```bash
# Environment variable
export MSVC_KIT_VS_CHANNEL=17

# Persisted configuration
msvc-kit config --set-vs-channel 2022
```

Precedence: `--vs-channel` > `MSVC_KIT_VS_CHANNEL` > config file > auto.

## When a channel is not published yet

Microsoft publishes a channel manifest only once the release is available. Until
then `https://aka.ms/vs/18/release/channel` serves an HTML page instead of JSON.
msvc-kit detects this and degrades cleanly instead of failing with a parse error:

- **Auto selection** skips the channel and falls back to the next one. The skip
  is recorded next to the manifest cache and remembered for 10 minutes, so an
  unpublished release is not re-probed on every command; the reason is logged at
  `debug` level, not `warn`, because walking past an unpublished channel is the
  normal case:

  ```text
  Visual Studio channel: Visual Studio 2022 (v17)
  ```

- **Pinned selection** reports a typed error, `ChannelUnavailable`, with the URL
  and the reason. It never silently falls back to another Visual Studio version,
  and it always talks to the network, so the “not published yet” answer is never
  hidden behind the 10 minute skip above.

- **Transport failures** (5xx, `429`, `408`, `407`) are *not* treated as an
  unpublished channel: they surface as `TransientHttp` instead of falling back to
  an older Visual Studio, so a proxy or outage is not mistaken for “Microsoft has
  not released it yet”.

An unusable response (HTML, empty body, invalid JSON, or a manifest without
packages) is never kept in the manifest cache, and the skip entry is dropped as
soon as a channel serves a usable manifest, so the channel is picked up as soon
as upstream publishes it.

## Adding support for a new Visual Studio version

Adding a release is a **one-row data change** in `src/vs_channel.rs`:

1. Open `src/vs_channel.rs` and add an entry to the `VS_CHANNELS` table. Keep the
   table sorted **newest first** — auto selection relies on that order:

   ```rust
   pub const VS_CHANNELS: &[VsChannelEntry] = &[
       VsChannelEntry {
           major: 19,
           year: 2027,
           channel_url: "https://aka.ms/vs/19/release/channel",
       },
       VsChannelEntry {
           major: 18,
           year: 2026,
           channel_url: "https://aka.ms/vs/18/release/channel",
       },
       // ...
   ];
   ```

2. Nothing else is required. Selection (`--vs-channel 19`, `--vs-channel 2027`),
   the library API, per-channel manifest caching (the manifest is cached as
   `channel-v19.json`) and the "newest first" auto fallback all read the table.

3. Add a test. `src/vs_channel.rs` has table tests; extend them with the new
   major/year pair, e.g. `selectors_resolve_to_the_same_channel`.

4. Update the table at the top of this page.

### Zero-code escape hatch

A bare major version that is **not** in the table still works: it resolves
through the `VS_CHANNEL_URL_TEMPLATE` (`https://aka.ms/vs/{major}/release/channel`).
So `msvc-kit download --vs-channel 19` targets a new release before any code
change; adding the row only adds the friendly release name and makes it the auto
selection candidate.

## Library API

```rust
use msvc_kit::downloader::{list_available_versions_with_selection, VsManifest};
use msvc_kit::vs_channel::{VsChannelSelection, VsChannelSpec};

// Discover packages from a specific Visual Studio release
let selection = VsChannelSelection::Pinned(VsChannelSpec::from_major(18));
let versions = list_available_versions_with_selection(selection).await?;
println!("served by: {:?}", versions.channel);

// Or take whatever the newest published channel offers
let versions = msvc_kit::list_available_versions().await?;
# Ok::<(), msvc_kit::MsvcKitError>(())
```

`DownloadOptions::builder().vs_channel("2026")` pins the channel for
`download_msvc` / `download_sdk`; `VsManifest::fetch_with_selection` returns the
manifest together with the channel that served it.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Unknown Visual Studio channel 'x'` | The selector is not a major version, a release year, or `auto`/`latest`. | Use `17`, `2022`, `auto`, … — the error lists the known channels. |
| `Visual Studio channel … is not available` (pinned) | Upstream has not published that channel manifest yet. | Drop the flag, or pin an older release such as `--vs-channel 2022`. |
| Auto selection logs "skipped as unavailable" | A newer channel is registered but not published yet. | Informational — msvc-kit used the next channel. Pin a channel to silence it. |
