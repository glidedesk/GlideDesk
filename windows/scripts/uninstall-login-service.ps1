<#
.SYNOPSIS
    GlideDesk — Remove Pre-Login & Lock Screen KVM Service (Windows)
    Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
#>

[CmdletBinding()]
param()

$TaskName = "GlideDeskLoginService"

# Verify Administrator privileges
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error "Error: This script must be run as Administrator in an elevated PowerShell session."
    exit 1
}

Write-Host "[*] Stopping task '$TaskName' if running..." -ForegroundColor Yellow
Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue | Out-Null

Write-Host "[*] Unregistering scheduled task '$TaskName'..." -ForegroundColor Yellow
try {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction Stop
    Write-Host "[✓] Successfully removed '$TaskName'." -ForegroundColor Green
} catch {
    Write-Warning "Task '$TaskName' was not found or already removed."
}
