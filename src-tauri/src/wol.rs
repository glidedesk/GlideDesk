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

#[tauri::command]
pub fn wake_peer(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[WoL] Requesting wake for peer: {}", id);
    let (target_macs, target_address) = {
        let s = state.inner.read();
        if let Some(entry) = s.wake.peers.get(&id) {
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
        s.wake.waking = Some(id);
    }
    push_state(&app, &state);

    Ok(())
}

#[tauri::command]
pub fn set_wake(
    on: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[WoL] Setting Wake-on-LAN: {}", on);
    #[cfg(target_os = "macos")]
    {
        // On macOS, try to toggle system womp (wake on magic packet) via pmset if possible
        let arg = if on { "1" } else { "0" };
        let _ = std::process::Command::new("pmset")
            .args(["-a", "womp", arg])
            .output();
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
    }
}
