// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Wake-on-LAN (WoL) Magic Packet Generator and Peer Awakening Service

use std::net::UdpSocket;
use tauri::{AppHandle, State};

use crate::commands::push_state;
use crate::state::AppState;

/// Parse MAC address from colon-separated, dash-separated, or raw 12-char hex.
pub fn parse_mac(mac_str: &str) -> Result<[u8; 6], String> {
    let clean: String = mac_str
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();

    if clean.len() != 12 {
        return Err(format!(
            "Invalid MAC address '{}': expected 12 hexadecimal characters, found {}",
            mac_str,
            clean.len()
        ));
    }

    let mut bytes = [0u8; 6];
    for i in 0..6 {
        bytes[i] = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16)
            .map_err(|e| format!("Failed to parse MAC byte at index {}: {}", i, e))?;
    }

    Ok(bytes)
}

/// Construct 102-byte Wake-on-LAN magic packet: 6x 0xFF followed by 16 repetitions of 6-byte target MAC.
pub fn build_magic_packet(mac_str: &str) -> Result<[u8; 102], String> {
    let mac = parse_mac(mac_str)?;
    let mut packet = [0xFFu8; 102];
    for i in 0..16 {
        let offset = 6 + i * 6;
        packet[offset..offset + 6].copy_from_slice(&mac);
    }
    Ok(packet)
}

/// Send Wake-on-LAN magic packet to broadcast address and optional peer IP on ports 9 and 7.
pub fn send_wol_magic_packet(mac_str: &str, target_ip: Option<&str>) -> Result<(), String> {
    let packet = build_magic_packet(mac_str)?;
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("Failed to bind UDP socket for WoL: {}", e))?;
    socket
        .set_broadcast(true)
        .map_err(|e| format!("Failed to set UDP broadcast flag: {}", e))?;

    let mut destinations = vec![
        "255.255.255.255:9".to_string(),
        "255.255.255.255:7".to_string(),
    ];

    if let Some(ip) = target_ip {
        let clean_ip = ip.trim();
        if !clean_ip.is_empty() {
            destinations.push(format!("{}:9", clean_ip));
            destinations.push(format!("{}:7", clean_ip));
        }
    }

    let mut sent_any = false;
    for dest in &destinations {
        if socket.send_to(&packet, dest).is_ok() {
            sent_any = true;
        }
    }

    if sent_any {
        log::info!(
            "[WoL] Sent magic packet for MAC {} to {:?}",
            mac_str,
            destinations
        );
        Ok(())
    } else {
        Err(format!(
            "Failed to transmit WoL packet to any destination: {:?}",
            destinations
        ))
    }
}

/// Pure builder for OS-specific Wake-on-LAN system configuration commands.
pub fn build_wake_config_command(
    os: &str,
    on: bool,
    iface: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    match os.to_lowercase().as_str() {
        "macos" | "mac" | "darwin" => {
            let arg = if on { "1" } else { "0" };
            Ok(("pmset".to_string(), vec!["-a".to_string(), "womp".to_string(), arg.to_string()]))
        }
        "windows" | "win" => {
            let action = if on { "/deviceenablewake" } else { "/devicedisablewake" };
            let dev = iface.unwrap_or("Ethernet");
            Ok(("powercfg".to_string(), vec![action.to_string(), dev.to_string()]))
        }
        "linux" => {
            let dev = iface.unwrap_or("eth0");
            let flag = if on { "g" } else { "d" };
            Ok(("ethtool".to_string(), vec!["-s".to_string(), dev.to_string(), "wol".to_string(), flag.to_string()]))
        }
        _ => Err(format!("Unsupported OS for Wake-on-LAN configuration: '{}'", os)),
    }
}

