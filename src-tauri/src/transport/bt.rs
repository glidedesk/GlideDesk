// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! Bluetooth between computers, as the way in to a direct network.
//!
//! A direct network needs a name and a password typed on the joining side, and
//! typing is exactly what people forget. Bluetooth is already there and needs no
//! network, so it carries those details: the computer that hosts the direct
//! network offers them over a small Bluetooth LE service, and the other computer
//! reads them, joins, and connects at the address it was told. Nothing runs
//! until the person asks for it.
//!
//! Windows offers the service (it has the peripheral role and it is the side
//! that hosts); macOS reads it through a small helper built with CoreBluetooth
//! (see bt/glidedesk-bt.swift), because the system Bluetooth APIs are not
//! reachable from a command line without one. Linux can look around with BlueZ
//! but does not join this way yet, and says so rather than offering a switch
//! that does nothing.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The service and value the two sides agree on. Written into both helpers; it
/// must not change without changing the other computer's side too.
pub const SERVICE_UUID: &str = "6a1c7d10-9e42-4b1a-9c2e-676c69646564";
pub const CHARACTERISTIC_UUID: &str = "6a1c7d11-9e42-4b1a-9c2e-676c69646564";

/// What one computer tells the other over Bluetooth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectOffer {
    /// Format version; a reader that does not know it says so instead of guessing.
    pub v: u8,
    /// What is being offered. Only `direct` for now.
    pub kind: String,
    /// The name this computer goes by, so the other can say who it found.
    pub name: String,
    #[serde(default)]
    pub ssid: String,
    #[serde(default)]
    pub key: String,
    /// Where the host of the direct network can be reached once joined.
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub port: u16,
}

