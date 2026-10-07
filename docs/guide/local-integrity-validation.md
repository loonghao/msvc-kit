# Local integrity continuation of #176 — 2026-10-05

The official #176 head inspected was
`2736536755659e5f18c064c5d2d5bcc46c3c3e19`; main was
`e876b446b640a0cdfbe0f2dcfd4955fb9385b08f`. Both remained unchanged at final review.
No remote write, release, security setting or global toolchain change was made.
Existing local uncommitted work was read and reused in a separate clone.

The Bundle Validation and Test Action failures are reproducible against the fresh
official VS17 manifest. Four failing VSIX files have exact matching authoritative
SHA-256 values and valid ZIP contents, while their manifest sizes are stale.
Raw public URLs, sizes and digests are in `vsix-size-evidence.json`.

The downloader verifies the full authoritative SHA-256 before publishing bytes.
A size difference is accepted only with an enabled, matching manifest digest.
An index's own digest cannot authorize the exception. Missing authoritative
digests and disabled hash verification retain strict size checking. Cache reuse
rehashes disk contents and records actual disk bytes while retaining declared
size in source identity. Failed validation preserves the old final file.

The continuation also makes invalid explicit --config/MSVC_KIT_CONFIG input fatal,
so it cannot silently switch installation/cache roots. Default implicit config
behavior remains as before.

Validation: 29 downloader tests pass; the full Rust library suite has 198 passes;
the new explicit-config regression passes both selectors; cargo fmt and
all-target cargo clippy with warnings denied pass. Tests used existing Rust
1.93.1 and an isolated target, with two build threads.

The resulting CLI acquired MSVC 14.44.35207 and SDK 10.0.26100.0 from Microsoft
with hashes enabled. Both receipts exist; a per-run lock records 315 payloads.
Compile/resource/link/manifest/execute doctor probes pass. A py-dem-bones 0.13.1
CPython 3.12 x64 wheel built with this toolchain, repaired runtime DLLs and passed
272 runtime tests in a clean venv (3 legacy computation tests skip on exceptions), plus native
numerical reconstruction. Its compiler and wheel summaries are recorded in the
py-dem-bones continuation's validation report.

Remote action/bundle matrices and native Windows ARM64 wheel execution remain
pending. Current open msvc-kit PRs are #176, #175 (Tokio; observed checks pass),
and #164 (release proposal; no checks observed). Suggested landing order is the
#176 continuation, then refreshing #175 onto the reviewed fix; #164 stays with
the normal release process. No remote branch was changed by this task.