/// Retrieve local network interface MAC addresses for advertising in wol.info.
pub fn get_local_macs() -> Vec<String> {
    let mut macs = Vec::new();
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("ifconfig").output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("ether ") {
                    if let Some(mac) = trimmed.strip_prefix("ether ") {
                        let clean_mac = mac.trim().split_whitespace().next().unwrap_or("");
                        if parse_mac(clean_mac).is_ok() && clean_mac != "00:00:00:00:00:00" {
                            macs.push(clean_mac.to_string());
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = std::process::Command::new("ip").args(["link"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("link/ether ") {
                    if let Some(mac) = trimmed.strip_prefix("link/ether ") {
                        let clean_mac = mac.trim().split_whitespace().next().unwrap_or("");
                        if parse_mac(clean_mac).is_ok() && clean_mac != "00:00:00:00:00:00" {
                            macs.push(clean_mac.to_string());
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("getmac").args(["/fo", "csv", "/nh"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split(',').collect();
                if let Some(raw) = parts.first() {
                    let clean = raw.trim_matches('"').trim();
                    if parse_mac(clean).is_ok() {
                        macs.push(clean.replace('-', ":").to_lowercase());
                    }
                }
            }
        }
    }
    macs
}

/// Record peer WoL MAC addresses and last-known IP into app state.
pub fn record_peer_wol_info(
    state: &AppState,
    peer_id: &str,
    name: &str,
    os: &str,
    macs: Vec<String>,
    address: Option<&str>,
) {
    if macs.is_empty() {
        return;
    }
    let mut s = state.inner.write();
    s.wake.peers.insert(
        peer_id.to_string(),
        serde_json::json!({
            "name": name,
            "os": os,
            "macs": macs,
            "address": address.unwrap_or(""),
            "at": chrono::Utc::now().timestamp_millis()
        }),
    );
}

/// Internal testable WoL peer awakening logic.
pub fn wake_peer_internal(id: &str, state: &AppState) -> Result<Vec<String>, String> {
    let (target_macs, target_address) = {
        let s = state.inner.read();
        if let Some(entry) = s.wake.peers.get(id) {
            let mut macs = Vec::new();
            if let Some(arr) = entry.get("macs").and_then(|v| v.as_array()) {
                for m in arr {
                    if let Some(s) = m.as_str() {
                        macs.push(s.to_string());
                    }
                }
            } else if let Some(m) = entry.get("mac").and_then(|v| v.as_str()) {
                macs.push(m.to_string());
            }

            let addr = entry
                .get("address")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            (macs, addr)
        } else {
            (Vec::new(), None)
        }
    };

    if target_macs.is_empty() {
        return Err(format!(
            "No MAC address recorded for peer '{}'. Wake-on-LAN requires discovering peer MAC on LAN.",
            id
        ));
    }

    let mut any_success = false;
    for mac in &target_macs {
        if send_wol_magic_packet(mac, target_address.as_deref()).is_ok() {
            any_success = true;
        }
    }

    if !any_success {
        return Err(format!(
            "Failed to send WoL magic packet for peer '{}' (MACs: {:?})",
            id, target_macs
        ));
    }

    {
        let mut s = state.inner.write();
        s.wake.waking = Some(id.to_string());
    }

    Ok(target_macs)
}

#[tauri::command]
pub fn wake_peer(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[WoL] Requesting wake for peer: {}", id);
    wake_peer_internal(&id, &state)?;
    push_state(&app, &state);
    Ok(())
}

pub fn detect_active_adapter(os: &str) -> Option<String> {
    match os.to_lowercase().as_str() {
        "windows" | "win" => {
            #[cfg(target_os = "windows")]
            {
                if let Ok(output) = std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", "Get-NetAdapter | Where-Object { $_.Status -eq 'Up' } | Select-Object -ExpandProperty Name -First 1"])
                    .output()
                {
                    if output.status.success() {
                        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if !name.is_empty() {
                            return Some(name);
                        }
                    }
                }
            }
            None
        }
        "linux" => {
            #[cfg(target_os = "linux")]
            {
                if let Ok(output) = std::process::Command::new("ip")
                    .args(["route", "show", "default"])
                    .output()
                {
                    if output.status.success() {
                        let text = String::from_utf8_lossy(&output.stdout);
                        let parts: Vec<&str> = text.split_whitespace().collect();
                        if let Some(pos) = parts.iter().position(|&w| w == "dev") {
                            if let Some(iface) = parts.get(pos + 1) {
                                return Some(iface.to_string());
                            }
                        }
                    }
                }
                if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name != "lo" && !name.starts_with("veth") && !name.starts_with("docker") {
                            return Some(name);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

#[tauri::command]
pub fn set_wake(
    on: bool,
    iface: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[WoL] Setting Wake-on-LAN: {} (iface={:?})", on, iface);
    let os = crate::network::current_os();
    let effective_iface = iface.or_else(|| detect_active_adapter(os));
    if let Ok((cmd, args)) = build_wake_config_command(os, on, effective_iface.as_deref()) {
        let _ = std::process::Command::new(&cmd).args(&args).output();
    }

    {
        let mut s = state.inner.write();
        s.wake.waking = if on {
            Some("active".to_string())
        } else {
            None
        };
    }
    push_state(&app, &state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_mac_formats() {
        let expected = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];

        assert_eq!(parse_mac("12:34:56:78:9a:bc").unwrap(), expected);
        assert_eq!(parse_mac("12-34-56-78-9A-BC").unwrap(), expected);
        assert_eq!(parse_mac("123456789abc").unwrap(), expected);
        assert_eq!(parse_mac("  12:34:56:78:9A:BC  ").unwrap(), expected);
    }

    #[test]
    fn test_parse_invalid_mac_formats() {
        assert!(parse_mac("").is_err());
        assert!(parse_mac("12:34:56").is_err()); // Too short
        assert!(parse_mac("12:34:56:78:9a:bc:de").is_err()); // Too long
        assert!(parse_mac("12:34:56:78:9a:zz").is_err()); // Non-hex characters
    }

    #[test]
    fn test_build_magic_packet_structure() {
        let mac_str = "AA:BB:CC:DD:EE:FF";
        let packet = build_magic_packet(mac_str).expect("build magic packet");

        assert_eq!(packet.len(), 102);

        // First 6 bytes are all 0xFF
        assert_eq!(&packet[0..6], &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

        // Subsequent 16 repetitions are AA:BB:CC:DD:EE:FF
        let expected_mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        for i in 0..16 {
            let offset = 6 + i * 6;
            assert_eq!(
                &packet[offset..offset + 6],
                &expected_mac,
                "Repetition {} failed",
                i
            );
        }
    }

    #[test]
    fn test_wake_peer_missing_mac_error() {
        let app_state = AppState::new();
        let s = app_state.inner.read();
        let peer_entry = s.wake.peers.get("non_existent_peer");
        assert!(peer_entry.is_none());

        // wake_peer_internal returns explicit error naming missing peer MAC
        let res = wake_peer_internal("non_existent_peer", &app_state);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("No MAC address recorded for peer 'non_existent_peer'"));
    }

    #[test]
    fn test_build_wake_config_command_all_os() {
        // macOS
        let (cmd, args) = build_wake_config_command("macos", true, None).expect("mac on");
        assert_eq!(cmd, "pmset");
        assert_eq!(args, vec!["-a", "womp", "1"]);

        let (cmd, args) = build_wake_config_command("darwin", false, None).expect("mac off");
        assert_eq!(cmd, "pmset");
        assert_eq!(args, vec!["-a", "womp", "0"]);

        // Windows
        let (cmd, args) = build_wake_config_command("windows", true, Some("Wi-Fi")).expect("win on");
        assert_eq!(cmd, "powercfg");
        assert_eq!(args, vec!["/deviceenablewake", "Wi-Fi"]);

        let (cmd, args) = build_wake_config_command("win", false, None).expect("win off default");
        assert_eq!(cmd, "powercfg");
        assert_eq!(args, vec!["/devicedisablewake", "Ethernet"]);

        // Linux
        let (cmd, args) = build_wake_config_command("linux", true, Some("wlan0")).expect("linux on");
        assert_eq!(cmd, "ethtool");
        assert_eq!(args, vec!["-s", "wlan0", "wol", "g"]);

        let (cmd, args) = build_wake_config_command("linux", false, None).expect("linux off default");
        assert_eq!(cmd, "ethtool");
        assert_eq!(args, vec!["-s", "eth0", "wol", "d"]);

        // Unsupported OS
        assert!(build_wake_config_command("freebsd", true, None).is_err());
    }

    #[test]
    fn test_record_peer_wol_info_and_wake_peer_flow() {
        let app_state = AppState::new();

        // 1. Initially no peer MAC -> wake fails
        assert!(wake_peer_internal("peer_macbook", &app_state).is_err());

        // 2. Record peer WoL info
        record_peer_wol_info(
            &app_state,
            "peer_macbook",
            "Benson MacBook",
            "macos",
            vec!["12:34:56:78:9A:BC".to_string(), "FE:DC:BA:98:76:54".to_string()],
            Some("192.168.1.120"),
        );

        // 3. Verify state peer map populated
        {
            let s = app_state.inner.read();
            let entry = s.wake.peers.get("peer_macbook").expect("peer exists in wake map");
            assert_eq!(entry.get("name").unwrap(), "Benson MacBook");
            assert_eq!(entry.get("os").unwrap(), "macos");
            assert_eq!(entry.get("address").unwrap(), "192.168.1.120");
            let macs = entry.get("macs").and_then(|v| v.as_array()).unwrap();
            assert_eq!(macs.len(), 2);
        }

        // 4. Wake peer succeeds and sets waking flag
        let res = wake_peer_internal("peer_macbook", &app_state);
        assert!(res.is_ok());
        let macs = res.unwrap();
        assert_eq!(macs.len(), 2);

        {
            let s = app_state.inner.read();
            assert_eq!(s.wake.waking.as_deref(), Some("peer_macbook"));
        }
    }
}
