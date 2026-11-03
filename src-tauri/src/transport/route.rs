// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Transfer Route Picker & Multi-Transport Fallback Engine

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteMode {
    #[serde(rename = "directTCP")]
    DirectTcp,
    #[serde(rename = "p2pDirect")]
    P2pDirect,
    #[serde(rename = "usbAdb")]
    UsbAdb,
    #[serde(rename = "httpChunked")]
    HttpChunked,
}

impl std::fmt::Display for RouteMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteMode::DirectTcp => write!(f, "directTCP"),
            RouteMode::P2pDirect => write!(f, "p2pDirect"),
            RouteMode::UsbAdb => write!(f, "usbAdb"),
            RouteMode::HttpChunked => write!(f, "httpChunked"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteCandidate {
    pub mode: RouteMode,
    pub target_addr: Option<String>,
    pub port: Option<u16>,
    pub usb_serial: Option<String>,
    pub available: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedRoute {
    pub mode: RouteMode,
    pub endpoint: String,
    pub reason: String,
}

pub struct RoutePicker;

impl RoutePicker {
    pub fn pick(candidates: &[RouteCandidate]) -> SelectedRoute {
        // Priority 1: Direct TCP
        if let Some(c) = candidates.iter().find(|c| c.mode == RouteMode::DirectTcp && c.available) {
            let endpoint = format!("{}:{}", c.target_addr.as_deref().unwrap_or("127.0.0.1"), c.port.unwrap_or(47802));
            log::info!("ROUTE_PICK mode=directTCP reason={}", c.reason);
            return SelectedRoute {
                mode: RouteMode::DirectTcp,
                endpoint,
                reason: c.reason.clone(),
            };
        }

        // Priority 2: P2P Direct (Wi-Fi direct without hotspot)
        if let Some(c) = candidates.iter().find(|c| c.mode == RouteMode::P2pDirect && c.available) {
            let endpoint = format!("{}:{}", c.target_addr.as_deref().unwrap_or("192.168.49.1"), c.port.unwrap_or(47802));
            log::info!("ROUTE_PICK mode=p2pDirect reason={}", c.reason);
            return SelectedRoute {
                mode: RouteMode::P2pDirect,
                endpoint,
                reason: c.reason.clone(),
            };
        }

        // Priority 3: USB ADB tunnel
        if let Some(c) = candidates.iter().find(|c| c.mode == RouteMode::UsbAdb && c.available) {
            let endpoint = format!("usb:{}", c.usb_serial.as_deref().unwrap_or("local"));
            log::info!("ROUTE_PICK mode=usbAdb reason={}", c.reason);
            return SelectedRoute {
                mode: RouteMode::UsbAdb,
                endpoint,
                reason: c.reason.clone(),
            };
        }

        // Priority 4: Fallback to HTTP chunked over local WebSocket port 47800
        let reason = "all direct high-speed routes unavailable, falling back to chunked transport".to_string();
        log::info!("ROUTE_PICK mode=httpChunked reason={}", reason);
        SelectedRoute {
            mode: RouteMode::HttpChunked,
            endpoint: "ws://127.0.0.1:47800".to_string(),
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_picker_direct_tcp_priority() {
        let candidates = vec![
            RouteCandidate {
                mode: RouteMode::HttpChunked,
                target_addr: None,
                port: None,
                usb_serial: None,
                available: true,
                reason: "fallback ready".to_string(),
            },
            RouteCandidate {
                mode: RouteMode::DirectTcp,
                target_addr: Some("192.168.1.100".to_string()),
                port: Some(47802),
                usb_serial: None,
                available: true,
                reason: "tcp_available".to_string(),
            },
        ];

        let selected = RoutePicker::pick(&candidates);
        assert_eq!(selected.mode, RouteMode::DirectTcp);
        assert_eq!(selected.endpoint, "192.168.1.100:47802");
        assert_eq!(selected.reason, "tcp_available");
    }

    #[test]
    fn test_route_picker_fallback_sequence() {
        let candidates = vec![
            RouteCandidate {
                mode: RouteMode::DirectTcp,
                target_addr: Some("192.168.1.100".to_string()),
                port: Some(47802),
                usb_serial: None,
                available: false,
                reason: "tcp_connection_refused".to_string(),
            },
            RouteCandidate {
                mode: RouteMode::UsbAdb,
                target_addr: None,
                port: None,
                usb_serial: Some("emulator-5554".to_string()),
                available: true,
                reason: "usb_device_ready".to_string(),
            },
        ];

        let selected = RoutePicker::pick(&candidates);
        assert_eq!(selected.mode, RouteMode::UsbAdb);
        assert_eq!(selected.endpoint, "usb:emulator-5554");
    }

    #[test]
    fn test_route_picker_all_unavailable_fallback_chunked() {
        let candidates = vec![
            RouteCandidate {
                mode: RouteMode::DirectTcp,
                target_addr: None,
                port: None,
                usb_serial: None,
                available: false,
                reason: "no_route".to_string(),
            },
            RouteCandidate {
                mode: RouteMode::P2pDirect,
                target_addr: None,
                port: None,
                usb_serial: None,
                available: false,
                reason: "p2p_disabled".to_string(),
            },
        ];

        let selected = RoutePicker::pick(&candidates);
        assert_eq!(selected.mode, RouteMode::HttpChunked);
        assert_eq!(selected.endpoint, "ws://127.0.0.1:47800");
    }
}
