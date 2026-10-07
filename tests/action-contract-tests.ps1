# Exercise the Action's process and runner-file contracts without acquiring a toolchain.
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$setupScript = Join-Path $repository 'scripts/action-setup.ps1'
$temporaryParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
$testRoot = Join-Path $temporaryParent ('msvc-kit-action-contract-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null

function Assert-Contract([bool] $condition, [string] $message) {
    if (-not $condition) { throw $message }
}

function New-Fixture([string] $name, [string] $components, [bool] $doctorPasses) {
    $root = Join-Path $testRoot $name
    New-Item -ItemType Directory -Path $root | Out-Null
    $tools = Join-Path $root 'tools'
    New-Item -ItemType Directory -Path $tools | Out-Null
    $toolScript = Join-Path $tools 'mock-tool.ps1'
    '$global:LASTEXITCODE = 0' | Set-Content -LiteralPath $toolScript -Encoding utf8
    $target = if ($components -eq 'sdk') { 'arm64' } else { 'x64' }
    $variables = @{
        PATH = $tools
        INCLUDE = (Join-Path $root 'include')
        LIB = (Join-Path $root "lib/$target")
        VSCMD_ARG_HOST_ARCH = 'x64'
        VSCMD_ARG_TGT_ARCH = $target
        MSVC_KIT_TEST_FORWARD = 'from-query'
    }
    $query = @{
        arch = $target; host_arch = 'x64'; fingerprint = ('a' * 64)
        env_vars = $variables
        tools = @{ rc = $toolScript; mt = $toolScript }
    }
    if ($components -ne 'sdk') {
        $query.msvc = @{ version = '14.44.35207' }
        $query.tools.cl = $toolScript
        $query.tools.link = $toolScript
        $query.tools.lib = $toolScript
        $variables.CC = $toolScript
        $variables.CXX = $toolScript
        $variables.VCToolsVersion = '14.44.35207'
    }
    $query.sdk = @{ version = '10.0.26100.0' }
    $variables.WindowsSDKVersion = '10.0.26100.0\'
    $variables.UCRTVersion = '10.0.26100.0'
    @{ query = $query; doctor_passes = $doctorPasses } |
        ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $root 'state.json') -Encoding utf8
    $fakeCli = Join-Path $root 'mock-msvc-kit.ps1'
    @'
$state = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'state.json') | ConvertFrom-Json
ConvertTo-Json -InputObject @($args) -Compress | Add-Content -LiteralPath (Join-Path $PSScriptRoot 'commands.jsonl')
$global:LASTEXITCODE = 0
if ($args -contains '--help') {
    if ($args[0] -eq 'download') { '--host-arch --lockfile'; return }
    if ($args[0] -eq 'query') { '--host-arch --lockfile'; return }
    '  doctor  Inspect toolchain'; return
}
switch ($args[0]) {
    'download' { return }
    'query' { $state.query | ConvertTo-Json -Depth 8; return }
    'doctor' {
        $status = if ($state.doctor_passes) { 'passed' } else { 'failed' }
        @{ schema = 'msvc-kit.doctor.v1'; status = $status; checks = @() } | ConvertTo-Json -Compress
        if (-not $state.doctor_passes) { $global:LASTEXITCODE = 10 }
        return
    }
    default { throw "Unexpected fake CLI invocation: $args" }
}
'@ | Set-Content -LiteralPath $fakeCli -Encoding utf8
    $environmentFile = Join-Path $root 'github-env'
    $pathFile = Join-Path $root 'github-path'
    $outputFile = Join-Path $root 'github-output'
    'BASELINE=unchanged' | Set-Content -LiteralPath $environmentFile -Encoding utf8
    'baseline-path' | Set-Content -LiteralPath $pathFile -Encoding utf8
    New-Item -ItemType File -Path $outputFile | Out-Null
    return @{
        Root = $root; Cli = $fakeCli; Target = $target; Components = $components
        EnvironmentFile = $environmentFile; PathFile = $pathFile; OutputFile = $outputFile
        EnvironmentBefore = [IO.File]::ReadAllText($environmentFile)
        PathBefore = [IO.File]::ReadAllText($pathFile)
        Tools = $tools; Tool = $toolScript
    }
}

