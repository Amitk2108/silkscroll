<#
.SYNOPSIS
  Installs, upgrades over a running copy, and uninstalls SilkScroll, checking
  each step. CI runs this on a clean Windows machine before every release.

.DESCRIPTION
  Don't run this on your own PC: it installs and then removes SilkScroll for
  the current user, which would stop and replace your own installation.
#>
param(
    [Parameter(Mandatory)] [string]$Setup,
    [Parameter(Mandatory)] [string]$Version
)
$ErrorActionPreference = "Stop"

$dir = Join-Path $env:LOCALAPPDATA "Programs\SilkScroll"
$exe = Join-Path $dir "silkscroll.exe"
$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{C5FF4B62-C1C9-4D7F-815E-FBEB76F4AB64}_is1"

function Install {
    $p = Start-Process -FilePath $Setup -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "Setup exited with code $($p.ExitCode)" }
}

function Check($condition, $message) {
    if (-not $condition) { throw "FAILED: $message" }
    Write-Host "ok: $message"
}

# 1. Fresh install.
Install
Check (Test-Path $exe) "installed to $dir"
Check ((Get-Item $exe).VersionInfo.ProductVersion -eq $Version) "installed exe reports version $Version"
Check (Test-Path $uninstallKey) "listed in Settings > Apps"
Check (Test-Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\SilkScroll.lnk") "Start menu shortcut created"

# 2. Upgrade while SilkScroll is running: setup must close it and replace the exe.
Start-Process -FilePath $exe
Start-Sleep -Seconds 3
Check ($null -ne (Get-Process silkscroll -ErrorAction SilentlyContinue)) "SilkScroll is running before the upgrade"
Install
Check ($null -eq (Get-Process silkscroll -ErrorAction SilentlyContinue)) "upgrade closed the running copy"
Check ((Get-Item $exe).VersionInfo.ProductVersion -eq $Version) "upgrade left a working exe in place"

# 3. Uninstall. The uninstaller hands off to a copy of itself, so wait for the files to go.
$p = Start-Process -FilePath (Join-Path $dir "unins000.exe") -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART" -Wait -PassThru
Check ($p.ExitCode -eq 0) "uninstaller exited cleanly"
for ($i = 0; $i -lt 60 -and (Test-Path $exe); $i++) { Start-Sleep -Seconds 1 }
Check (-not (Test-Path $exe)) "uninstall removed the exe"
Check (-not (Test-Path $uninstallKey)) "uninstall removed the Settings > Apps entry"
$run = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name SilkScroll -ErrorAction SilentlyContinue
Check ($null -eq $run) "uninstall removed the startup entry"

Write-Host "Installer smoke test passed."
