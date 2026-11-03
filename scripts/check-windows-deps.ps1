# Checks that Windows release binaries do not depend on the Visual C++ runtime DLLs.
# A clean Windows (such as the winget validation VM) has no VCRUNTIME140.dll, and an exe that
# imports it fails to start with 0xC0000135. Build with crt-static (.cargo/config.toml) to avoid it.
#
# Usage: scripts/check-windows-deps.ps1 <exe> [<exe> ...]
# Exit codes: 0 = no C runtime DLL dependency, 1 = a C runtime DLL is imported,
#             2 = could not check (file missing, dumpbin missing or failed, no imports read)

param(
    [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
    [string[]]$Path
)

$CrtDll = '^(vcruntime\d*(_\d+)?|msvcp\d*(_\w+)?|ucrtbased?|api-ms-win-crt-[\w-]+)\.dll$'

function Find-Dumpbin {
    $cmd = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $null }
    $found = & $vswhere -latest -products * -find 'VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
    if ($found -and (Test-Path $found)) { return $found }
    return $null
}

$dumpbin = Find-Dumpbin
if (-not $dumpbin) {
    Write-Host "❌ dumpbin.exe not found (install the MSVC build tools)" -ForegroundColor Red
    exit 2
}

$status = 0
foreach ($exe in $Path) {
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
        Write-Host "❌ $exe`: file not found" -ForegroundColor Red
        $status = 2
        continue
    }
    $output = & $dumpbin /nologo /dependents $exe 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ $exe`: dumpbin failed (exit $LASTEXITCODE)" -ForegroundColor Red
        $output | ForEach-Object { Write-Host "   $_" }
        $status = 2
        continue
    }
    # Dependency lines are indented DLL names; matching them, not the localized headings,
    # keeps this independent of the dumpbin language.
    $deps = @($output | ForEach-Object { "$_".Trim() } | Where-Object { $_ -match '^[\w.-]+\.dll$' })
    # Every Windows exe imports at least kernel32.dll, so an empty list means the output was not read.
    if ($deps.Count -eq 0) {
        Write-Host "❌ $exe`: no DLL imports found in the dumpbin output" -ForegroundColor Red
        $output | ForEach-Object { Write-Host "   $_" }
        $status = 2
        continue
    }
    $crt = @($deps | Where-Object { $_ -match $CrtDll })
    if ($crt.Count -gt 0) {
        Write-Host "❌ $exe imports the C runtime: $($crt -join ', ')" -ForegroundColor Red
        if ($status -eq 0) { $status = 1 }
    } else {
        Write-Host "✅ $exe`: no C runtime DLL ($($deps.Count) imports: $($deps -join ', '))" -ForegroundColor Green
    }
}
exit $status
