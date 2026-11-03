#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Scrcpy 5.0.1 Protocol: stream headers, video frame decoding, and control messages.

pub const SCRCPY_VERSION: &str = "5.0.1";
pub const DEVICE_SERVER_PATH: &str = "/data/local/tmp/twindesk-scrcpy-server.jar";

pub const CODEC_H264: u32 = 0x68323634;
pub const CODEC_H265: u32 = 0x68323635;
pub const CODEC_AV1: u32 = 0x00617631;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamHeader {
    Resolution { width: u32, height: u32 },
    Frame { pts: u64, size: u32 },
}

pub fn parse_stream_header(buf: &[u8; 12]) -> StreamHeader {
    if (buf[0] & 0x80) != 0 {
        let width = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let height = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        StreamHeader::Resolution { width, height }
    } else {
        let pts = u64::from_be_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]);
        let size = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        StreamHeader::Frame { pts, size }
    }
}

pub struct ControlMessage;

impl ControlMessage {
    pub const TYPE_INJECT_KEYCODE: u8 = 0;
    pub const TYPE_INJECT_TEXT: u8 = 1;
    pub const TYPE_INJECT_TOUCH: u8 = 2;
    pub const TYPE_INJECT_SCROLL: u8 = 3;
    pub const TYPE_BACK_OR_SCREEN_ON: u8 = 4;
    pub const TYPE_EXPAND_NOTIFICATIONS: u8 = 5;
    pub const TYPE_EXPAND_SETTINGS: u8 = 6;
    pub const TYPE_COLLAPSE_PANELS: u8 = 7;
    pub const TYPE_GET_CLIPBOARD: u8 = 8;
    pub const TYPE_SET_CLIPBOARD: u8 = 9;
    pub const TYPE_ROTATE: u8 = 11;
    pub const TYPE_START_APP: u8 = 16;

    pub const ACTION_DOWN: u8 = 0;
    pub const ACTION_UP: u8 = 1;
    pub const ACTION_MOVE: u8 = 2;

    pub fn inject_keycode(action: u8, keycode: u32, repeat: u32, metastate: u32) -> Vec<u8> {
        let mut msg = Vec::with_capacity(14);
        msg.push(Self::TYPE_INJECT_KEYCODE);
        msg.push(action);
        msg.extend_from_slice(&keycode.to_be_bytes());
        msg.extend_from_slice(&repeat.to_be_bytes());
        msg.extend_from_slice(&metastate.to_be_bytes());
        msg
    }

    pub fn inject_text(text: &str) -> Vec<u8> {
        let bytes = text.as_bytes();
        let mut msg = Vec::with_capacity(5 + bytes.len());
        msg.push(Self::TYPE_INJECT_TEXT);
        msg.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        msg.extend_from_slice(bytes);
        msg
    }

    pub fn inject_touch(
        action: u8,
        pointer_id: u64,
        x: u32,
        y: u32,
        width: u16,
        height: u16,
        pressure: u16,
        action_button: u32,
        buttons: u32,
    ) -> Vec<u8> {
        let mut msg = Vec::with_capacity(32);
        msg.push(Self::TYPE_INJECT_TOUCH);
        msg.push(action);
        msg.extend_from_slice(&pointer_id.to_be_bytes());
        msg.extend_from_slice(&x.to_be_bytes());
        msg.extend_from_slice(&y.to_be_bytes());
        msg.extend_from_slice(&width.to_be_bytes());
        msg.extend_from_slice(&height.to_be_bytes());
        msg.extend_from_slice(&pressure.to_be_bytes());
        msg.extend_from_slice(&action_button.to_be_bytes());
        msg.extend_from_slice(&buttons.to_be_bytes());
        msg
    }

    pub fn inject_scroll(
        x: u32,
        y: u32,
        width: u16,
        height: u16,
        hscroll: i32,
        vscroll: i32,
        buttons: u32,
    ) -> Vec<u8> {
        let mut msg = Vec::with_capacity(21);
        msg.push(Self::TYPE_INJECT_SCROLL);
        msg.extend_from_slice(&x.to_be_bytes());
        msg.extend_from_slice(&y.to_be_bytes());
        msg.extend_from_slice(&width.to_be_bytes());
        msg.extend_from_slice(&height.to_be_bytes());
        msg.extend_from_slice(&hscroll.to_be_bytes());
        msg.extend_from_slice(&vscroll.to_be_bytes());
        msg.extend_from_slice(&buttons.to_be_bytes());
        msg
    }

    pub fn back_or_screen_on(action: u8) -> Vec<u8> {
        vec![Self::TYPE_BACK_OR_SCREEN_ON, action]
    }

    pub fn expand_notifications() -> Vec<u8> {
        vec![Self::TYPE_EXPAND_NOTIFICATIONS]
    }

    pub fn expand_settings() -> Vec<u8> {
        vec![Self::TYPE_EXPAND_SETTINGS]
    }

    pub fn collapse_panels() -> Vec<u8> {
        vec![Self::TYPE_COLLAPSE_PANELS]
    }

    pub fn rotate() -> Vec<u8> {
        vec![Self::TYPE_ROTATE]
    }

    pub fn start_app(pkg: &str) -> Vec<u8> {
        let bytes = pkg.as_bytes();
        let mut msg = Vec::with_capacity(5 + bytes.len());
        msg.push(Self::TYPE_START_APP);
        msg.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        msg.extend_from_slice(bytes);
        msg
    }

    pub fn set_clipboard(sequence: u64, paste: bool, text: &str) -> Vec<u8> {
        let bytes = text.as_bytes();
        let mut msg = Vec::with_capacity(14 + bytes.len());
        msg.push(Self::TYPE_SET_CLIPBOARD);
        msg.extend_from_slice(&sequence.to_be_bytes());
        msg.push(if paste { 1 } else { 0 });
        msg.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        msg.extend_from_slice(bytes);
        msg
    }
}