impl DirectOffer {
    pub fn new(name: &str, ssid: &str, key: &str, addr: &str, port: u16) -> Self {
        Self {
            v: 1,
            kind: "direct".to_string(),
            name: name.to_string(),
            ssid: ssid.to_string(),
            key: key.to_string(),
            addr: addr.to_string(),
            port,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Read what another computer offered. Older or other offers are named,
    /// not silently taken as a direct network.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text.trim())
            .map_err(|e| format!("The other computer sent something unreadable: {}", e))?;
        let kind = value.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        if kind != "direct" {
            return Err("The other computer is not offering a direct network.".to_string());
        }
        let version = value.get("v").and_then(|v| v.as_u64()).unwrap_or(0);
        if version != 1 {
            return Err(format!(
                "The other computer offers a newer kind of connection (version {}). Update GlideDesk on this one.",
                version
            ));
        }
        serde_json::from_value(value)
            .map_err(|e| format!("The other computer's offer was incomplete: {}", e))
    }

    /// A whole offer is useful only if it says where to go.
    pub fn usable(&self) -> bool {
        !self.ssid.trim().is_empty() && !self.key.is_empty() && !self.addr.trim().is_empty()
    }
}

/// The advertiser this app started, if any. One at a time.
static OFFERING: Mutex<Option<Child>> = Mutex::new(None);

// ------------------------------------------------------------------
// What this computer can do
// ------------------------------------------------------------------

/// What this computer can do, for the page to show. Quick on purpose: the
/// first page load must not wait on a radio.
pub fn caps(resource_dir: Option<&Path>) -> Value {
    #[cfg(windows)]
    let caps = json!({
        "advertise": true,
        "read": false,
        "scan": false,
        "state": "unknown",
        "note": "Windows offers the direct network over Bluetooth.",
    });

    #[cfg(target_os = "macos")]
    let caps = {
        // The helper knows whether Bluetooth is on and whether GlideDesk may
        // use it; asking takes a moment, and it is the only true answer.
        let mut state = "unknown".to_string();
        let mut available = true;
        if let Ok(helper) = helper_path(resource_dir) {
            if let Ok(out) = run_with_timeout(&helper, &["caps"], 5) {
                if let Some(line) = out.lines().last() {
                    if let Ok(v) = serde_json::from_str::<Value>(line) {
                        state = v
                            .get("state")
                            .and_then(|s| s.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        available = v.get("le").and_then(|b| b.as_bool()).unwrap_or(true);
                    }
                }
            } else {
                state = "missing".to_string();
                available = false;
            }
        } else {
            state = "missing".to_string();
            available = false;
        }
        // Reading is offered whenever the helper is there, so a permission
        // that is granted after the first look does not hide the control; the
        // helper itself says what to do when Bluetooth is off or not allowed.
        json!({
            "advertise": available,
            "read": available,
            "scan": available,
            "state": state,
            "note": "macOS reads a direct network over Bluetooth.",
        })
    };

    #[cfg(target_os = "linux")]
    let caps = {
        let state = Command::new("bluetoothctl")
            .args(["show"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .map(|text| {
                if text.contains("Powered: yes") {
                    "on"
                } else if text.is_empty() {
                    "unsupported"
                } else {
                    "off"
                }
            })
            .unwrap_or("unsupported");
        json!({
            "advertise": false,
            "read": false,
            "scan": state == "on",
            "state": state,
            "note": "Bluetooth carries a direct network from a Windows computer.",
        })
    };

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    let caps = json!({ "advertise": false, "read": false, "scan": false, "state": "unsupported" });

    let mut value = caps;
    value["offering"] = json!(offering());
    value
}

/// Whether this computer is offering something over Bluetooth right now.
pub fn offering() -> bool {
    OFFERING.lock().is_some()
}

// ------------------------------------------------------------------
// Offering (Windows)
// ------------------------------------------------------------------

/// Offer a direct network over Bluetooth until the person stops it.
pub fn offer(resource_dir: Option<&Path>, offer: &DirectOffer) -> Result<Value, String> {
    if !offer.usable() {
        return Err("There is no direct network to offer yet. Start it first.".to_string());
    }
    stop();

    #[cfg(windows)]
    {
        return offer_windows(offer);
    }

    #[cfg(target_os = "macos")]
    {
        return offer_macos(resource_dir, offer);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (resource_dir, offer);
        Err("This computer cannot offer a direct network over Bluetooth; the Windows one does that.".to_string())
    }
}

#[cfg(windows)]
fn app_data() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("GlideDesk")
}

/// Windows runs a small PowerShell script (bt/windows-bt.ps1): it uses the
/// system's own GATT server, which is the only way to offer a service there.
#[cfg(windows)]
fn offer_windows(offer: &DirectOffer) -> Result<Value, String> {
    let dir = app_data();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not make {}: {}", dir.display(), e))?;

    let payload_file = dir.join("bt-offer.json");
    let out_file = dir.join("bt-offer-status.json");
    let script = dir.join("bt-host.ps1");

    std::fs::write(&payload_file, offer.to_json())
        .map_err(|e| format!("Could not write the offer: {}", e))?;
    std::fs::write(&script, include_str!("../../bt/windows-bt.ps1"))
        .map_err(|e| format!("Could not write the Bluetooth helper: {}", e))?;
    let _ = std::fs::remove_file(&out_file);

    let child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
            "-PayloadFile",
            &payload_file.to_string_lossy(),
            "-OutFile",
            &out_file.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start the Bluetooth helper: {}", e))?;

    // The script says when the service is up, or why it is not.
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut answer = None;
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(&out_file) {
            let text = text.trim_start_matches('\u{feff}').trim().to_string();
            if !text.is_empty() {
                answer = Some(text);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(120));
    }

    let status: Value = match answer {
        Some(text) => serde_json::from_str(&text)
            .unwrap_or_else(|_| json!({ "ok": false, "error": text })),
        None => {
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
            return Err("Bluetooth did not answer in time. Check that it is on and try again.".to_string());
        }
    };

    if !status.get("ok").and_then(|b| b.as_bool()).unwrap_or(false) {
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        let why = status
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("Bluetooth would not start.");
        return Err(why.to_string());
    }

    *OFFERING.lock() = Some(child);
    log::info!(
        "[Bluetooth] Offering {} for {} over Bluetooth",
        offer.ssid,
        offer.addr
    );
    Ok(json!({
        "ok": true,
        "offering": true,
        "bytes": status.get("bytes").cloned().unwrap_or(json!(0)),
    }))
}

/// macOS offers its own network too, for the case where the Windows computer is
/// the one that joins (a hotspot the Mac is on, for instance).
#[cfg(target_os = "macos")]
fn offer_macos(resource_dir: Option<&Path>, offer: &DirectOffer) -> Result<Value, String> {
    let helper = helper_path(resource_dir)?;
    let payload = base64(&offer.to_json());
    let mut child = Command::new(&helper)
        .args(["advertise", "--payload", &payload, "--name", &offer.name])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start the Bluetooth helper: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "The Bluetooth helper said nothing.".to_string())?;
    let mut lines = BufReader::new(stdout).lines();
    let first = lines
        .next()
        .transpose()
        .map_err(|e| format!("The Bluetooth helper said nothing: {}", e))?
        .unwrap_or_default();
    let status: Value = serde_json::from_str(&first)
        .map_err(|_| format!("The Bluetooth helper answered oddly: {}", first))?;

    if !status.get("ok").and_then(|b| b.as_bool()).unwrap_or(false) {
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        let why = status
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("Bluetooth would not start.");
        return Err(why.to_string());
    }

    *OFFERING.lock() = Some(child);
    log::info!("[Bluetooth] Offering {} over Bluetooth", offer.ssid);
    Ok(json!({ "ok": true, "offering": true }))
}

/// Stop offering, if this computer was.
pub fn stop() -> Value {
    let taken = OFFERING.lock().take();
    if let Some(mut child) = taken {
        let _ = child.kill();
        let _ = child.wait();
        log::info!("[Bluetooth] Stopped offering");
    }
    json!({ "ok": true, "offering": false })
}

// ------------------------------------------------------------------
// Reading (macOS)
// ------------------------------------------------------------------

/// Read what a nearby computer is offering over Bluetooth.
pub fn read(resource_dir: Option<&Path>, seconds: u64) -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    {
        let helper = helper_path(resource_dir)?;
        let out = run_with_timeout(&helper, &["read", "--seconds", &seconds.to_string()], seconds + 10)
            .map_err(|e| format!("Could not use Bluetooth: {}", e))?;
        let line = out
            .lines()
            .last()
            .ok_or_else(|| "Bluetooth answered nothing.".to_string())?;
        let answer: Value = serde_json::from_str(line)
            .map_err(|_| format!("Bluetooth answered oddly: {}", line))?;
        if !answer.get("ok").and_then(|b| b.as_bool()).unwrap_or(false) {
            return Err(answer
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("No computer answered over Bluetooth.")
                .to_string());
        }
        let text = answer.get("text").and_then(|t| t.as_str()).unwrap_or("");
        let offer = DirectOffer::from_json(text)?;
        if !offer.usable() {
            return Err("The other computer's offer did not say which network to join.".to_string());
        }
        log::info!(
            "[Bluetooth] Found {} offering {} at {}",
            offer.name,
            offer.ssid,
            offer.addr
        );
        return Ok(json!({
            "ok": true,
            "offer": offer,
            "from": answer.get("name").cloned().unwrap_or(json!("")),
        }));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (resource_dir, seconds);
        Err("This computer cannot read a direct network over Bluetooth; a Mac does that.".to_string())
    }
}

/// Nearby Bluetooth devices, for when someone wants to see that the radio works.
pub fn scan(resource_dir: Option<&Path>, seconds: u64) -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    {
        let helper = helper_path(resource_dir)?;
        let out = run_with_timeout(&helper, &["scan", "--seconds", &seconds.to_string()], seconds + 8)
            .map_err(|e| format!("Could not use Bluetooth: {}", e))?;
        let line = out
            .lines()
            .last()
            .ok_or_else(|| "Bluetooth answered nothing.".to_string())?;
        let mut answer: Value = serde_json::from_str(line)
            .map_err(|_| format!("Bluetooth answered oddly: {}", line))?;
        if !answer.get("ok").and_then(|b| b.as_bool()).unwrap_or(false) {
            return Err(answer
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Bluetooth did not scan.")
                .to_string());
        }
        // Only the ones offering a direct network are of use here.
        if let Some(devices) = answer.get_mut("devices").and_then(|d| d.as_array_mut()) {
            devices.retain(|d| {
                d.get("services")
                    .and_then(|s| s.as_array())
                    .map(|list| {
                        list.iter().any(|s| {
                            s.as_str()
                                .map(|s| {
                                    s.replace('-', "")
                                        .eq_ignore_ascii_case(&SERVICE_UUID.replace('-', ""))
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            });
        }
        return Ok(answer);
    }

    #[cfg(target_os = "linux")]
    {
        let out = Command::new("bluetoothctl")
            .args(["--timeout", &seconds.to_string(), "scan", "on"])
            .output()
            .map_err(|e| format!("Could not scan for Bluetooth devices: {}", e))?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let devices: Vec<Value> = text
            .lines()
            .filter_map(|line| {
                let (_, rest) = line.split_once("Device ")?;
                let (address, name) = rest.split_once(' ')?;
                Some(json!({ "id": address, "name": name }))
            })
            .collect();
        return Ok(json!({ "ok": true, "devices": devices }));
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (resource_dir, seconds);
        Err("Scanning is not needed here; this computer offers the network.".to_string())
    }
}

// ------------------------------------------------------------------
// The helper beside the app
// ------------------------------------------------------------------

/// Where the little Bluetooth helper lives. Inside the app bundle it is a
/// resource; a checkout keeps it in `src-tauri/resources`; a build can point at
/// another one with GLIDEDESK_BT_HELPER.
pub fn helper_path(resource_dir: Option<&Path>) -> Result<PathBuf, String> {
    let mut tried: Vec<PathBuf> = Vec::new();
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(explicit) = std::env::var("GLIDEDESK_BT_HELPER") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Some(dir) = resource_dir {
        candidates.push(dir.join("glidedesk-bt"));
        candidates.push(dir.join("resources").join("glidedesk-bt"));
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        candidates.push(exe.join("glidedesk-bt"));
        candidates.push(exe.join("../Resources/glidedesk-bt"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("glidedesk-bt"));
    candidates.push(PathBuf::from("/usr/local/bin/glidedesk-bt"));

    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
        tried.push(candidate);
    }
    Err(format!(
        "The Bluetooth helper is missing from this build. Looked in: {}",
        tried
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Run the helper and keep its answer, giving up after `seconds` so a stuck
/// permission prompt cannot hang the app.
fn run_with_timeout(program: &Path, args: &[&str], seconds: u64) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not run {}: {}", program.display(), e))?;

    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Bluetooth took too long to answer.".to_string());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not use {}: {}", program.display(), e));
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Could not read the Bluetooth answer: {}", e))?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let err = String::from_utf8_lossy(&output.stderr).to_string();
    if text.trim().is_empty() {
        if !err.trim().is_empty() {
            return Err(err.trim().to_string());
        }
        return Err("Bluetooth answered nothing.".to_string());
    }
    Ok(text)
}

/// Base64 without pulling in a dependency at this layer.
pub fn base64(text: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offer_round_trips() {
        let offer = DirectOffer::new("GlideDesk Direct", "Direct network", "password123", "10.0.0.1", 47800);
        let text = offer.to_json();
        let back = DirectOffer::from_json(&text).expect("read back");
        assert_eq!(offer, back);
        assert!(back.usable());
    }

    #[test]
    fn an_offer_from_another_kind_is_refused() {
        let err = DirectOffer::from_json(r#"{"v":1,"kind":"phone","ssid":"x","key":"y"}"#)
            .expect_err("not a direct network");
        assert!(err.contains("not offering"));
    }

    #[test]
    fn an_offer_from_the_future_is_named() {
        let err = DirectOffer::from_json(r#"{"v":9,"kind":"direct","ssid":"x","key":"y","addr":"1.2.3.4","port":1}"#)
            .expect_err("too new");
        assert!(err.contains("version 9"));
    }

    #[test]
    fn an_offer_without_an_address_is_not_usable() {
        let mut offer = DirectOffer::new("GlideDesk Direct", "Direct network", "password123", "", 47800);
        assert!(!offer.usable());
        offer.addr = "10.0.0.1".to_string();
        assert!(offer.usable());
        offer.key.clear();
        assert!(!offer.usable());
    }

    #[test]
    fn the_bytes_on_the_wire_are_json_and_base64() {
        let offer = DirectOffer::new("GlideDesk Direct", "Direct network", "password123", "10.0.0.1", 47800);
        assert_eq!(base64(&offer.to_json()).is_empty(), false);
        assert_eq!(
            base64(&offer.to_json()),
            "eyJ2IjoxLCJraW5kIjoiZGlyZWN0IiwibmFtZSI6IkdsaWRlRGVzayBEaXJlY3QiLCJzc2lkIjoiRGlyZWN0IG5ldHdvcmsiLCJrZXkiOiJwYXNzd29yZDEyMyIsImFkZHIiOiIxMC4wLjAuMSIsInBvcnQiOjQ3ODAwfQ=="
        );
    }

    #[test]
    fn the_windows_helper_offers_the_agreed_service() {
        // The helper is written out at runtime; a wrong UUID would mean two
        // builds that cannot see each other, so it is checked here.
        let script = include_str!("../../bt/windows-bt.ps1");
        assert!(script.contains(SERVICE_UUID));
        assert!(script.contains(CHARACTERISTIC_UUID));
        assert!(script.contains("StartAdvertising"));
        assert!(script.contains("IsPeripheralRoleSupported"));
    }

    #[test]
    fn the_helpers_agree_on_the_service() {
        let swift = include_str!("../../bt/glidedesk-bt.swift");
        assert!(swift.contains(SERVICE_UUID));
        assert!(swift.contains(CHARACTERISTIC_UUID));
    }
}