function Invoke-Fixture($fixture) {
    $startInfo = [Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'pwsh.exe'))
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in @('-NoProfile', '-File', $setupScript)) { $startInfo.ArgumentList.Add($argument) }
    $variables = @{
        MSVC_KIT_EXE = $fixture.Cli
        INSTALL_DIR = (Join-Path $fixture.Root 'installation')
        RUNNER_TEMP = $fixture.Root
        TARGET_ARCH = $fixture.Target
        HOST_ARCH = 'x64'
        COMPONENTS = $fixture.Components
        VERIFY_HASHES = 'true'; EXPORT_ENV = 'true'
        MSVC_VERSION = ''; SDK_VERSION = ''; LOCKFILE = ''; VS_CHANNEL = ''
        GITHUB_ENV = $fixture.EnvironmentFile
        GITHUB_PATH = $fixture.PathFile
        GITHUB_OUTPUT = $fixture.OutputFile
    }
    foreach ($entry in $variables.GetEnumerator()) { $startInfo.Environment[$entry.Key] = [string] $entry.Value }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        Assert-Contract ($process.Start()) 'Failed to start isolated Action process'
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) { $process.Kill($true); throw 'Action contract fixture timed out' }
        return @{ ExitCode = $process.ExitCode; Stdout = $stdout.GetAwaiter().GetResult(); Stderr = $stderr.GetAwaiter().GetResult() }
    } finally { $process.Dispose() }
}

try {
    $failed = New-Fixture 'failed-doctor' 'all' $false
    $failedResult = Invoke-Fixture $failed
    Assert-Contract ($failedResult.ExitCode -ne 0) 'Failed doctor must fail the Action'
    Assert-Contract ([IO.File]::ReadAllText($failed.EnvironmentFile) -eq $failed.EnvironmentBefore) 'Failed doctor wrote GITHUB_ENV'
    Assert-Contract ([IO.File]::ReadAllText($failed.PathFile) -eq $failed.PathBefore) 'Failed doctor wrote GITHUB_PATH'
    Write-Output 'PASS: failed doctor leaves runner environment files untouched'

    $selected = New-Fixture 'query-forwarding' 'all' $true
    $selectedResult = Invoke-Fixture $selected
    Assert-Contract ($selectedResult.ExitCode -eq 0) "Selected Action failed: $($selectedResult.Stderr)"
    $selectedEnvironment = [IO.File]::ReadAllText($selected.EnvironmentFile)
    Assert-Contract ($selectedEnvironment.Contains('MSVC_KIT_TEST_FORWARD=from-query')) 'Action did not forward query environment'
    Assert-Contract ($selectedEnvironment.Contains("CC=$($selected.Tool)")) 'Action did not forward selected compiler'
    Assert-Contract ([IO.File]::ReadAllText($selected.PathFile).Contains($selected.Tools)) 'Action did not forward query PATH additions'
    $selectedOutput = [IO.File]::ReadAllText($selected.OutputFile)
    Assert-Contract ($selectedOutput.Contains('msvc-version=14.44.35207')) 'Action lost resolved MSVC version'
    Assert-Contract ($selectedOutput.Contains('fingerprint=' + ('a' * 64))) 'Action lost query fingerprint'
    Write-Output 'PASS: successful Action forwards query versions, environment and tools'

    $sdk = New-Fixture 'sdk-only' 'sdk' $true
    $sdkResult = Invoke-Fixture $sdk
    Assert-Contract ($sdkResult.ExitCode -eq 0) "SDK-only Action failed: $($sdkResult.Stderr)"
    $sdkOutput = [IO.File]::ReadAllText($sdk.OutputFile)
    Assert-Contract ($sdkOutput -match '(?m)^msvc-version=\r?$') 'SDK-only Action invented an MSVC version'
    Assert-Contract ($sdkOutput -match '(?m)^cl-path=\r?$') 'SDK-only Action invented a compiler path'
    Assert-Contract ($sdkOutput -match '(?m)^link-path=\r?$') 'SDK-only Action invented a linker path'
    Assert-Contract ($sdkOutput.Contains('sdk-version=10.0.26100.0')) 'SDK-only Action lost SDK version'
    $sdkEnvironment = [IO.File]::ReadAllText($sdk.EnvironmentFile)
    Assert-Contract ($sdkEnvironment.Contains('VSCMD_ARG_HOST_ARCH=x64')) 'SDK-only Action lost host architecture'
    Assert-Contract ($sdkEnvironment.Contains('VSCMD_ARG_TGT_ARCH=arm64')) 'SDK-only Action lost target architecture'
    Assert-Contract ($sdkEnvironment -notmatch '(?m)^CC=') 'SDK-only Action invented CC'
    Write-Output 'PASS: SDK-only Action keeps compiler outputs empty and forwards host/target metadata'
} finally {
    $resolvedRoot = [IO.Path]::GetFullPath($testRoot)
    if (-not $resolvedRoot.StartsWith($temporaryParent, [StringComparison]::OrdinalIgnoreCase) -or
        [IO.Path]::GetFileName($resolvedRoot) -notlike 'msvc-kit-action-contract-*') {
        throw 'Action contract cleanup escaped the allocated temporary root'
    }
    Remove-Item -LiteralPath $resolvedRoot -Recurse -Force
}
