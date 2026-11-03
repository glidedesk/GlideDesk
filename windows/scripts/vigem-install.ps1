# TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
# Run elevated once (gamepad.js): installs the ViGEmBus driver
# (https://github.com/nefarius/ViGEmBus, BSD-3-Clause, signed by Nefarius Software Solutions e.U.)
# for Remote > Real gamepad: the phone's gamepad as an Xbox 360 controller.
param(
  [Parameter(Mandatory = $true)][string]$Url,
  [Parameter(Mandatory = $true)][string]$Sha256,
  [Parameter(Mandatory = $true)][string]$Log
)
$ErrorActionPreference = 'Stop'
try {
  $dir = Join-Path $env:ProgramData 'TwinDesk\vigem'
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $exe = Join-Path $dir 'ViGEmBus-setup.exe'
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
  Invoke-WebRequest -Uri $Url -OutFile $exe -UseBasicParsing
  if ((Get-FileHash -Algorithm SHA256 $exe).Hash -ne $Sha256) { throw 'the driver download has the wrong checksum' }
  $sig = Get-AuthenticodeSignature $exe
  if ($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'CN=Nefarius Software Solutions') { throw 'the driver installer''s signature is not valid' }

  # An Advanced Installer setup: no window of its own, the MSI quiet, no restart.
  $p = Start-Process -FilePath $exe -ArgumentList '/exenoui', '/qn', '/norestart' -Wait -PassThru -WindowStyle Hidden
  # 3010 / 1641: installed, Windows wants a restart.
  if ($p.ExitCode -ne 0 -and $p.ExitCode -ne 3010 -and $p.ExitCode -ne 1641) { throw ('the driver did not install (error ' + $p.ExitCode + ')') }
  Remove-Item -Force $exe -ErrorAction SilentlyContinue
  Set-Content -Path $Log -Value 'ok'
} catch {
  Set-Content -Path $Log -Value ('error: ' + $_.Exception.Message)
}
