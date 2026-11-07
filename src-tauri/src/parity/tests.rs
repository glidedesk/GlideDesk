// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Non-trivial unit tests for Phone ADB, Scrcpy protocol, Camera VCam, and native Parity modules.

#[cfg(test)]
mod tests {
    use crate::camera::process_vcam_frame;
    use crate::parity::{generate_launchagent_plist, is_startup_enabled, parse_wifi_toggle, scan_installed_apps, toggle_app_mute};
    use crate::phone::adb::{resolve_resource_path, AdbClient};
    use crate::phone::scrcpy::{self, ControlMessage, StreamHeader};

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
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            assert!(!apps.is_empty(), "Expected at least one installed application to be detected");
            for app in &apps {
                assert!(!app.trim().is_empty(), "Application name must not be blank");
            }
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
        assert!(!devices[1].usb);

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
    fn test_dns_sd_output_parsing() {
        let dns_sd_fixture = "\
Browsing for _adb-tls-connect._tcp.local.
DATE: ---Tue 03 Nov 2026---
17:54:06.196  ...STARTING...
Timestamp     A/R    Flags  if Domain               Service Type             Instance Name
17:54:07.123  Add        2   4 local.               _adb-tls-connect._tcp.   adb-connect-test-1  192.168.1.120:39841
17:54:07.456  Add        2   4 local.               _adb-tls-pairing._tcp.   adb-pairing-test-2  192.168.1.120:42119
";
        let parsed = AdbClient::parse_bonjour_output(dns_sd_fixture);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "adb-connect-test-1");
        assert_eq!(parsed[0].service_type, "connect");
        assert_eq!(parsed[0].address, "192.168.1.120");
        assert_eq!(parsed[0].port, 39841);

