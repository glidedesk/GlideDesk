# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
#
# Offers a direct network over Bluetooth, for a computer that is not on a
# network this one shares. The payload holds the network name, the password and
# the address; the other computer reads it with CoreBluetooth or BlueZ and joins.
#
# Run by transport/bt.rs as:
#   powershell -File bt-host.ps1 -PayloadFile <json> -OutFile <status json>

param(
  [Parameter(Mandatory = $true)][string]$PayloadFile,
  [Parameter(Mandatory = $true)][string]$OutFile
)

$ErrorActionPreference = 'Continue'

function Write-Status([hashtable]$values) {
  $values | ConvertTo-Json -Compress | Out-File $OutFile -Encoding utf8
}

function Fail([string]$message) {
  Write-Status @{ ok = $false; error = $message }
  exit 1
}

# An offer left behind by a crash would still be advertising yesterday's network,
# and a second service with the same identifier may not start beside it. This
# script runs alone: any other copy of it is stopped first.
Get-CimInstance Win32_Process -Filter "Name = 'powershell.exe'" -ErrorAction SilentlyContinue |
  Where-Object { $_.ProcessId -ne $PID -and $_.CommandLine -like '*bt-host.ps1*' } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }

$null = [Windows.Devices.Bluetooth.GenericAttributeProfile.GattServiceProvider, Windows.Devices.Bluetooth.GenericAttributeProfile, ContentType = WindowsRuntime]
$null = [Windows.Devices.Bluetooth.BluetoothAdapter, Windows.Devices.Bluetooth, ContentType = WindowsRuntime]
$null = [Windows.Devices.Radios.Radio, Windows.Devices.Radios, ContentType = WindowsRuntime]
Add-Type -AssemblyName System.Runtime.WindowsRuntime

$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
  })[0]

function Await($operation, [Type]$resultType) {
  $task = $asTask.MakeGenericMethod($resultType).Invoke($null, @($operation))
  $task.Wait()
  return $task.Result
}

# The radio first: a module that is switched off cannot advertise, and saying so
# is better than a service that silently never appears.
try {
  $access = Await ([Windows.Devices.Radios.Radio]::RequestAccessAsync()) ([Windows.Devices.Radios.RadioAccessStatus])
  if ($access -ne 'Allowed') { Fail 'Windows did not allow Bluetooth. Allow it in Settings > Privacy > Radios.' }
  $radios = Await ([Windows.Devices.Radios.Radio]::GetRadiosAsync()) ([System.Collections.Generic.IReadOnlyList[Windows.Devices.Radios.Radio]])
  $bluetooth = $radios | Where-Object { $_.Kind -eq 'Bluetooth' } | Select-Object -First 1
  if ($bluetooth -and $bluetooth.State -ne 'On') { Fail 'Bluetooth is off. Turn it on in Settings > Bluetooth & devices.' }
} catch {
  Fail "Could not ask about the Bluetooth radio: $($_.Exception.Message)"
}

$adapter = Await ([Windows.Devices.Bluetooth.BluetoothAdapter]::GetDefaultAsync()) ([Windows.Devices.Bluetooth.BluetoothAdapter])
if (-not $adapter) { Fail 'This computer has no Bluetooth adapter.' }
if (-not $adapter.IsLowEnergySupported) { Fail 'This Bluetooth adapter does not support Bluetooth LE.' }
if (-not $adapter.IsPeripheralRoleSupported) { Fail 'This Bluetooth adapter cannot offer a service to other computers.' }

try {
  $payload = Get-Content -Path $PayloadFile -Raw -Encoding UTF8
} catch {
  Fail "Could not read what should be offered: $($_.Exception.Message)"
}
if (-not $payload) { Fail 'There is no direct network to offer over Bluetooth.' }

$serviceUuid = [guid]'6a1c7d10-9e42-4b1a-9c2e-676c69646564'
$characteristicUuid = [guid]'6a1c7d11-9e42-4b1a-9c2e-676c69646564'

$created = Await ([Windows.Devices.Bluetooth.GenericAttributeProfile.GattServiceProvider]::CreateAsync($serviceUuid)) ([Windows.Devices.Bluetooth.GenericAttributeProfile.GattServiceProviderResult])
if ($created.Error -ne 'Success') { Fail "Windows would not create the Bluetooth service ($($created.Error))." }
$provider = $created.ServiceProvider
$service = $provider.Service

# The bytes a reader gets. WindowsRuntimeBufferExtensions is the only way
# PowerShell can hand a real IBuffer to WinRT (DataWriter.DetachBuffer does not
# survive the trip), so the value is made from a byte array directly.
$bytes = [System.Text.Encoding]::UTF8.GetBytes($payload)
$value = [System.Runtime.InteropServices.WindowsRuntime.WindowsRuntimeBufferExtensions]::AsBuffer($bytes)

$parameters = [Windows.Devices.Bluetooth.GenericAttributeProfile.GattLocalCharacteristicParameters]::new()
$parameters.CharacteristicProperties = [Windows.Devices.Bluetooth.GenericAttributeProfile.GattCharacteristicProperties]::Read
$parameters.ReadProtectionLevel = [Windows.Devices.Bluetooth.GenericAttributeProfile.GattProtectionLevel]::Plain
$parameters.StaticValue = $value

$characteristicResult = Await ($service.CreateCharacteristicAsync($characteristicUuid, $parameters)) ([Windows.Devices.Bluetooth.GenericAttributeProfile.GattLocalCharacteristicResult])
if ($characteristicResult.Error -ne 'Success') { Fail "Windows would not create the Bluetooth value ($($characteristicResult.Error))." }
$characteristic = $characteristicResult.Characteristic

$advertising = [Windows.Devices.Bluetooth.GenericAttributeProfile.GattServiceProviderAdvertisingParameters]::new()
$advertising.IsDiscoverable = $true
$advertising.IsConnectable = $true
$provider.StartAdvertising($advertising)
Start-Sleep -Milliseconds 400

$status = "$($provider.AdvertisementStatus)"
if ($status -ne 'Started') { Fail "The Bluetooth service did not start ($status)." }

Write-Status @{
  ok = $true
  advertising = $true
  address = $adapter.BluetoothAddress.ToString('X')
  bytes = $bytes.Length
}

# Serve reads until the app stops us (the process is killed, which removes the
# service with it).
while ($true) { Start-Sleep -Seconds 3600 }
