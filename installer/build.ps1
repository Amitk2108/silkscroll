<#
.SYNOPSIS
  Builds dist\SilkScroll-Setup.exe from target\release\silkscroll.exe.

.DESCRIPTION
  Run `cargo build --release` first. Needs Inno Setup 6.3 or newer
  (winget install JRSoftware.InnoSetup). The version defaults to the one in
  Cargo.toml.
#>
param(
    [string]$Version,
    [string]$OutDir = "dist"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not $Version) {
        $Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    }

    $exe = Join-Path $root "target\release\silkscroll.exe"
    if (-not (Test-Path $exe)) { throw "Build the app first: cargo build --release" }

    # build.rs renders the icon into Cargo's OUT_DIR; reuse it for the setup.
    $icon = Get-ChildItem "target\release\build" -Recurse -Filter silkscroll.ico |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1

    $iscc = @(
        (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source,
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $iscc) { throw "Inno Setup not found. Install it with: winget install JRSoftware.InnoSetup" }

    New-Item -ItemType Directory -Force $OutDir | Out-Null
    $arguments = @("/Q", "/DAppVersion=$Version", "/DExePath=$exe", "/O$(Resolve-Path $OutDir)")
    if ($icon) { $arguments += "/DIconFile=$($icon.FullName)" }
    & $iscc @arguments "installer\silkscroll.iss"
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE" }

    Write-Host "Built $OutDir\SilkScroll-Setup.exe (version $Version)"
}
finally {
    Pop-Location
}
