# Exercise the workflow's actual PowerShell activation block with inherited tools.
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$lines = Get-Content -LiteralPath (Join-Path $repository '.github/workflows/bundle.yml')
$step = [Array]::IndexOf($lines, '      - name: Validate setup.ps1 activates environment')
if ($step -lt 0) { throw 'PowerShell activation workflow step is absent' }
for ($start = $step + 1; $start -lt $lines.Count; $start++) {
    if ($lines[$start] -eq '        run: |') { break }
}
$body = [Collections.Generic.List[string]]::new()
for ($index = $start + 1; $index -lt $lines.Count; $index++) {
    if ($lines[$index].Trim().Length -eq 0) { $body.Add(''); continue }
    if (-not $lines[$index].StartsWith('          ')) { break }
    $body.Add($lines[$index].Substring(10))
}
if ($body.Count -eq 0) { throw 'PowerShell activation workflow block is empty' }

$temporaryParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
$testRoot = Join-Path $temporaryParent ('msvc-kit-bundle-contract-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
try {
    foreach ($fixture in @(
        @{ Name = 'selected-before-inherited'; WrongFirst = $false; NormalizeExpected = $false },
        @{ Name = 'normalized-selected-path'; WrongFirst = $false; NormalizeExpected = $true },
        @{ Name = 'wrong-first-compiler'; WrongFirst = $true; NormalizeExpected = $false },
        @{ Name = 'native-compiler-failure'; WrongFirst = $false; NormalizeExpected = $false; FailureExit = 17 },
        @{ Name = 'negative-native-compiler-failure'; WrongFirst = $false; NormalizeExpected = $false; FailureExit = -1073741515 }
    )) {
        $root = Join-Path $testRoot $fixture.Name
        $bundle = Join-Path $root 'test-bundle'
        $selected = Join-Path $bundle 'selected'
        $inherited = Join-Path $root 'inherited'
        New-Item -ItemType Directory -Path $selected, $inherited | Out-Null
        foreach ($directory in @($selected, $inherited)) {
            # A real native executable exercises PATH ordering and exit handling.
            Copy-Item -LiteralPath $env:ComSpec -Destination (Join-Path $directory 'cl.exe')
        }
        $selectedCompiler = Join-Path $selected 'cl.exe'
        $paths = if ($fixture.WrongFirst) { "$inherited;$selected" } else { "$selected;$inherited" }
        @"
`$env:VSCMD_ARG_HOST_ARCH = 'x64'
`$env:VSCMD_ARG_TGT_ARCH = 'x64'
`$env:PATH = '$paths;' + `$env:PATH
"@ | Set-Content -LiteralPath (Join-Path $bundle 'setup.ps1') -Encoding utf8
        $script = Join-Path $root 'validate.ps1'
        $validation = $body -join "`r`n"
        if (-not $validation.Contains('& $compiler /?')) { throw 'Compiler probe invocation is absent' }
        # The copied cmd.exe needs an explicit exit command: its localized /?
        # resources are not copied. Keep workflow resolution and guards intact.
        $exitCode = if ($fixture.ContainsKey('FailureExit')) { $fixture.FailureExit } else { 0 }
        $validation = $validation.Replace('& $compiler /?', ('& $compiler /d /c exit ' + $exitCode))
        Set-Content -LiteralPath $script -Value $validation -Encoding utf8
        $processInfo = [Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'pwsh.exe'))
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true
        $processInfo.RedirectStandardOutput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.WorkingDirectory = $root
        foreach ($argument in @('-NoProfile', '-File', $script)) { $processInfo.ArgumentList.Add($argument) }
        $processInfo.Environment['BUNDLE_ARCH'] = 'x64'
        $processInfo.Environment['BUNDLE_CL'] = if ($fixture.NormalizeExpected) { Join-Path $selected '.\cl.exe' } else { $selectedCompiler }
        $process = [Diagnostics.Process]::Start($processInfo)
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) {
            $process.Kill($true)
            throw "Activation fixture timed out: $($fixture.Name)"
        }
        $output = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()
        if ($fixture.ContainsKey('FailureExit')) {
            if ($process.ExitCode -eq 0 -or -not $output.Contains('Activated compiler failed')) {
                throw "Native compiler exit $($fixture.FailureExit) must fail activation: $output"
            }
        } elseif ($fixture.WrongFirst) {
            if ($process.ExitCode -eq 0 -or -not $output.Contains($selectedCompiler) -or -not $output.Contains((Join-Path $inherited 'cl.exe'))) {
                throw "Wrong compiler must fail with actual and expected paths: $output"
            }
        } elseif ($process.ExitCode -ne 0) {
            throw "$($fixture.Name) should select the first compiler: $output"
        }
        $process.Dispose()
        Write-Output "PASS: $($fixture.Name)"
    }
} finally {
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if (-not $resolved.StartsWith($temporaryParent, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe temporary fixture path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
