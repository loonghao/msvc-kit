# Changelog

## [0.2.9](https://github.com/loonghao/msvc-kit/compare/v0.2.8...v0.2.9) (2026-02-13)


### Dependencies

* update GitHub Artifact Actions ([54d56c3](https://github.com/loonghao/msvc-kit/commit/54d56c35f4e6641df6b797b6002c114c18475cd7))

## [0.2.8](https://github.com/loonghao/msvc-kit/compare/v0.2.7...v0.2.8) (2026-02-13)


### Bug Fixes

* resolve winget duplicate installer entry validation error ([b200632](https://github.com/loonghao/msvc-kit/commit/b20063224cf7c3e3dd87c46f23710c8a3321e058))


### Dependencies

* update Rust crate mockito to v1.7.2 ([62f859c](https://github.com/loonghao/msvc-kit/commit/62f859cd0750b3f62f3cd3bcd505c8fc81c8e90d))
* update Rust crate toml to v1 ([5f00c4a](https://github.com/loonghao/msvc-kit/commit/5f00c4a556e70bc81a6d01a07c857cd8c90d4809))


### Miscellaneous

* **deps:** bump bytes from 1.11.0 to 1.11.1 ([2a807b5](https://github.com/loonghao/msvc-kit/commit/2a807b5f8367e9eb9974f6772ab572abafc68df4))

## [0.2.7](https://github.com/loonghao/msvc-kit/compare/v0.2.6...v0.2.7) (2026-02-13)


### Features

* add GitHub Action, enhance library API with cache_manager support ([#70](https://github.com/loonghao/msvc-kit/issues/70), [#573](https://github.com/loonghao/msvc-kit/issues/573)) ([b132277](https://github.com/loonghao/msvc-kit/commit/b132277011c78a4c8729c2c2c88f42e8d6930b8a))

## [0.2.6](https://github.com/loonghao/msvc-kit/compare/v0.2.5...v0.2.6) (2026-02-13)


### Features

* add TLS backend feature flags to avoid cmake/NASM dependency on Windows ([#44](https://github.com/loonghao/msvc-kit/issues/44)) ([ee8c05f](https://github.com/loonghao/msvc-kit/commit/ee8c05f4c14f252052da06937bcd06cc0c67b62f))
* migrate self-update from self_update to axoupdater 0.9 ([3fcb74c](https://github.com/loonghao/msvc-kit/commit/3fcb74cf80365bc3861d9d7638c2a8a17708f1e0))


### Dependencies

* update Rust crate clap to v4.5.56 ([9895d6a](https://github.com/loonghao/msvc-kit/commit/9895d6a27340b81262c936161181a418bdc6c50c))

## [0.2.5](https://github.com/loonghao/msvc-kit/compare/v0.2.4...v0.2.5) (2026-01-17)


### Bug Fixes

* enable chore commits to trigger releases ([937c6b5](https://github.com/loonghao/msvc-kit/commit/937c6b5eb8def7cb0cbba05e603729ac6661a864))


### Miscellaneous

* **deps:** update rust crate chrono to v0.4.43 ([92019db](https://github.com/loonghao/msvc-kit/commit/92019dbcf4d5f50785d5c487f762d6b7ed6227f6))
* **deps:** upgrade dependencies ([66f739a](https://github.com/loonghao/msvc-kit/commit/66f739a4291c40513e554076c7df59b09b105024))
* **deps:** upgrade dependencies ([ad4ed70](https://github.com/loonghao/msvc-kit/commit/ad4ed70612219478c370049071cd2a5107edc20b))

## [0.2.4](https://github.com/loonghao/msvc-kit/compare/v0.2.3...v0.2.4) (2026-01-11)


### Bug Fixes

* **ci:** add release-tag parameter to winget-releaser to fix 404 error ([3d23c2a](https://github.com/loonghao/msvc-kit/commit/3d23c2ab0b8166fad9c6f4236c211c132f9cbf17))

## [0.2.3](https://github.com/loonghao/msvc-kit/compare/v0.2.2...v0.2.3) (2026-01-11)


### Bug Fixes

* **msi:** prevent concurrent msiexec invocations causing error 1618 ([d6aea13](https://github.com/loonghao/msvc-kit/commit/d6aea13c4f0455fe52b26608ed08a8c068cd8010))
* remove dead links and fix clippy warnings ([76cc8c7](https://github.com/loonghao/msvc-kit/commit/76cc8c7234b3e47731edc535eba76d0f5f804ebd))
* resolve lint warnings in http.rs ([54a5239](https://github.com/loonghao/msvc-kit/commit/54a523980117ee5e2fc2e98262b09baadfc82365))
* update test expectations for optimized buffer sizes and fix formatting ([dfa8989](https://github.com/loonghao/msvc-kit/commit/dfa8989036b76d047a128095769c61084b3f159d))
* use native-tls backend for reqwest ([242e454](https://github.com/loonghao/msvc-kit/commit/242e454ca5cd416eab47ecb9ea8526e51f340132))


### Performance Improvements

* add comprehensive performance optimizations for download and extraction ([e0787b4](https://github.com/loonghao/msvc-kit/commit/e0787b4d97352adfc789f55cc1845c3d5ccf85a0))

## [Unreleased]

### Performance

* **parallel-download**: MSVC and SDK packages now download simultaneously using `tokio::join!`, reducing total download time by 30-50%
* **parallel-extraction**: Package extraction now uses multi-threaded processing with `buffer_unordered`, improving extraction speed 2-4x
* **streaming-hash**: SHA256 hash computation now happens during download, eliminating a second file read operation
* **connection-pooling**: HTTP client now uses connection pooling with `pool_max_idle_per_host(10)` for better connection reuse
* **optimized-buffers**: Increased hash buffer from 1MB to 4MB and extraction buffer from 128KB to 256KB for better throughput
* **rwlock-index**: Replaced `Mutex` with `RwLock` for download index to reduce lock contention during parallel downloads

### Bug Fixes

* **msi-extraction**: Add global mutex lock to prevent concurrent `msiexec` invocations (error 1618)
* **msi-extraction**: Add retry mechanism with exponential backoff for handling system-level installer conflicts

### Documentation

* Add performance optimization guide (English and Chinese)

## [0.2.2](https://github.com/loonghao/msvc-kit/compare/v0.2.1...v0.2.2) (2026-01-05)


### Bug Fixes

* add missing CommandFactory import and update deploy-pages action version ([7584634](https://github.com/loonghao/msvc-kit/commit/7584634d414b34434bc13e8769a1b7ad53d34b7d))

## [0.2.1](https://github.com/loonghao/msvc-kit/compare/v0.2.0...v0.2.1) (2026-01-04)


### Bug Fixes

* architecture filtering for MSVC and SDK packages ([1f23da7](https://github.com/loonghao/msvc-kit/commit/1f23da73e7f7c92295b42cceafecde875805c573))
* update deploy-pages action from v5 to v6 ([0de2a0f](https://github.com/loonghao/msvc-kit/commit/0de2a0f62341b2c8277e8ac76cd686fe3e3fe256))


### Documentation

* add SDK tools documentation (signtool, rc, mt, etc.) ([5af4398](https://github.com/loonghao/msvc-kit/commit/5af4398078b29df2000a38b164ed5eecb4559ead))

## [0.2.0](https://github.com/loonghao/msvc-kit/compare/v0.1.4...v0.2.0) (2026-01-04)


### ⚠ BREAKING CHANGES

* Pre-built MSVC bundles are no longer distributed via GitHub Releases. Users must create bundles locally with 'msvc-kit bundle --accept-license'.

### Features

* add bundle command for portable MSVC toolchain creation ([f27c381](https://github.com/loonghao/msvc-kit/commit/f27c3813be71726bc7a2d6db815fd825083bbd66))
* add bundle release CI and list_available_versions API ([6884f11](https://github.com/loonghao/msvc-kit/commit/6884f119715b8a2ce90f586ca5563af65d425b2b))


### Bug Fixes

* export generate_activation_script_with_vars and fix Setup pattern matching ([f2aad74](https://github.com/loonghao/msvc-kit/commit/f2aad74a34b747aa1496a770f8a64e3f406d6906))
* remove broken has_existing_content logic that skips extraction ([9f9d5eb](https://github.com/loonghao/msvc-kit/commit/9f9d5eba44b4bb7f4725707e40bb50449d805fbe))

## [0.1.4](https://github.com/loonghao/msvc-kit/compare/v0.1.3...v0.1.4) (2026-01-03)


### Bug Fixes

* replace unreliable error test with HashMismatch test ([17af874](https://github.com/loonghao/msvc-kit/commit/17af87447e1cd4f46842f700359bad7357b42643))

## [0.1.3](https://github.com/loonghao/msvc-kit/compare/v0.1.2...v0.1.3) (2026-01-03)


### Features

* make self_update dependency optional to avoid lzma-sys conflict ([00cf2b9](https://github.com/loonghao/msvc-kit/commit/00cf2b98b2ea77c39b0fb3d0d0a6d6e16f5aa5c8))


### Bug Fixes

* **docs:** add type module to package.json for ESM support ([fcc28f5](https://github.com/loonghao/msvc-kit/commit/fcc28f5e20b5e12f818fae7e2ef4ffccbd6067f4))
* **tests:** use struct initialization instead of default() for ToolPaths ([0d4629d](https://github.com/loonghao/msvc-kit/commit/0d4629d1c98b11071da130393727c84248d69d35))


### Documentation

* add missing Chinese documentation files ([a3f217a](https://github.com/loonghao/msvc-kit/commit/a3f217a1a4c212d944ad34dcd20e106221397b3a))
* add winget and PowerShell script installation methods ([a63e9f6](https://github.com/loonghao/msvc-kit/commit/a63e9f629a23b1a7f83ad9a05b7e329ad7a2f704))

## [0.1.2](https://github.com/loonghao/msvc-kit/compare/v0.1.1...v0.1.2) (2026-01-03)


### Features

* add self-update, coverage CI, fix crates.io publish ([c318f06](https://github.com/loonghao/msvc-kit/commit/c318f06e4a3247ba8962c1a5db616ca9d9d091ad))


### Bug Fixes

* **deps:** update rust crate indicatif to 0.18 ([a3ce8c5](https://github.com/loonghao/msvc-kit/commit/a3ce8c58e8e1be3ddf9380fa9e10ed706afbec29))
* **deps:** update rust crate reqwest to 0.13 ([b479007](https://github.com/loonghao/msvc-kit/commit/b47900703e785c4ff1a2c0e771b05d1a3ae50afc))
* **deps:** update rust crate self_update to 0.42 ([c4f75a5](https://github.com/loonghao/msvc-kit/commit/c4f75a576a84b76c50196c4ae53c4b0152b61aa7))
* **deps:** update rust crate simd-json to 0.17 ([bb77b37](https://github.com/loonghao/msvc-kit/commit/bb77b37117df5ebec43a89efeec4cb041940d94f))
* **deps:** update rust crate toml to 0.9 ([0036fa8](https://github.com/loonghao/msvc-kit/commit/0036fa8c408f3783b677b98e1b01dc272126da33))
* use Release.version field instead of method ([0025c53](https://github.com/loonghao/msvc-kit/commit/0025c53af757162cdbd4340999d09206d00c0731))

## [0.1.1](https://github.com/loonghao/msvc-kit/compare/v0.1.0...v0.1.1) (2026-01-03)


### Features

* add path access API and VitePress documentation site ([7833681](https://github.com/loonghao/msvc-kit/commit/7833681948708f949bf0ea084a925a60f6c98084))


### Bug Fixes

* update tests to use new builder pattern API ([0c16761](https://github.com/loonghao/msvc-kit/commit/0c1676174686157924c19b81f90d741b851cc47d))
* upgrade to rust-actions-toolkit@v4 ([84c0812](https://github.com/loonghao/msvc-kit/commit/84c0812596bb69a4449eb3f2b267f4c0d86bd6ff))
* use rust-actions-toolkit for CI workflows ([b1cda87](https://github.com/loonghao/msvc-kit/commit/b1cda87481cfba9c0972bc59c9fda7a434e61c44))


### Documentation

* move architecture to .codebuddy/rules, update config format to TOML ([ba2e7d3](https://github.com/loonghao/msvc-kit/commit/ba2e7d37fd8e6ea1401d8ecd4f9f75ee7fd8d5c3))
* split Chinese README to README_zh.md and add comprehensive tests ([9a3a554](https://github.com/loonghao/msvc-kit/commit/9a3a554500229a65727ddd32a1eccc8e94e08ad5))
* update API examples to use builder pattern ([c89636c](https://github.com/loonghao/msvc-kit/commit/c89636c36466db2b2e96315e8f89b04c0441781d))
* update README with comprehensive CLI options and API documentation ([389c017](https://github.com/loonghao/msvc-kit/commit/389c01732291b38dcc901588350d282717a561df))
