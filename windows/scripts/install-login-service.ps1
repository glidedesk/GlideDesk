<#
.SYNOPSIS
    GlideDesk — Secure Pre-Login & Lock Screen KVM Service Installer (Windows)
    Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
.DESCRIPTION
    Registers GlideDesk as an elevated system-level background helper or scheduled startup task
    running with highest privileges (WinSta0\Winlogon desktop access) to enable mouse & keyboard
    remote control before user logon and during lock screens / UAC prompts.
#>

[CmdletBinding()]
param(
    [string]$AppPath = "C:\Program Files\GlideDesk\GlideDesk.exe"
)

# Verify Administrator privileges
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error "Error: This script must be run as Administrator in an elevated PowerShell session."
    exit 1
}

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " GlideDesk — Windows Pre-Login & Lock Screen KVM Setup" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# Locate executable
if (-not (Test-Path $AppPath)) {
    $altPaths = @(
        "$env:LOCALAPPDATA\Programs\GlideDesk\GlideDesk.exe",
        "$PSScriptRoot\..\..\src-tauri\target\release\glidedesk.exe",
        "$PSScriptRoot\GlideDesk.exe"
    )
    foreach ($p in $altPaths) {
        if (Test-Path $p) {
            $AppPath = (Resolve-Path $p).Path
            break
        }
    }
}

if (-not (Test-Path $AppPath)) {
    Write-Warning "GlideDesk executable not found at '$AppPath'."
    Write-Host "Please specify -AppPath with the full path to GlideDesk.exe." -ForegroundColor Yellow
} else {
    Write-Host "[+] Found GlideDesk binary: $AppPath" -ForegroundColor Green
}

$TaskName = "GlideDeskLoginService"

Write-Host "[*] Configuring Scheduled Task at Windows startup with highest privileges..." -ForegroundColor Yellow

$Action = New-ScheduledTaskAction -Execute $AppPath -Argument "--daemon"
$Trigger = New-ScheduledTaskTrigger -AtStartup
$Principal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -LogonType ServiceAccount -RunLevel Highest
$Settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Days 365) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)

# Unregister existing task if present
Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue | Out-Null

try {
    Register-ScheduledTask -TaskName $TaskName -Action $Action -Trigger $Trigger -Principal $Principal -Settings $Settings -Description "GlideDesk background KVM input engine for pre-login and lock screen control." | Out-Null
    Write-Host "[✓] Successfully registered '$TaskName' to start at boot as SYSTEM." -ForegroundColor Green
} catch {
    Write-Error "Failed to register scheduled task: $_"
    exit 1
}

Write-Host "`n[✓] Setup complete! GlideDesk will now maintain active KVM control across Windows lock screens." -ForegroundColor Green
Write-Host "    To test immediately, run: Start-ScheduledTask -TaskName '$TaskName'" -ForegroundColor Cyan
