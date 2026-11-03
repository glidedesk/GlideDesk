# TwinDesk - Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
# Run elevated (vcam.js): the virtual camera's media source (twindesk-vcam-<arch>.dll) copied to
# ProgramData\TwinDesk\vcam under a name of its own build (a loaded one can't be overwritten) and
# registered for the whole PC: Windows' Frame Server, which runs it, is a service and doesn't see
# this user's classes or folders. Earlier copies go once nothing has them loaded.
param(
  [Parameter(Mandatory = $true)][string]$Dll,
  [Parameter(Mandatory = $true)][string]$Name,
  [Parameter(Mandatory = $true)][string]$Log
)
$ErrorActionPreference = 'Stop'
try {
  if ($Name -notmatch '^twindesk-vcam-[a-z0-9]+-[0-9a-f]{12}\.dll$') { throw 'bad name' }
  $dir = Join-Path $env:ProgramData 'TwinDesk\vcam'
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  # Only administrators may change it; Frame Server (Local Service) and everyone else may read it.
  & icacls.exe $dir /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-32-545:(OI)(CI)RX' '*S-1-5-19:(OI)(CI)RX' | Out-Null
  $to = Join-Path $dir $Name
  Copy-Item -LiteralPath $Dll -Destination $to -Force
  $p = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\regsvr32.exe') -ArgumentList '/s', ('"' + $to + '"') -Wait -PassThru -WindowStyle Hidden
  if ($p.ExitCode -ne 0) { throw ('registering it failed (' + $p.ExitCode + ')') }
  Get-ChildItem -LiteralPath $dir -Filter 'twindesk-vcam-*.dll' | Where-Object { $_.Name -ne $Name } | ForEach-Object {
    try { Remove-Item -LiteralPath $_.FullName -Force } catch { }
  }
  Set-Content -LiteralPath $Log -Value 'ok' -Encoding UTF8
} catch {
  Set-Content -LiteralPath $Log -Value ('error: ' + $_.Exception.Message) -Encoding UTF8
}
