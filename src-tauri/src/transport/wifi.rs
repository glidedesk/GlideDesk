// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! A direct network between this computer and the one it is paired with.
//!
//! When the two are not on a network they share — different Wi-Fi, a phone's
//! hotspot, no router at all — one side hosts a small access point and the other
//! joins it. The session then runs over the address each gets there, and the
//! normal Wi-Fi connection of the host stays up (Windows keeps its own link while
//! its hotspot runs).
//!
//! Windows hosts, because it can do that without dropping its own connection;
//! macOS and Linux join (a Mac cannot host a network that another computer can
//! join, so it is never asked to). Nothing here runs until the person asks for it.

#[cfg(windows)]
use std::path::PathBuf;
use std::process::Command;

use serde_json::{json, Value};

/// What this computer can do, for the page to show.
pub fn caps() -> Value {
    json!({
        "host": cfg!(windows),
        "join": cfg!(any(windows, target_os = "macos", target_os = "linux")),
        // A hotspot is the same thing here, made by the system rather than by us.
        "hotspot": cfg!(windows),
        "state": state(),
    })
}

/// The network this computer is on, or empty when it is not on one.
pub fn state() -> Value {
    #[cfg(target_os = "macos")]
    {
        if let Some(dev) = macos_wifi_device() {
            let ssid = Command::new("networksetup")
                .args(["-getairportnetwork", &dev])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| s.split(':').nth(1).map(|v| v.trim().to_string()))
                .unwrap_or_default();
            // "You are not associated with an AirPort network." is not a name.
            let ssid = if ssid.starts_with("You are not") { String::new() } else { ssid };
            return json!({ "ssid": ssid, "device": dev });
        }
        json!({ "ssid": "" })
    }
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("netsh").args(["wlan", "show", "interfaces"]).output();
        let ssid = out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .map(|text| {
                text.lines()
                    .find_map(|l| {
                        let t = l.trim();
                        // "SSID   : name" is the network; "BSSID : mac" is not.
                        if t.starts_with("BSSID") {
                            return None;
                        }
                        t.strip_prefix("SSID")
                            .and_then(|rest| rest.split_once(':').map(|(_, v)| v.trim().to_string()))
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        json!({ "ssid": ssid })
    }
    #[cfg(target_os = "linux")]
    {
        let ssid = Command::new("nmcli")
            .args(["-t", "-f", "ACTIVE,SSID", "device", "wifi"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|text| {
                text.lines()
                    .find_map(|l| l.strip_prefix("yes:").map(|s| s.to_string()))
            })
            .unwrap_or_default();
        json!({ "ssid": ssid })
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        json!({ "ssid": "" })
    }
}

// ------------------------------------------------------------------
// Hosting (Windows)
// ------------------------------------------------------------------

/// The script that starts and stops a hosted network, kept beside the app's files
/// so it can be run again after a restart.
#[cfg(windows)]
fn host_script() -> Result<PathBuf, String> {
    let dir = dirs_config();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not make {}: {}", dir.display(), e))?;
    let path = dir.join("wifi-host.ps1");
    let body = r#"$ErrorActionPreference = 'Stop'
$mode = $args[0]
$out = $args[1]
$null = [Windows.Networking.NetworkOperators.NetworkOperatorTetheringManager, Windows.Networking.NetworkOperators, ContentType = WindowsRuntime]
$conn = [Windows.Networking.Connectivity.NetworkInformation]::GetInternetConnectionProfile()
$mgr = [Windows.Networking.NetworkOperators.NetworkOperatorTetheringManager]::CreateFromConnectionProfile($conn)
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
  })[0]
function Invoke-Op($op) {
  $t = $asTask.MakeGenericMethod([Windows.Networking.NetworkOperators.NetworkOperatorTetheringOperationResult]).Invoke($null, @($op))
  $t.Wait()
  "$($t.Result.Status)"
}
if ($mode -eq 'stop') {
  $r = Invoke-Op $mgr.StopTetheringAsync()
  @{ on = $false; status = $r } | ConvertTo-Json -Compress | Out-File $out -Encoding utf8
  exit 0
}
$r = Invoke-Op $mgr.StartTetheringAsync()
$ap = $mgr.GetCurrentAccessPointConfiguration()
# The address the other computer will reach this one at: the hosted network's own.
$ip = (Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
  Where-Object { $_.IPAddress -like '192.168.137.*' } | Select-Object -First 1).IPAddress
@{ on = ($mgr.TetheringOperationalState -eq 'On'); status = $r; ssid = "$($ap.Ssid)"; key = "$($ap.Passphrase)"; address = "$ip" } |
  ConvertTo-Json -Compress | Out-File $out -Encoding utf8
"#;
    std::fs::write(&path, body).map_err(|e| format!("Could not write {}: {}", path.display(), e))?;
    Ok(path)
}

#[cfg(windows)]
fn dirs_config() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("GlideDesk")
}

