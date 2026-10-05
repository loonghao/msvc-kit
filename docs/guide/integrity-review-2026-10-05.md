# Integrity review, 2026-10-05

## Fixed candidates reviewed

- msvc-kit: `4f6ff42cdc714f9831821711aeba1c2ae9a43f43`
- py-dem-bones: `aba267be7f6368331356a174b4a340066a6811ce`

This source and regression review found merge blockers. It is not a separate
reviewer's approval. CI and independent approval remain required before merge.

## Findings and corrections

1. **P1: mutable manifest metadata could authenticate a poisoned cache.**
   `src/downloader/cache.rs` accepted a body whose SHA matched its neighboring
   metadata file, then reused it after a server 304. Altering both files while
   retaining the original ETag reproduced acceptance of the altered body.
   Channels now always require a fresh full response. Package manifest cache
   reuse requires the digest supplied by that fresh channel as well as the
   local consistency checks. Network and digest failures return errors.
2. **P1: the channel's package manifest SHA was ignored.**
   `src/downloader/manifest.rs` fetched and parsed a valid JSON manifest even
   when its SHA differed from the freshly served channel payload. A fixture
   reproduced this. The digest is now mandatory and checked before cache
   publication or parsing. Existing valid cache bytes survive a failed refresh
   but are not returned as a successful result.
3. **P1: an absent explicit configuration selected defaults.**
   `--config` and `MSVC_KIT_CONFIG` with a nonexistent file exited successfully
   and selected the default root. Missing files now fail before operational
   commands. Intentional `config --set-*`/`--reset` initialization remains
   supported. Malformed explicit files already fail and retain that behavior.
4. **P1: a computed index SHA could replace absent official payload metadata.**
   The payload cache used an index's computed digest when the official manifest
   omitted it. Verification now requires a valid official payload SHA. Neither
   a forged index nor a matching size can supply that trust. Explicitly disabled
   verification retains strict size checks; the Windows build never disables it.

The VSIX size exception remains conditional on full bytes matching an official
SHA. An actual digest mismatch never becomes a size exception.

The corrected CLI source commit is
`5dd2d2ccd7329cbcd001ef2084f723b1fbe79389`. Validation at that source:

| Check | Result |
| --- | --- |
| Library tests, including the forged index regression | 199 passed |
| Manifest integrity integration fixtures | 4 passed |
| Malformed/missing explicit config (flag and environment cases) | 2 tests passed |
| Intentional explicit config creation and precedence | 3 tests passed |
| Clippy, locked dependencies, all targets, warnings denied | Passed |
| Fresh official VS17 probe through this CLI | Exit 1, declared/actual manifest SHA mismatch |

The two initial manifest regressions failed on the frozen input before the
corrections. Raw logs and test cache paths are excluded from public files.

## New upstream integrity blocker

A fresh VS17 channel advertised the following package manifest:

- Channel: <https://aka.ms/vs/17/release/channel>
- Declared SHA256:
  `6e470016e4324c84c255ffd0beb3767d17ec89cc8561e9409ee3e1f6d29400f5`
- Retrieved SHA256:
  `f0a50ea157222c29abd5ea6ff01bfc3c33b04e011c5e45ee2ca38ef0778e5643`
- Declared bytes: 30,443,537; retrieved bytes: 17,954,732.

The direct official URL, an identity-encoding/no-cache request, and a cache-bypass
query returned the same mismatching bytes. There was no Content-Encoding header.
The file contains signature metadata, but this task has not verified its signing
format and trust chain. A computed hash or unverified signature is not an override
for the declared digest. The updated acquisition path fails closed on this input.

`manifest-integrity-evidence.json` records only public source URLs, byte counts
and hashes. This prevents claiming fresh acquisition, all ABI checks, or merge
readiness until an authoritative matching source or independently validated
equivalent integrity proof is available. The earlier x64 wheel's numerical smoke
test remains a functional result for the prior candidate; it does not prove this
new channel-to-manifest integrity boundary.

## PR history and merge gates

The first approved continuation CI run reproduced the official manifest SHA
mismatch in action/bundle acquisition. Rust and coverage also exposed a legacy
CLI fixture using an empty, schema-invalid explicit configuration. That fixture
now serializes valid isolated settings before testing doctor JSON and child exit
codes. Production configuration validation remains unchanged and strict.

At review time #176 remained at
`2736536755659e5f18c064c5d2d5bcc46c3c3e19` and #97 at
`7c80520b6325aeaeb53dc5ffc5232dcb2d1859f8`.

The #176 continuation descends from its existing head. For #97, start from its
existing head and merge the reviewed local migration branch, which incorporates
main `21797949b9fe0d425b35a390fddb553a9632b5d7`; preserve the remote head as an
ancestor. Fetch and compare again immediately before each ordinary push. A remote
change must be reconciled, reviewed and tested. Never force-push.

Only #176 and #97 are approved for remote updates. No release, tag, repository
security change or global toolchain change is part of this work. Do not merge
with a failing integrity check, untested required ABI, or missing independent
review. Other PRs inherit the shared migration only after its approved landing;
their coverage is recorded in py-dem-bones' PR matrix and rollout report.
