# Documentation contract audit

Every entry below was produced by reading the implementation and the
documentation side by side. "Documentation" is what the English and Chinese docs
claimed before this refresh; "Implementation" is what the code does.

Scope: `README.md`, `README_zh.md`, `docs/**`, `action.yml`. The audit ran
against the tree that contains the cache-directory wiring, the Visual Studio
channel registry (VS 2026) and portable mode.

Status values:

- **Fixed** — documentation corrected in this change.
- **Already fixed** — the code changed first (stage 1) and the documentation
  already matched; listed so the stage 1 work can be re-checked.
- **Open** — a real implementation gap. Per the issue's hard constraint, no code
  was changed for these; they need a separate implementation task.

## 1. Default paths

| # | Documentation said | Implementation does | Status |
|---|--------------------|---------------------|--------|
| 1.1 | Default installation root is `%LOCALAPPDATA%\loonghao\msvc-kit\` | `MsvcKitConfig::default()` uses `ProjectDirs::data_local_dir()`, i.e. `%LOCALAPPDATA%\loonghao\msvc-kit\data` on Windows. The trailing `data` component comes from the `directories` crate | **Fixed** (README, both `getting-started`, both `installation`) |
| 1.2 | Configuration file is `%LOCALAPPDATA%\loonghao\msvc-kit\config\config.toml` | `default_config_path()` uses `ProjectDirs::config_dir()`, which on Windows is **Roaming** `%APPDATA%\loonghao\msvc-kit\config\config.toml` — a different tree from the installation root | **Fixed** (both READMEs). `docs/guide/cli-config.md` already had this right |
| 1.3 | `DownloadOptions::default().target_dir` defaults to the platform install directory (`%LOCALAPPDATA%\loonghao\msvc-kit`) | It uses `MSVC_KIT_INSTALL_DIR` and otherwise the **relative path** `msvc-kit`. It never reads the CLI configuration | **Fixed** (both `api/download-options.md`) |
| 1.4 | Cache root only described as "the platform default cache directory" | `default_cache_root()` is `ProjectDirs::cache_dir()`: `%LOCALAPPDATA%\loonghao\msvc-kit\cache` (Windows), `$XDG_CACHE_HOME/msvc-kit` (Linux), `~/Library/Caches/com.loonghao.msvc-kit` (macOS) | **Fixed** (READMEs, `guide/caching.md`, both `guide/cli-config.md`) |

Verified by running `directories::ProjectDirs::from("com", "loonghao", "msvc-kit")`
on Windows and reading `directories-6.0.0/src/{win,lin,mac}.rs`.

## 2. Flags that do not exist

| # | Documentation said | Implementation does | Status |
|---|--------------------|---------------------|--------|
| 2.1 | `msvc-kit download --host-arch <arch>` (README, both `guide/cli-download.md`, both `guide/architecture.md`, ~10 occurrences) | `Commands::Download` has no `host_arch` field. The CLI hardcodes `host_arch: Some(Architecture::host())`. Only `Commands::Bundle` has `--host-arch` | **Fixed** — replaced with `--arch`, plus a note that the host is detected and that `--host-arch` is a `bundle` flag |
| 2.2 | `setup --portable-root "%~dp0runtime"` "rewrites install root to `%~dp0runtime`" | The value is bound as `_portable_root` and **never read**: any value switches the script to `ScriptContext::portable(...)`, which anchors paths at `%~dp0` / `$PSScriptRoot` / `$SCRIPT_DIR`, not at the supplied path | **Fixed** — `ScriptContext::portable_root()` anchors the generated script's root at the supplied value; an empty value is rejected. Documented in both READMEs and `guide/cli-setup.md` |

## 3. Flags and fields that were undocumented

| # | Implementation has | Status |
|---|--------------------|--------|
| 3.1 | `download --include-component <COMPONENT>` (repeatable; `spectre`, `mfc`, `atl`, `asan`, `uwp`, `cli`, `modules`, `redist`, `custom:<pattern>`) | **Fixed** — documented in both READMEs and both `guide/cli-download.md` |
| 3.2 | `download --exclude-pattern <PATTERN>` (repeatable, case-insensitive substring) | **Fixed** — same places |
| 3.3 | `config --set-vs-channel <CHANNEL>` | **Fixed** — added to both READMEs and both `guide/cli-config.md` |
| 3.4 | `default_vs_channel` TOML field | **Fixed** — added to the TOML sample in both `guide/cli-config.md` |
| 3.5 | Global `--config <PATH>` and `--verbose` | **Fixed** — documented in both READMEs |
| 3.6 | `DownloadOptions::{vs_channel, manifest_cache_dir, include_components, exclude_patterns}` | **Fixed** — added to both `api/download-options.md` |
| 3.7 | `AvailableVersions::channel` | **Fixed** — added to `api/library.md` and `zh/api/library.md` |
| 3.8 | `install-into-vs` subcommand | **Fixed** in README_zh (English README already had it) |
| 3.9 | `MSVC_KIT_CONFIG`, `MSVC_KIT_PORTABLE`, `MSVC_KIT_VS_CHANNEL`, `MSVC_KIT_DIR`, `MSVC_KIT_INNER_PROGRESS` | **Fixed** — consolidated into an environment variable table in both READMEs |
| 3.10 | `MSVC_KIT_INSTALL_DIR`, `MSVC_KIT_MSVC_VERSION`, `MSVC_KIT_SDK_VERSION`, `MSVC_KIT_PARALLEL_DOWNLOADS`, `MSVC_KIT_VERIFY_HASHES`, `MSVC_KIT_DRY_RUN`, `MSVC_KIT_INCLUDE_COMPONENTS`, `MSVC_KIT_EXCLUDE_PATTERNS` | **Fixed** — documented in the README tables and both `api/download-options.md` with their exact scope |

### 3.10 scope: library only, not the CLI

`DownloadOptions::default()` reads these eight variables, and
`DownloadOptionsBuilder::default()` starts from it, so library and builder users
get them. The CLI builds its `DownloadOptions` literally from
`target` / `config.install_dir` / flags and reads none of them, so
`MSVC_KIT_PARALLEL_DOWNLOADS=8 msvc-kit download` has no effect.
`docs/guide/performance.md` listed two of them without that caveat; the caveat is
now stated there, in `guide/caching.md` and in the READMEs.

## 4. Claims about the code that were wrong

| # | Documentation said | Implementation does | Status |
|---|--------------------|---------------------|--------|
| 4.1 | `MsvcKitError::NetworkError(e)` | The variant is `Network(#[from] reqwest::Error)` | **Fixed** (both `api/library.md`) |
| 4.2 | The `self-update` feature "includes the `self_update` crate which depends on `lzma-sys`" | The feature is `self-update = ["dep:axoupdater"]`. There is no `self_update` crate and no `lzma-sys` in the tree; `zip` is pulled with `default-features = false, features = ["deflate"]`. The stale `lzma` conflict snippet was removed | **Fixed** (both `api/library.md`) |
| 4.3 | Download index is an "SQLite database" | It is [redb](https://github.com/cberner/redb) (`src/downloader/index.rs`, `redb = "4"`) | **Fixed** (both `guide/performance.md`). `guide/caching.md` already said redb — the two pages contradicted each other |
| 4.4 | "Configuration functions use file locking for concurrent access" | `load_config`/`save_config` do a plain read/write of one TOML file. The only lock is an in-process `RwLock` around the `--config` override | **Fixed** (both `api/library.md`) |
| 4.5 | `msvc-kit = "0.1"` in `Cargo.toml` samples | Current version is 0.2.17; `"0.1"` means `^0.1` and cannot resolve to 0.2.x | **Fixed** (both `api/library.md`, `examples/quick-compile.md`) |
| 4.6 | `msvc-kit --version` prints `msvc-kit 0.1.x` | Prints the real version, currently 0.2.x | **Fixed** (both `guide/installation.md`) |
| 4.7 | Exit-code page quotes `std::process::exit(0)` and "lines 228-235" | The code returns `Ok(())` from `main` (no `process::exit`), at a different location. Behaviour (exit 0) is unchanged | **Fixed** (`docs/exit-code-behavior.md`) |
| 4.8 | Action input `host-arch` "empty = auto-detect" | The action maps an empty value to `x64` when building tool paths (`action.yml:188`, `:263`) | **Fixed** (both `guide/github-action.md`) |
| 4.9 | Action input `install-dir` default `$RUNNER_TEMP/msvc-kit` | `action.yml:29` default is `""`; the step resolves it to `$RUNNER_TEMP/msvc-kit` at run time (`action.yml:92-95`) | **Fixed** — documented as "empty = `$RUNNER_TEMP/msvc-kit`" in both `guide/github-action.md` |
| 4.10 | `list --available` prints a full version list | It prints `Latest MSVC version:` / `Latest Windows SDK version:` only | **Fixed** (`guide/cli-list-clean.md`) |
| 4.11 | `list` output shows `14.44.34823 (C:\...)` under "Installed MSVC versions" | Real output is `MSVC Compiler:` / `Windows SDK:` with `  - <version>` lines | **Fixed** (`guide/cli-list-clean.md`) |
| 4.12 | "Not yet implemented - check files manually" (dry run) | Accurate — reworded as a plain statement, no promise of a flag | **Fixed** |
| 4.13 | `clean --cache` removes `downloads/` | Removes `<install dir>/downloads`, which is created by the downloader; correct, but the doc did not say it is inside the install dir | **Fixed** — paths now written as `<install dir>/downloads` |

## 5. Documentation site structure

| # | Problem | Status |
|---|---------|--------|
| 5.1 | The `zh` nav linked `/zh/examples/basic` and four `/zh/dcc/*` pages; `docs/zh/examples/` and `docs/zh/dcc/` do not exist | **Fixed** — those entries now point at the English pages and are labelled `（英文）` |
| 5.2 | The `zh` sidebar linked `/zh/guide/{what-is-msvc-kit,cli-setup,cli-list-clean,caching,ci-cd}` — none of those files exist | **Fixed** — same treatment |
| 5.3 | `docs/exit-code-behavior.md` was in no sidebar and no nav, so it was unreachable from the site | **Fixed** — added to the English "Advanced" sidebar (and to the zh sidebar as an English link) |
| 5.4 | The English `DCC Integration` nav lists 4 pages while the sidebar lists 6 (Blender, overview missing from nav) | **Open** — nav/sidebar inconsistency only; no incorrect statement |

## 6. Remaining gaps (not addressed here)

- `docs/examples/*`, `docs/dcc/*`, `docs/guide/{what-is-msvc-kit,cli-setup,cli-list-clean,caching,ci-cd}`
  and `docs/exit-code-behavior.md` have no Chinese translation. The zh navigation
  now links them as English pages instead of 404-ing.
- `README.md` and `README_zh.md` still drift in small ways (for example the
  English README lists the Visual Studio channel feature and the zh one now does
  too, but the two files are maintained by hand).

## 7. Already fixed in stage 1 (documentation already matched)

Listed so the stage 1 changes can be re-verified against the docs:

- `MsvcKitConfig.cache_dir` is honoured: `effective_cache_dir()` prefers an
  explicit `cache_dir`, then `<install_dir>/cache` when the install dir moved,
  then the platform cache root. `docs/guide/cli-config.md` describes exactly this
  precedence, and `msvc-kit config` prints the resolved directory.
- `MSVC_KIT_DIR` relocates the installation directory and the default cache, is
  not persisted by `config --set-*`, and does not move the configuration file.
  Documented in `docs/guide/cli-config.md`.
- Visual Studio 2026 (major 18) is in `VS_CHANNELS`, auto selection walks the
  table newest-first, unpublished channels are skipped with a 10-minute memory,
  and transport failures surface as `TransientHttp`. Documented in
  `docs/guide/vs-versions.md`.
- Portable mode (`msvc-kit.portable` marker, `MSVC_KIT_PORTABLE`, `--portable`,
  `--no-portable`, `MSVC_KIT_CONFIG`, `--config`) is implemented as described in
  `docs/guide/cli-config.md`, including the rule that `--config` cannot be
  combined with `--portable` / `--no-portable`.
- Manifest caching writes `channel-v18.json` under `<cache dir>/manifests` with
  ETag/Last-Modified metadata, as `docs/guide/caching.md` describes.