#[cfg(windows)]
fn run_host(mode: &str) -> Result<Value, String> {
    let script = host_script()?;
    let out = dirs_config().join("wifi-host.json");
    let _ = std::fs::remove_file(&out);
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
            mode,
            &out.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("Could not run the network helper: {}", e))?;
    let text = std::fs::read_to_string(&out).unwrap_or_default();
    let json: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("The network helper did not answer ({}): {}", e, status))?;
    Ok(json)
}

/// Start hosting. Returns the network name and password the other computer needs.
pub fn host_start() -> Result<Value, String> {
    #[cfg(windows)]
    {
        let v = run_host("start")?;
        if v.get("on").and_then(|b| b.as_bool()).unwrap_or(false) {
            log::info!(
                "[Wifi] Hosting {} at {}",
                v.get("ssid").and_then(|s| s.as_str()).unwrap_or(""),
                v.get("address").and_then(|s| s.as_str()).unwrap_or("")
            );
        }
        Ok(v)
    }
    #[cfg(not(windows))]
    {
        Err("Only Windows can host a direct network; join the other computer’s instead.".to_string())
    }
}

pub fn host_stop() -> Result<Value, String> {
    #[cfg(windows)]
    {
        let v = run_host("stop")?;
        log::info!("[Wifi] Stopped hosting");
        Ok(v)
    }
    #[cfg(not(windows))]
    {
        Ok(json!({ "on": false }))
    }
}

// ------------------------------------------------------------------
// Joining
// ------------------------------------------------------------------

