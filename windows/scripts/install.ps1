# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# Headless-safe Windows installer & updater.
# Run via PowerShell:
#   iex (irm https://raw.githubusercontent.com/glidedesk/glidedesk/main/windows/scripts/install.ps1)

$ErrorActionPreference = "Stop"
$Repo = "glidedesk/glidedesk"

Write-Host "==> Fetching latest GlideDesk release from GitHub..." -ForegroundColor Cyan
$Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$Version = $Release.tag_name -replace '^v',''
Write-Host "==> Target version: $Version" -ForegroundColor Green

$Asset = $Release.assets | Where-Object { $_.name -like "*x64-setup.exe" } | Select-Object -First 1
if (-not $Asset) {
    $Asset = $Release.assets | Where-Object { $_.name -like "*.exe" -and $_.name -notlike "*portable*" } | Select-Object -First 1
}

if (-not $Asset) {
    Write-Error "Could not find a valid Windows setup asset in release $Version."
    exit 1
}

$DownloadUrl = $Asset.browser_download_url
$InstallerPath = Join-Path $env:TEMP "GlideDesk_${Version}_Setup.exe"

Write-Host "==> Downloading $DownloadUrl ..." -ForegroundColor Cyan
Invoke-WebRequest -Uri $DownloadUrl -OutFile $InstallerPath -UseBasicParsing

# Check if GlideDesk is currently running
$WasRunning = Get-Process -Name "glidedesk" -ErrorAction SilentlyContinue
if ($WasRunning) {
    Write-Host "==> Stopping running GlideDesk instances..." -ForegroundColor Yellow
    Stop-Process -Name "glidedesk" -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
}

Write-Host "==> Executing silent installation..." -ForegroundColor Cyan
Start-Process -FilePath $InstallerPath -ArgumentList "/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS" -Wait

# Clean up installer artifact
Remove-Item -Path $InstallerPath -Force -ErrorAction SilentlyContinue

Write-Host "==> GlideDesk $Version has been successfully installed & updated!" -ForegroundColor Green
