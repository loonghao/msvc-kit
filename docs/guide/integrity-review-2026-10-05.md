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
   The initial correction required a fresh full channel response and its digest
   for package manifest cache reuse, in addition to local consistency checks.
   Network and digest failures returned errors. The authenticated Windows
   alternative added on 2026-10-08 is described below.
2. **P1: the channel's package manifest SHA was ignored.**
   `src/downloader/manifest.rs` fetched and parsed a valid JSON manifest even
   when its SHA differed from the freshly served channel payload. A fixture
   reproduced this. The initial correction made the channel digest mandatory
   before cache publication or parsing. Existing valid cache bytes survived a
   failed refresh but were not returned as a successful result.
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

## Upstream mismatch observed on 2026-10-05

A fresh VS17 channel advertised the following package manifest:

- Channel: <https://aka.ms/vs/17/release/channel>
- Declared SHA256:
  `6e470016e4324c84c255ffd0beb3767d17ec89cc8561e9409ee3e1f6d29400f5`
- Retrieved SHA256:
  `f0a50ea157222c29abd5ea6ff01bfc3c33b04e011c5e45ee2ca38ef0778e5643`
- Declared bytes: 30,443,537; retrieved bytes: 17,954,732.

The direct official URL, an identity-encoding/no-cache request, and a cache-bypass
query returned the same mismatching bytes. There was no Content-Encoding header.
At that point, the file's signing format and trust chain had not been verified.
A computed hash or unverified signature could not override the declared digest.
The initial corrected acquisition path therefore failed closed on this input.

`manifest-integrity-evidence.json` records only public source URLs, byte counts
and hashes. Those results did not establish fresh acquisition, all ABI checks, or
merge readiness. They required an authoritative matching source or independently
validated equivalent integrity proof. The earlier x64 wheel's numerical smoke
test remains a functional result for the prior candidate; it does not prove this
new channel-to-manifest integrity boundary.

## Signed-manifest resolution added on 2026-10-08

Matching the fresh channel's SHA256 remains the standard authentication path.
For a mismatching package catalog, the Windows-only alternative verifies the
original SHA256/RSA signatures of both the fresh channel and catalog. Windows
must validate each signer's current code-signing chain, explicit Code Signing
EKU, Microsoft publisher CN and organization, and revocation status. Unavailable
revocation information fails closed. The chain must end at Microsoft Root
Certificate Authority 2011, pinned by DER SHA256:
`847df6a78497943f27fc72eb93f9a637320a02b561d0a91b09e87a7807ed7c61`.
The root certificate is independently published by
[Microsoft PKI](https://www.microsoft.com/pki/certs/MicRooCerAut2011_2011_03_22.crt).

The catalog must match the authenticated channel's `buildVersion`, channel item
version, `productLine` and `productSemanticVersion`, with the expected
`manifestType` and `manifestName`. Cached fallback bytes repeat the signature,
current certificate trust and release-identity checks before 304 reuse. The
implementation uses native Windows cryptography without an external process or
additional DLL. Other platforms continue rejecting a catalog digest mismatch.
Individual package SHA256 verification is unchanged.

This replaces the earlier digest-only acquisition gate with an independently
authenticated alternative; it does not assert that current PR-head CI has passed.
Fresh acquisition, the Action matrix and Bundle validation must succeed on the
exact candidate before merge.

Local validation with Rust 1.93.1 passed the complete all-features test suite,
including 23 doctests. A live-cache test using real Microsoft VS17 signed
documents passed the initial 200 download, signature revalidation before 304
reuse, and rejection of an invalid fresh channel while preserving the previous
cache. VitePress documentation also built successfully. These local results do
not establish current PR-head CI success; Clippy validation was still running
when this evidence was recorded.

## Independent receipt review follow-up

A separate reviewer found that `record_installation` recorded an observed file
SHA without checking whether it matched the official expected payload SHA.
Unchecked downloads and same-size changes after acquisition could therefore
produce ordinary v1 source receipts. This did not bypass py-dem-bones' strict
acquisition, which never enables `--no-verify`, but it blurred verified and
unverified source provenance.

Receipt generation now requires a Completed index entry, a valid official SHA,
verified status and a matching computed index SHA, then rehashes the entire
archive against that official SHA. Every payload must pass before atomic receipt
publication; failures preserve the previous receipt. A matching SHA still allows
the actual VSIX size to replace stale declared size. Empty payload receipts fail.

New receipts use `msvc-kit.installation-receipt.v2`. Capture, lock loading and
download verification reject old v1 receipts because they did not distinguish
verified provenance. Selection-only locks with no receipts retain their v1
container format. Old source receipts require fresh verified acquisition, not
a schema edit or a locally computed digest. These local records are editable
source pins, not independent publisher attestations or proofs that an installed
tree is unmodified. Every strict acquisition still checks the fresh official
channel/catalog/payload chain.

The new receipt regressions first failed on the frozen v1 source. The corrected
lock suite covers unchecked entries, absent/malformed expected hashes, stale
computed hashes, same-size archive mutation, partial/empty payload sets, v1
rejection, preservation after failure, and matching official SHA with stale size.
At that stage the official manifest mismatch remained an acquisition gate; the
2026-10-08 signed-manifest resolution above addresses that separate boundary.

Validation used Rust 1.93.1 and a single compiler worker: the lock/receipt suite
passed 7 tests, doctor passed 7, and execution passed 3. Locked Clippy for the
library, CLI and lock tests passed with warnings denied; formatting passed.

## PR history and merge gates

The following records the earlier continuation and its gates, before the
2026-10-08 signed-manifest implementation.

The first approved continuation CI run reproduced the official manifest SHA
mismatch in action/bundle acquisition. Rust and coverage also exposed a legacy
CLI fixture using an empty, schema-invalid explicit configuration. That fixture
now serializes valid isolated settings before testing doctor JSON and child exit
codes. Production configuration validation remains unchanged and strict.

The corrected fixture's GitHub Rust Tests, Coverage, Clippy and three target
pre-build jobs passed on `b6d38e423e2eda25d47851ba03db4c062272f69e`.
The separate Codecov patch check remained below its existing target (71.84%
versus 78.25%). Additional contract tests now cover child environment isolation
and exit/spawn failures, CMake path quoting and preservation after a failed
write, receipt capture and portable roundtrips, missing provenance, invalid
receipt/payload identities, and an opt-in compiler launch failure. The focused
suite passed 15 tests (7 doctor, 3 execution, 5 lock) with Rust 1.93.1 and one
compiler worker. These are test changes; production Rust source remains identical
to the immutable CLI revision pinned by py-dem-bones. Fresh CI must validate the
resulting PR head and its Codecov patch result; no threshold is lowered.

At review time #176 remained at
`2736536755659e5f18c064c5d2d5bcc46c3c3e19` and #97 at
`7c80520b6325aeaeb53dc5ffc5232dcb2d1859f8`.

The #176 continuation descends from its existing head. For #97, start from its
existing head and merge the reviewed local migration branch, which incorporates
main `21797949b9fe0d425b35a390fddb553a9632b5d7`; preserve the remote head as an
ancestor. Fetch and compare again immediately before each ordinary push. A remote
change must be reconciled, reviewed and tested. Never force-push.

The scope recorded for that review approved only #176 and #97 for remote updates.
No release, tag, repository security change or global toolchain change was part
of that work. Do not merge
with a failing integrity check, untested required ABI, or missing independent
review. Other PRs inherit the shared migration only after its approved landing;
their coverage is recorded in py-dem-bones' PR matrix and rollout report.
