# Changelog

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
