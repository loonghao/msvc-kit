$ErrorActionPreference = 'Stop'
$exePath = $env:MSVC_KIT_EXE
$installRoot = $env:INSTALL_DIR
$nativeArch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    'X64' { 'x64' }; 'X86' { 'x86' }; 'Arm64' { 'arm64' }; default { throw 'Unsupported runner architecture' }
}
$targetArch = $env:TARGET_ARCH
$hostArch = if ($env:HOST_ARCH) { $env:HOST_ARCH } else { $nativeArch }
if ($targetArch -notin @('x64','x86','arm64') -or $hostArch -notin @('x64','x86','arm64')) { throw 'Invalid host or target architecture' }
if ($env:COMPONENTS -notin @('all','msvc','sdk')) { throw 'components must be all, msvc, or sdk' }
if ($env:VERIFY_HASHES -notin @('true','false') -or $env:EXPORT_ENV -notin @('true','false')) { throw 'Boolean inputs must be true or false' }
$downloadHelp = & $exePath download --help | Out-String
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect download capabilities' }
$queryHelp = & $exePath query --help | Out-String
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect query capabilities' }
$selectionArgs = @('--dir', $installRoot, '--arch', $targetArch)
$downloadArgs = @('download', '--target', $installRoot, '--arch', $targetArch)
if ($downloadHelp.Contains('--host-arch') -and $queryHelp.Contains('--host-arch')) {
    $selectionArgs += @('--host-arch', $hostArch)
    $downloadArgs += @('--host-arch', $hostArch)
} elseif ($hostArch -ne $targetArch -or $hostArch -ne $nativeArch -or $nativeArch -eq 'arm64') {
    throw 'This CLI cannot select host architecture; provide a newer msvc-kit-path for cross-compilation'
}
foreach ($selector in @{ '--msvc-version' = $env:MSVC_VERSION; '--sdk-version' = $env:SDK_VERSION }.GetEnumerator()) {
    if ($selector.Value) { $selectionArgs += @($selector.Key, $selector.Value); $downloadArgs += @($selector.Key, $selector.Value) }
}
if ($env:LOCKFILE) {
    if (-not $downloadHelp.Contains('--lockfile') -or -not $queryHelp.Contains('--lockfile')) { throw 'The selected CLI does not support lockfile' }
    $selectionArgs += @('--lockfile', $env:LOCKFILE)
    $downloadArgs += @('--lockfile', $env:LOCKFILE)
}
if ($env:VS_CHANNEL) { $downloadArgs += @('--vs-channel', $env:VS_CHANNEL) }
if ($env:VERIFY_HASHES -eq 'false') { $downloadArgs += '--no-verify' }
if ($env:COMPONENTS -eq 'msvc') { $downloadArgs += '--no-sdk' }
if ($env:COMPONENTS -eq 'sdk') { $downloadArgs += '--no-msvc' }
& $exePath @downloadArgs
if ($LASTEXITCODE -ne 0) { throw "Download failed ($LASTEXITCODE)" }
$queryArgs = @('query') + $selectionArgs + @('--component', $env:COMPONENTS, '--format', 'json')
$queryJson = & $exePath @queryArgs | Out-String
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve selected installation' }
$query = $queryJson | ConvertFrom-Json
if (-not $query.env_vars) { throw 'CLI did not return an environment' }
$requiredTools = switch ($env:COMPONENTS) { 'all' { @('cl','link','lib','rc','mt') }; 'msvc' { @('cl','link','lib') }; 'sdk' { @('rc','mt') } }
foreach ($tool in $requiredTools) {
    if (-not $query.tools.$tool -or -not (Test-Path -LiteralPath $query.tools.$tool -PathType Leaf)) { throw "Required tool $tool is missing" }
}
foreach ($variable in $query.env_vars.PSObject.Properties) {
    if ($variable.Value -match '[\r\n]') { throw "Invalid environment value: $($variable.Name)" }
    if ($variable.Name -eq 'PATH') {
        $env:PATH = "$($variable.Value);$env:PATH"
    } else {
        [Environment]::SetEnvironmentVariable($variable.Name, [string]$variable.Value, 'Process')
    }
}
$help = & $exePath --help | Out-String
if ($env:COMPONENTS -eq 'all' -and $help -match '(?m)^\s+doctor\s') {
    $doctorArgs = @('doctor') + $selectionArgs + @('--compile', '--format', 'json')
    $reportJson = & $exePath @doctorArgs | Out-String
    $doctorExit = $LASTEXITCODE
    Write-Host $reportJson
    $report = $reportJson | ConvertFrom-Json
    if ($doctorExit -ne 0 -or $report.schema -ne 'msvc-kit.doctor.v1' -or $report.status -ne 'passed') { throw 'Toolchain compile probe failed' }
} else {
    # Older releases have no doctor: test the selected tools directly.
    $probeRoot = Join-Path $env:RUNNER_TEMP ([guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $probeRoot | Out-Null
    Push-Location $probeRoot
    try {
        if ($env:COMPONENTS -eq 'sdk') {
            '1 RCDATA { 1, 2, 3 }' | Set-Content probe.rc
            & $query.tools.rc /nologo /fo probe.res probe.rc
        } elseif ($env:COMPONENTS -eq 'msvc') {
            'int probe() { return 0; }' | Set-Content probe.cpp
            & $query.tools.cl /nologo /c /Foprobe.obj probe.cpp
        } else {
            "#include <windows.h>`n#include <vector>`nint main() { std::vector<int> v{1}; return GetCurrentProcessId() && v[0] == 1 ? 0 : 1; }" | Set-Content probe.cpp
            & $query.tools.cl /nologo /EHsc /Feprobe.exe probe.cpp
        }
        if ($LASTEXITCODE -ne 0) { throw 'Toolchain compilation failed' }
        if ($env:COMPONENTS -eq 'all' -and $targetArch -eq $nativeArch) {
            & .\probe.exe
            if ($LASTEXITCODE -ne 0) { throw 'Toolchain executable probe failed' }
        }
    } finally {
        Pop-Location
        $resolvedProbe = [IO.Path]::GetFullPath($probeRoot)
        $resolvedTemp = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\') + '\'
        if (-not $resolvedProbe.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase)) { throw 'Probe cleanup escaped RUNNER_TEMP' }
        Remove-Item -LiteralPath $resolvedProbe -Recurse
    }
}
if ($env:EXPORT_ENV -eq 'true') {
    foreach ($variable in $query.env_vars.PSObject.Properties) {
        if ($variable.Name -eq 'PATH') {
            $variable.Value.Split(';') | Where-Object { $_ } | Add-Content -LiteralPath $env:GITHUB_PATH
        } else {
            "$($variable.Name)=$($variable.Value)" >> $env:GITHUB_ENV
        }
    }
}
$outputs = @{
    'msvc-version' = $query.msvc.version; 'sdk-version' = $query.sdk.version
    'cl-path' = $query.tools.cl; 'link-path' = $query.tools.link; 'rc-path' = $query.tools.rc
    'include-path' = $query.env_vars.INCLUDE; 'lib-path' = $query.env_vars.LIB; 'fingerprint' = $query.fingerprint
}
foreach ($entry in $outputs.GetEnumerator()) { "$($entry.Key)=$($entry.Value)" >> $env:GITHUB_OUTPUT }
Write-Host "Verified selected toolchain: host=$hostArch target=$targetArch"
