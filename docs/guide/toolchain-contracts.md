# Selected toolchains and diagnostics

msvc-kit manages user-mode MSVC and Windows SDK toolchains. Driver development with WDK/EWDK has additional version, Visual Studio extension, signing and testing requirements; it is not installed by these commands. Performance Toolkit, Debugging Tools and signing tools can be future optional capabilities rather than default dependencies of DCC plugins.

## One selection across commands

`setup`, `env`, `query`, `doctor`, `run`, `lock` and `cmake` use the same installed-version resolver. CLI selectors override configured versions. Omitted versions select the latest installed numeric version. A dotted prefix matches whole components (`14.4` does not match `14.40`); a requested version that is absent fails instead of selecting another version. SDK build shorthand such as `26100` remains supported.

`--host-arch` selects executables; `--arch` selects generated binaries and libraries. For example, x64 to arm64 uses MSVC `Hostx64/arm64`, SDK `bin/<version>/x64`, and arm64 libraries.

```powershell
msvc-kit query --dir C:/tools/msvc --msvc-version 14.44 --sdk-version 10.0.26100.0 --arch arm64 --host-arch x64 --format json
msvc-kit doctor --dir C:/tools/msvc --arch arm64 --host-arch x64 --compile --format json
```

`query --format json` returns `env_vars`, `tools`, selected component versions, `arch`, `host_arch` and `fingerprint`. `env --format json` keeps its flat variable map for existing consumers. SDK-only queries export SDK/UCRT variables and host tools without compiler metadata. MSVC-only queries do not imply an SDK.

The fingerprint hashes versions and architectures, independent of installation path. It identifies a selection and does not prove binary integrity.

## Readiness checks

`doctor` reports schema `msvc-kit.doctor.v1`, `status`, individual `checks` and the selected `toolchain`. Read-only checks verify required compiler tools, SDK tools, headers and libraries. `--compile` additionally compiles C++, creates resources, links, embeds a manifest and runs the result when host and target are native. Each subprocess has a 30-second deadline. Cross-target execution is reported as skipped. A failed diagnostic exits with code **10**; ordinary command errors exit nonzero.

## Child processes and CMake

```powershell
msvc-kit run --dir C:/tools/msvc --msvc-version 14.44 -- cmd /d /c build.cmd
msvc-kit cmake --dir C:/tools/msvc --output build/msvc-toolchain.cmake
msvc-kit run --dir C:/tools/msvc -- cmake -S . -B build -G Ninja --toolchain build/msvc-toolchain.cmake
msvc-kit run --dir C:/tools/msvc -- cmake --build build
```

`run` prepends selected tools to inherited PATH, sets the selected INCLUDE/LIB and target-specific compiler/linker variables, and preserves the child's exit code. It does not change the parent environment. Generate the CMake file with the same selectors used by `run`. The generated file is intended for Ninja; Visual Studio/MSBuild generators still require a registered compatible VS installation. Consumers choose their CRT and host ABI; the generic toolchain file does not impose them.

## Locks and acquisition receipts

```powershell
msvc-kit lock --dir C:/tools/msvc --output msvc-kit.lock.json
msvc-kit doctor --dir C:/tools/msvc --lockfile msvc-kit.lock.json --compile
msvc-kit download --target C:/tools/msvc --lockfile msvc-kit.lock.json
```

Schema `msvc-kit.toolchain-lock.v1` records exact installed versions, host/target and the metadata fingerprint. CLI downloads additionally write installation receipts with source URLs, archive sizes and actual SHA256 values after successful extraction. A lock captures available receipts; on later download their complete payload set and hashes are checked before extraction. Existing external installations may produce a **selection-only lock** with no download receipts. This is explicit in the `receipts` field and does not claim pinned source artifacts.

Microsoft's channel manifests can remove or replace older packages. Lock acquisition then fails; a lock cannot make unavailable archives downloadable. Cached source files are rehashed when verification is enabled, and completed downloads publish atomically. A matching official SHA256 takes precedence over stale declared sizes; missing official package digests fail with verification enabled. Cache, manifest and MSI state use process locks. Receipts attest acquired archives, not a later unmodified extracted directory.

Channels are fetched fresh. Package catalogs normally must match the fresh channel's SHA256 before parsing or cache publication. A Windows-only [signed-manifest fallback](./caching#windows-signed-manifest-fallback) verifies both original signatures, current Windows code-signing trust and revocation, the Microsoft publisher and pinned Microsoft Root CA 2011, then binds the catalog's release identity to that channel. These checks repeat before cached fallback bytes can be reused after a 304. Other platforms reject a digest mismatch, and individual package SHA256 checks remain unchanged.

