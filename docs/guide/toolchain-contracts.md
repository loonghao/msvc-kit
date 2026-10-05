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

Microsoft's channel manifests can remove or replace older packages. Lock acquisition then fails; a lock cannot make unavailable archives downloadable. Cached source files are rehashed when verification is enabled, and completed downloads publish atomically. A matching official SHA256 takes precedence over stale declared sizes; without a trusted digest size checks remain strict. Cache, manifest and MSI state use process locks. Receipts attest acquired archives, not a later unmodified extracted directory.

## Consumer responsibilities

| Consumer | Required boundary |
| --- | --- |
| py-dem-bones | Explicit dynamic CRT, explicit OpenMP choice, repaired wheel numerical test in a fresh environment |
| Maya plugin | DevKit year matches requested Maya, Cargo/CMake share selected environment, packaged DLLs, isolated mayapy load test |
| Unreal | Engine-owned compiler compatibility, registered VS discovery, actual UBT compiler and SDK log readback |
| Unity | Python/C# editor bridge needs no MSVC by default; Windows IL2CPP and native standalone packaging validate only when used |

Compilation and package tests do not certify an active editor GUI or a released installation. Each consumer remains responsible for its own host acceptance.