/// Join the other computer's network. Blocks while the system connects.
pub fn join(ssid: &str, key: &str) -> Result<(), String> {
    let ssid = ssid.trim();
    if ssid.is_empty() {
        return Err("Type the network name first.".to_string());
    }
    log::info!("[Wifi] Joining {}", ssid);

    #[cfg(target_os = "macos")]
    {
        let dev = macos_wifi_device()
            .ok_or_else(|| "This Mac has no Wi-Fi interface to join with.".to_string())?;
        let out = Command::new("networksetup")
            .args(["-setairportnetwork", &dev, ssid, key])
            .output()
            .map_err(|e| format!("Could not join {}: {}", ssid, e))?;
        // networksetup answers on stdout even when it works, and stays quiet on
        // success; a line with "Error" or "not find" is the failure case.
        let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let bad = said.to_lowercase().contains("error")
            || said.to_lowercase().contains("not find")
            || said.to_lowercase().contains("could not");
        if bad {
            return Err(said);
        }
        log::info!("[Wifi] Joined {} on {}", ssid, dev);
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let profile = windows_profile_xml(ssid, key);
        let dir = dirs_config();
        std::fs::create_dir_all(&dir).map_err(|e| format!("Could not make {}: {}", dir.display(), e))?;
        let file = dir.join("wifi-join.xml");
        std::fs::write(&file, profile).map_err(|e| format!("Could not write the profile: {}", e))?;
        let add = Command::new("netsh")
            .args([
                "wlan",
                "add",
                "profile",
                &format!("filename={}", file.display()),
                "user=current",
            ])
            .output()
            .map_err(|e| format!("Could not prepare the network: {}", e))?;
        if !add.status.success() {
            return Err(String::from_utf8_lossy(&add.stdout).trim().to_string());
        }
        let connect = Command::new("netsh")
            .args(["wlan", "connect", &format!("name={}", ssid)])
            .output()
            .map_err(|e| format!("Could not join {}: {}", ssid, e))?;
        if !connect.status.success() {
            return Err(String::from_utf8_lossy(&connect.stdout).trim().to_string());
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let out = Command::new("nmcli")
            .args(["device", "wifi", "connect", ssid, "password", key])
            .output()
            .map_err(|e| format!("Could not join {}: {}", ssid, e))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Joining a network is not supported here.".to_string())
}

/// Leave the network this computer joined, to go back to its own.
pub fn leave() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(dev) = macos_wifi_device() {
            let _ = Command::new("networksetup")
                .args(["-setairportpower", &dev, "off"])
                .output();
            let _ = Command::new("networksetup")
                .args(["-setairportpower", &dev, "on"])
                .output();
        }
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("netsh").args(["wlan", "disconnect"]).output();
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("nmcli").args(["device", "disconnect", "wifi"]).output();
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

/// The Wi-Fi interface of this Mac, as `networksetup` names it.
#[cfg(target_os = "macos")]
pub fn macos_wifi_device() -> Option<String> {
    let out = Command::new("networksetup")
        .args(["-listallhardwareports"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    parse_hardware_port(&text, "Wi-Fi")
        .or_else(|| parse_hardware_port(&text, "AirPort"))
}

/// Read the device under a hardware port heading.
///
/// The listing is blocks of `Hardware Port: <name>`, `Device: <if>`, `Ethernet
/// Address: <mac>`; the device belongs to the heading just above it.
pub fn parse_hardware_port(listing: &str, port: &str) -> Option<String> {
    let mut want = false;
    for line in listing.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("Hardware Port:") {
            want = name.trim() == port;
        } else if want {
            if let Some(dev) = line.strip_prefix("Device:") {
                let dev = dev.trim().to_string();
                if !dev.is_empty() {
                    return Some(dev);
                }
            }
        }
    }
    None
}

/// A Windows WLAN profile for a WPA2 network, which `netsh` can add.
pub fn windows_profile_xml(ssid: &str, key: &str) -> String {
    let escaped = ssid
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>{ssid}</name>
  <SSIDConfig><SSID><name>{ssid}</name></SSID></SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>auto</connectionMode>
  <MSM><security>
    <authEncryption><authentication>WPA2PSK</authentication><encryption>AES</encryption><useOneX>false</useOneX></authEncryption>
    <sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{key}</keyMaterial></sharedKey>
  </security></MSM>
</WLANProfile>"#,
        ssid = escaped,
        key = key
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_port_device_is_read_from_its_heading() {
        let listing = "Hardware Port: Ethernet\nDevice: en0\nEthernet Address: aa:bb\n\nHardware Port: Wi-Fi\nDevice: en1\nEthernet Address: cc:dd\n";
        assert_eq!(parse_hardware_port(listing, "Wi-Fi").as_deref(), Some("en1"));
        assert_eq!(parse_hardware_port(listing, "Ethernet").as_deref(), Some("en0"));
        assert_eq!(parse_hardware_port(listing, "Thunderbolt"), None);
    }

    #[test]
    fn profile_carries_the_name_and_the_key() {
        let xml = windows_profile_xml("Direct network", "password123");
        assert!(xml.contains("<name>Direct network</name>"));
        assert!(xml.contains("<keyMaterial>password123</keyMaterial>"));
        assert!(xml.contains("WPA2PSK"));
    }

    #[test]
    fn profile_escapes_xml_characters() {
        let xml = windows_profile_xml("A&B", "k<1>");
        assert!(xml.contains("A&amp;B"));
        assert!(xml.contains("k&lt;1&gt;"));
    }
}
