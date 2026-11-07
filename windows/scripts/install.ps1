# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# 1-Line Windows PowerShell Installer & Updater (Always installs the latest release)
# Usage: irm https://raw.githubusercontent.com/glidedesk/GlideDesk/main/windows/scripts/install.ps1 | iex

$ErrorActionPreference = "Stop"

Write-Host "=== GlideDesk Windows Auto-Installer ===" -ForegroundColor Cyan
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# 1. Resolve latest release tag
Write-Host "Resolving latest GlideDesk release..." -ForegroundColor Cyan
$tag = $null
try {
    $req = [System.Net.WebRequest]::Create("https://github.com/glidedesk/GlideDesk/releases/latest")
    $req.AllowAutoRedirect = $false
    $resp = $req.GetResponse()
    $loc = $resp.GetResponseHeader("Location")
    $resp.Close()
    if ($loc -match "tag/(v[^/]+)") {
        $tag = $matches[1]
    }
} catch {
    # Fallback to API
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/glidedesk/GlideDesk/releases/latest" -Headers @{ "User-Agent" = "GlideDesk-Installer" }
        $tag = $release.tag_name
    } catch {}
}

if (-not $tag) {
    $tag = "v3.2.0"
}

$version = $tag.TrimStart("v")
Write-Host "Latest release: $tag" -ForegroundColor Green

$downloadUrl = "https://github.com/glidedesk/GlideDesk/releases/download/$tag/GlideDesk_${version}_x64-setup.exe"
$tempPath = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "GlideDesk-setup.exe")

# 2. Download installer
Write-Host "Downloading GlideDesk $tag installer..." -ForegroundColor Cyan
Invoke-WebRequest -Uri $downloadUrl -OutFile $tempPath -UseBasicParsing

# 3. Terminate running instance if any
$running = Get-Process -Name "glidedesk" -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "Closing running GlideDesk instance..." -ForegroundColor Yellow
    Stop-Process -Name "glidedesk" -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
}

# 4. Run silent installer
Write-Host "Installing GlideDesk..." -ForegroundColor Cyan
$process = Start-Process -FilePath $tempPath -ArgumentList "/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS" -Wait -PassThru

if ($process.ExitCode -eq 0) {
    Write-Host "`n✓ GlideDesk $tag installed successfully!" -ForegroundColor Green
} else {
    Write-Host "`nInstaller finished with code $($process.ExitCode)." -ForegroundColor Yellow
}

# Cleanup
Remove-Item -Path $tempPath -Force -ErrorAction SilentlyContinue