        assert_eq!(parsed[1].name, "adb-pairing-test-2");
        assert_eq!(parsed[1].service_type, "pairing");
        assert_eq!(parsed[1].address, "192.168.1.120");
        assert_eq!(parsed[1].port, 42119);
    }

    #[test]
    fn test_scrcpy_stream_header_resolution_parsing() {
        let mut buf = [0u8; 12];
        buf[0] = 0x80;
        let w_bytes = 1080u32.to_be_bytes();
        buf[4..8].copy_from_slice(&w_bytes);
        let h_bytes = 2400u32.to_be_bytes();
        buf[8..12].copy_from_slice(&h_bytes);

        let header = scrcpy::parse_stream_header(&buf);
        assert_eq!(header, StreamHeader::Resolution { width: 1080, height: 2400 });
    }

    #[test]
    fn test_scrcpy_stream_header_frame_parsing() {
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
        let key_msg = ControlMessage::inject_keycode(ControlMessage::ACTION_DOWN, 85, 0, 0);
        assert_eq!(key_msg.len(), 14);
        assert_eq!(key_msg[0], ControlMessage::TYPE_INJECT_KEYCODE);
        assert_eq!(key_msg[1], ControlMessage::ACTION_DOWN);
        assert_eq!(u32::from_be_bytes([key_msg[2], key_msg[3], key_msg[4], key_msg[5]]), 85);

        let text = "GlideDesk Pair";
        let text_msg = ControlMessage::inject_text(text);
        assert_eq!(text_msg[0], ControlMessage::TYPE_INJECT_TEXT);
        let text_len = u32::from_be_bytes([text_msg[1], text_msg[2], text_msg[3], text_msg[4]]) as usize;
        assert_eq!(text_len, text.len());
        assert_eq!(&text_msg[5..5 + text_len], text.as_bytes());

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

        let scroll_msg = ControlMessage::inject_scroll(500, 600, 1080, 2400, -5, 12, 0);
        assert_eq!(scroll_msg[0], ControlMessage::TYPE_INJECT_SCROLL);
        assert_eq!(u32::from_be_bytes(scroll_msg[1..5].try_into().unwrap()), 500);
        assert_eq!(u32::from_be_bytes(scroll_msg[5..9].try_into().unwrap()), 600);
        assert_eq!(i32::from_be_bytes(scroll_msg[13..17].try_into().unwrap()), -5);
        assert_eq!(i32::from_be_bytes(scroll_msg[17..21].try_into().unwrap()), 12);
        assert_eq!(u32::from_be_bytes(scroll_msg[21..25].try_into().unwrap()), 0);

        let cb_text = "https://glidedesk.com";
        let cb_msg = ControlMessage::set_clipboard(1, true, cb_text);
        assert_eq!(cb_msg[0], ControlMessage::TYPE_SET_CLIPBOARD);
        assert_eq!(u64::from_be_bytes(cb_msg[1..9].try_into().unwrap()), 1);
        assert_eq!(cb_msg[9], 1);
        let cb_len = u32::from_be_bytes(cb_msg[10..14].try_into().unwrap()) as usize;
        assert_eq!(cb_len, cb_text.len());
        assert_eq!(&cb_msg[14..14 + cb_len], cb_text.as_bytes());
    }

    #[test]
    fn test_vcam_frame_error_paths() {
        // 1. Empty buffer must be rejected
        let res_empty = process_vcam_frame(&[], true);
        assert!(res_empty.is_err());
        assert_eq!(res_empty.unwrap_err(), "vcam_frame: frame buffer is empty");

        // 2. Inactive vcam sink must be rejected
        let dummy_frame = vec![0u8; 1024];
        let res_inactive = process_vcam_frame(&dummy_frame, false);
        assert!(res_inactive.is_err());
        assert_eq!(res_inactive.unwrap_err(), "Virtual camera sink is not available or not running");

        // 3. Active vcam sink with valid buffer succeeds
        let res_ok = process_vcam_frame(&dummy_frame, true);
        assert!(res_ok.is_ok());
    }

    #[test]
    fn test_net_wifi_and_set_startup() {
        // Test parsing of various wifi toggle configurations
        assert!(parse_wifi_toggle(&serde_json::json!(true)));
        assert!(!parse_wifi_toggle(&serde_json::json!(false)));
        assert!(parse_wifi_toggle(&serde_json::json!({ "on": true })));
        assert!(!parse_wifi_toggle(&serde_json::json!({ "on": false })));
        assert!(parse_wifi_toggle(&serde_json::json!({ "power": "on" })));
        assert!(!parse_wifi_toggle(&serde_json::json!({ "power": "off" })));

        // Test LaunchAgent plist generation
        let plist = generate_launchagent_plist("/Applications/GlideDesk.app/Contents/MacOS/glidedesk");
        assert!(plist.contains("<string>com.glidedesk.desktop</string>"));
        assert!(plist.contains("<string>/Applications/GlideDesk.app/Contents/MacOS/glidedesk</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<true/>"));

        // Verify is_startup_enabled does not panic
        let _ = is_startup_enabled();
    }

    #[test]
    fn test_resource_path_resolution() {
        // 1. Existing file in repo root
        let cargo_toml = resolve_resource_path("Cargo.toml");
        assert!(cargo_toml.is_some());
        assert!(cargo_toml.unwrap().is_file());

        // 2. Scrcpy-server vendor resolution
        let scrcpy = AdbClient::find_scrcpy_server();
        assert!(scrcpy.is_some());
        assert!(scrcpy.unwrap().is_file());

        // 3. Non-existent path returns None
        let missing = resolve_resource_path("non_existent_resource_xyz_987654.bin");
        assert!(missing.is_none());
    }

    #[test]
    fn test_mute_notif_app_logic() {
        let mut muted = Vec::new();
        let app_name = "Messages";

        // Mute
        assert!(toggle_app_mute(&mut muted, app_name));
        assert_eq!(muted, vec!["Messages"]);

        // Case-insensitive Unmute
        assert!(!toggle_app_mute(&mut muted, "messages"));
        assert!(muted.is_empty());
    }

    #[test]
    fn test_get_settings_url() {
        use crate::parity::get_settings_url;
        assert!(get_settings_url("screen").is_some());
        assert!(get_settings_url("accessibility").is_some());
        assert!(get_settings_url("inputMonitoring").is_some());
        assert!(get_settings_url("localNetwork").is_some());
        assert!(get_settings_url("fullDisk").is_some());
        assert!(get_settings_url("bluetooth").is_some());
        assert!(get_settings_url("location").is_some());
        assert!(get_settings_url("extensions").is_some());
        assert!(get_settings_url("unknownPermission").is_none());
    }
}
