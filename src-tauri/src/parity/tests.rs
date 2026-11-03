// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Non-trivial unit tests for Phone ADB, Scrcpy protocol, and native Parity modules.

#[cfg(test)]
mod tests {
    use crate::phone::adb::AdbClient;
    use crate::phone::scrcpy::{self, ControlMessage, StreamHeader};
    use crate::parity::scan_installed_apps;

    #[test]
    fn test_phone_assets_default_shape() {
        let assets = crate::phone::get_phone_assets(None);
        assert!(assets.get("apps").is_some());
        assert!(assets.get("wallpaper").is_some());
        assert!(assets["apps"].as_array().is_some());
    }

    #[test]
    fn test_scan_installed_apps() {
        let apps = scan_installed_apps();
        // On development systems (macOS/Windows/Linux), applications should be discovered
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            assert!(!apps.is_empty(), "Expected at least one installed application to be detected");
            for app in &apps {
                assert!(!app.trim().is_empty(), "Application name must not be blank");
            }
            // Ensure apps list is deduplicated and sorted
            let mut sorted = apps.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(apps, sorted);
        }
    }

    #[test]
    fn test_adb_devices_parsing() {
        let sample_output = "\
List of devices attached
emulator-5554          device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emulator64_arm64 transport_id:1
192.168.1.105:5555     device product:coral model:Pixel_4_XL device:coral transport_id:2
R58M30ABCDE            unauthorized usb:1-1 transport_id:3
offline-device-1       offline transport_id:4
";

        let devices = AdbClient::parse_devices(sample_output);
        assert_eq!(devices.len(), 4);

        assert_eq!(devices[0].serial, "emulator-5554");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[0].model.as_deref(), Some("sdk_gphone64_arm64"));
        assert!(devices[0].usb);

        assert_eq!(devices[1].serial, "192.168.1.105:5555");
        assert_eq!(devices[1].state, "device");
        assert_eq!(devices[1].model.as_deref(), Some("Pixel_4_XL"));
        assert!(!devices[1].usb); // Wi-Fi endpoint with port

        assert_eq!(devices[2].serial, "R58M30ABCDE");
        assert_eq!(devices[2].state, "unauthorized");
        assert!(devices[2].usb);

        assert_eq!(devices[3].serial, "offline-device-1");
        assert_eq!(devices[3].state, "offline");
    }

    #[test]
    fn test_adb_mdns_services_parsing() {
        let sample_output = "\
List of discovered services
adb-pair-123456        _adb-tls-pairing._tcp.    192.168.1.50:41235
adb-connect-789012     _adb-tls-connect._tcp.    192.168.1.50:37891
unknown-service        _other._tcp.              192.168.1.80:9999
";

        let services = AdbClient::parse_mdns_services(sample_output);
        // Only pairing and connect services are retained
        assert_eq!(services.len(), 2);

        assert_eq!(services[0].name, "adb-pair-123456");
        assert_eq!(services[0].service_type, "pairing");
        assert_eq!(services[0].address, "192.168.1.50");
        assert_eq!(services[0].port, 41235);

        assert_eq!(services[1].name, "adb-connect-789012");
        assert_eq!(services[1].service_type, "connect");
        assert_eq!(services[1].address, "192.168.1.50");
        assert_eq!(services[1].port, 37891);
    }

    #[test]
    fn test_scrcpy_stream_header_resolution_parsing() {
        // Scrcpy sets the MSB (0x80) on byte 0 to indicate resolution metadata packet
        let mut buf = [0u8; 12];
        buf[0] = 0x80;
        // width = 1080 (0x00000438)
        let w_bytes = 1080u32.to_be_bytes();
        buf[4..8].copy_from_slice(&w_bytes);
        // height = 2400 (0x00000960)
        let h_bytes = 2400u32.to_be_bytes();
        buf[8..12].copy_from_slice(&h_bytes);

        let header = scrcpy::parse_stream_header(&buf);
        assert_eq!(header, StreamHeader::Resolution { width: 1080, height: 2400 });
    }

    #[test]
    fn test_scrcpy_stream_header_frame_parsing() {
        // MSB bit cleared: 8 bytes PTS + 4 bytes frame payload size
        let mut buf = [0u8; 12];
        let pts: u64 = 987654321;
        let size: u32 = 65536;
        buf[0..8].copy_from_slice(&pts.to_be_bytes());
        buf[8..12].copy_from_slice(&size.to_be_bytes());

        let header = scrcpy::parse_stream_header(&buf);
        assert_eq!(header, StreamHeader::Frame { pts, size });
    }

    #[test]
    fn test_scrcpy_control_message_encoding() {
        // 1. Keycode injection: type (0), action (0), keycode (85), repeat (0), metastate (0)
        let key_msg = ControlMessage::inject_keycode(ControlMessage::ACTION_DOWN, 85, 0, 0);
        assert_eq!(key_msg.len(), 14);
        assert_eq!(key_msg[0], ControlMessage::TYPE_INJECT_KEYCODE);
        assert_eq!(key_msg[1], ControlMessage::ACTION_DOWN);
        assert_eq!(u32::from_be_bytes([key_msg[2], key_msg[3], key_msg[4], key_msg[5]]), 85);

        // 2. Text injection: type (1), 4-byte len, text bytes
        let text = "GlideDesk Pair";
        let text_msg = ControlMessage::inject_text(text);
        assert_eq!(text_msg[0], ControlMessage::TYPE_INJECT_TEXT);
        let text_len = u32::from_be_bytes([text_msg[1], text_msg[2], text_msg[3], text_msg[4]]) as usize;
        assert_eq!(text_len, text.len());
        assert_eq!(&text_msg[5..5 + text_len], text.as_bytes());

        // 3. Touch injection: type (2), action, pointer_id, x, y, width, height, pressure, action_btn, buttons
        let touch_msg = ControlMessage::inject_touch(
            ControlMessage::ACTION_MOVE,
            42,
            540,
            960,
            1080,
            2400,
            1000,
            0,
            0,
        );
        assert_eq!(touch_msg[0], ControlMessage::TYPE_INJECT_TOUCH);
        assert_eq!(touch_msg[1], ControlMessage::ACTION_MOVE);
        assert_eq!(u64::from_be_bytes(touch_msg[2..10].try_into().unwrap()), 42);
        assert_eq!(u32::from_be_bytes(touch_msg[10..14].try_into().unwrap()), 540);
        assert_eq!(u32::from_be_bytes(touch_msg[14..18].try_into().unwrap()), 960);

        // 4. Scroll injection: type (3), x, y, width, height, hscroll, vscroll, buttons
        let scroll_msg = ControlMessage::inject_scroll(500, 600, 1080, 2400, -5, 12, 0);
        assert_eq!(scroll_msg[0], ControlMessage::TYPE_INJECT_SCROLL);
        assert_eq!(u32::from_be_bytes(scroll_msg[1..5].try_into().unwrap()), 500);
        assert_eq!(u32::from_be_bytes(scroll_msg[5..9].try_into().unwrap()), 600);
        assert_eq!(i32::from_be_bytes(scroll_msg[13..17].try_into().unwrap()), -5);
        assert_eq!(i32::from_be_bytes(scroll_msg[17..21].try_into().unwrap()), 12);
        assert_eq!(u32::from_be_bytes(scroll_msg[21..25].try_into().unwrap()), 0);

        // 5. Clipboard injection: type (9), seq (8-byte), paste bool, 4-byte len, text bytes
        let cb_text = "https://glidedesk.com";
        let cb_msg = ControlMessage::set_clipboard(1, true, cb_text);
        assert_eq!(cb_msg[0], ControlMessage::TYPE_SET_CLIPBOARD);
        assert_eq!(u64::from_be_bytes(cb_msg[1..9].try_into().unwrap()), 1);
        assert_eq!(cb_msg[9], 1); // paste: true
        let cb_len = u32::from_be_bytes(cb_msg[10..14].try_into().unwrap()) as usize;
        assert_eq!(cb_len, cb_text.len());
        assert_eq!(&cb_msg[14..14 + cb_len], cb_text.as_bytes());
    }
}
