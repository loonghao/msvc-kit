$ErrorActionPreference = 'Stop'
if ($env:RUNNER_OS -ne 'Windows') { throw 'msvc-kit requires a Windows runner' }
$installRoot = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $env:RUNNER_TEMP 'msvc-kit' }
New-Item -ItemType Directory -Path $installRoot -Force | Out-Null
$installRoot = (Resolve-Path -LiteralPath $installRoot).Path
if ($env:MSVC_KIT_PATH) {
    $exePath = (Resolve-Path -LiteralPath $env:MSVC_KIT_PATH).Path
    if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) { throw 'msvc-kit-path must identify an executable' }
} else {
    $headers = @{ 'User-Agent' = 'msvc-kit-action'; Accept = 'application/vnd.github+json' }
    if ($env:GH_TOKEN) { $headers.Authorization = "Bearer $env:GH_TOKEN" }
    $selector = if ($env:MSVC_KIT_VERSION -eq 'latest') { 'latest' } else {
        'tags/' + [uri]::EscapeDataString('v' + $env:MSVC_KIT_VERSION.TrimStart('v'))
    }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/loonghao/msvc-kit/releases/$selector" -Headers $headers
    $assets = @($release.assets | Where-Object { $_.name -match '^msvc-kit(-[0-9.]+)?-x86_64-windows\.exe$' })
    if ($assets.Count -ne 1) { throw "Release $($release.tag_name) has no unique Windows x64 executable" }
    $asset = $assets[0]
    if ($asset.digest -notmatch '^sha256:([a-fA-F0-9]{64})$') { throw 'Release asset has no SHA256 digest; provide an explicitly verified msvc-kit-path' }
    $expectedHash = $Matches[1]
    $exePath = Join-Path $installRoot 'msvc-kit.exe'
    $pendingPath = "$exePath.$([guid]::NewGuid().ToString('N')).part"
    try {
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $pendingPath
        if ((Get-FileHash -LiteralPath $pendingPath -Algorithm SHA256).Hash -ne $expectedHash) { throw 'msvc-kit release SHA256 mismatch' }
        Move-Item -LiteralPath $pendingPath -Destination $exePath -Force
    } finally {
        if (Test-Path -LiteralPath $pendingPath) { Remove-Item -LiteralPath $pendingPath }
    }
    Write-Host "Verified msvc-kit $($release.tag_name)"
}
& $exePath --version
if ($LASTEXITCODE -ne 0) { throw 'msvc-kit executable did not run successfully' }
foreach ($entry in @{ 'exe-path' = $exePath; 'install-dir' = $installRoot }.GetEnumerator()) {
    if ($entry.Value -match '[\r\n]') { throw 'Paths must not contain line breaks' }
    "$($entry.Key)=$($entry.Value)" >> $env:GITHUB_OUTPUT
}