## Consumer responsibilities

| Consumer | Required boundary |
| --- | --- |
| py-dem-bones | Explicit dynamic CRT, explicit OpenMP choice, repaired wheel numerical test in a fresh environment |
| Maya plugin | DevKit year matches requested Maya, Cargo/CMake share selected environment, packaged DLLs, isolated mayapy load test |
| Unreal | Engine-owned compiler compatibility, registered VS discovery, actual UBT compiler and SDK log readback |
| Unity | Python/C# editor bridge needs no MSVC by default; Windows IL2CPP and native standalone packaging validate only when used |

Compilation and package tests do not certify an active editor GUI or a released installation. Each consumer remains responsible for its own host acceptance.


## Existing VS2015 and early Windows 10 SDK layouts

Read-only selection can use the original VS2015 installation root and a separate
Windows Kits/10 root. No copy, registry update, symbolic link or modern-layout
alias is required:

```powershell
msvc-kit doctor --dir 'C:/Program Files (x86)/Microsoft Visual Studio 14.0' --sdk-dir 'C:/Program Files (x86)/Windows Kits/10' --msvc-version 14.0 --sdk-version 10.0.14393.0 --host-arch x64 --arch x64 --format json
```

Use the same selectors for `env --format json`, `query`, `run` and `cmake`.
`--sdk-dir` is the directory containing `Include`, `Lib` and `bin`, not a version
subdirectory. When an SDK is selected, explicit nonexistent or empty SDK roots fail; no ambient environment
variable, registry entry or different root is substituted. This option does not
install or download VS2015 and is not an automatic system-installation search.
When capturing a modern toolchain lock with a separate SDK root, SDK receipts
from the portable compiler root are not reused as provenance for that SDK.

VS2015 detection requires `VC/vcvarsall.bat` and `VC/include/crtversion.h`
identifying major 14, minor 0. Because it has no versioned compiler directory,
the selected `msvc.version` and `VCToolsVersion` are **`14.0` (v140 family)**.
CRT build metadata is not used to claim an exact compiler patch version.
`14.0.x` selectors fail instead of widening or selecting a newer compiler.
Modern versioned toolsets retain their existing full version/prefix rules.

The machine-readable doctor check `version.msvc` has status `skipped` and explains
that the exact legacy compiler patch is unverified. Other required filesystem
checks must still pass for overall status `passed`. A `resolution` failure reports
unsupported legacy host/target combinations or the precise missing compiler path.
The existing fingerprint identifies the selected family, SDK, host and target;
it does not attest a legacy compiler patch or binary integrity. Exact-version
`lock` files therefore remain unavailable for family-only VS2015 selections.

Supported legacy host/target pairs are x86/x86 (`VC/bin`), x86/x64
(`VC/bin/x86_amd64`), x64/x86 (`VC/bin/amd64_x86`) and x64/x64 (`VC/bin/amd64`).
Target libraries come from `VC/lib` for x86 and `VC/lib/amd64` for x64.
The selected directory must contain `cl.exe`, `link.exe` and `lib.exe`.
Cross environments also put the native host directory on PATH for compiler DLLs;
compiler lookup never substitutes its different-target tools. Other legacy
architectures fail explicitly.

SDK versions `10.0.10240.0`, `10.0.10586.0` and `10.0.14393.0` may use unversioned
`bin/<host>` directories when their versioned bin directory is absent. A versioned
bin directory takes priority even if incomplete. Modern SDKs never fall back to
unversioned tools. All SDK include and library paths remain version-selected.
Unversioned SDK tool paths do not prove the executables' exact binary versions.
Doctor reports this as `version.sdk_tools` with status `skipped`.

Portable activation templates cannot represent these layouts or a separate SDK
root, so `setup --script` rejects them with guidance to use `env` or `run`.
Directory-fixture tests cover discovery, host/target mapping, selection failures,
JSON contracts and diagnostics. They are not evidence that a real VS2015
installation has compiled a program; that requires an explicit Windows
`doctor --compile` probe on the actual installation.

For Rust callers, existing `ToolchainRequest`, `QueryOptions`, `DoctorOptions` and
entrypoints remain source-compatible, including complete struct literals.
The additive `resolve_toolchain_with_sdk_root`, `query_installation_with_sdk_root`
and `doctor_with_sdk_root` functions take an optional `&Path` after the original
request/options argument; `None` retains portable-root selection.
`ToolchainLock::capture_with_sdk_root` follows the same convention.
