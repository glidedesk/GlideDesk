// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Shortcuts across keyboard layouts.
//
// Keys travel as their physical place, but each key-down may also say the letter it
// types on the sender's layout (`ch`). While Ctrl or Cmd is held, the receiver presses
// its own key for that letter instead, so a German Mac's Cmd+Z (the key in the Y place)
// still undoes on an English PC (Ctrl+Y there). Without Ctrl/Cmd keys stay where they
// are, and keys from an older app (no `ch`) are unchanged.
//
// The sender fills `ch` from the character the OS actually produced, so the mapping is
// right on every layout. Capture is implemented on macOS (CGEventKeyboardGetUnicodeString);
// Windows (ToUnicodeEx) and Linux (XKB) follow. The receiver maps a letter to its own key
// through the US table below — accurate on a US layout; OS layout querying follows with
// input-method support.

use std::collections::HashMap;

/// Longest `ch`/`tx` we accept (a key typing a ligature, e.g. ක්ෂ on Sinhala Wij 9).
pub const MAX_CHARS: usize = 8;

/// Windows virtual-key code for the key that types `c` on a US layout.
pub fn vk_for_char(c: char) -> Option<u16> {
    let vk = match c {
        'a'..='z' => 0x41 + (c as u16 - 'a' as u16),
        'A'..='Z' => 0x41 + (c as u16 - 'A' as u16),
        '0'..='9' => 0x30 + (c as u16 - '0' as u16),
        ')' => 0x30,
        '!' => 0x31,
        '@' => 0x32,
        '#' => 0x33,
        '$' => 0x34,
        '%' => 0x35,
        '^' => 0x36,
        '&' => 0x37,
        '*' => 0x38,
        '(' => 0x39,
        ' ' => 0x20,
        ';' | ':' => 0xBA,
        '=' | '+' => 0xBB,
        ',' | '<' => 0xBC,
        '-' | '_' => 0xBD,
        '.' | '>' => 0xBE,
        '/' | '?' => 0xBF,
        '`' | '~' => 0xC0,
        '[' | '{' => 0xDB,
        '\\' | '|' => 0xDC,
        ']' | '}' => 0xDD,
        '\'' | '"' => 0xDE,
        _ => return None,
    };
    Some(vk)
}

/// The character a US key types alone, when no OS layout data is available.
pub fn us_char(vk: u16) -> Option<char> {
    let c = match vk {
        0x41..=0x5A => (b'a' + (vk as u8 - 0x41)) as char,
        0x30..=0x39 => (b'0' + (vk as u8 - 0x30)) as char,
        0x20 => ' ',
        0xBA => ';',
        0xBB => '=',
        0xBC => ',',
        0xBD => '-',
        0xBE => '.',
        0xBF => '/',
        0xC0 => '`',
        0xDB => '[',
        0xDC => '\\',
        0xDD => ']',
        0xDE => '\'',
        _ => return None,
    };
    Some(c)
}

/// The character a US key types with Shift.
pub fn us_shift_char(vk: u16) -> Option<char> {
    let c = match vk {
        0x41..=0x5A => (b'A' + (vk as u8 - 0x41)) as char,
        0x30 => ')',
        0x31 => '!',
        0x32 => '@',
        0x33 => '#',
        0x34 => '$',
        0x35 => '%',
        0x36 => '^',
        0x37 => '&',
        0x38 => '*',
        0x39 => '(',
        0xBA => ':',
        0xBB => '+',
        0xBC => '<',
        0xBD => '_',
        0xBE => '>',
        0xBF => '?',
        0xC0 => '~',
        0xDB => '{',
        0xDC => '|',
        0xDD => '}',
        0xDE => '"',
        _ => return None,
    };
    Some(c)
}

/// A printable string of at most MAX_CHARS that we are willing to forward as `ch`/`tx`.
pub fn clean_text(s: &str) -> Option<String> {
    if s.is_empty() || s.chars().count() > MAX_CHARS {
        return None;
    }
    if s.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(s.to_string())
}

/// One character, the only case that can be sent as `ch`.
pub fn single_char(s: &str) -> Option<char> {
    let mut it = s.chars();
    let c = it.next()?;
    if it.next().is_some() || c.is_control() {
        return None;
    }
    Some(c)
}

/// Remembers which key we injected for each incoming key, so key-up matches the key-down
/// even when a shortcut remapped it.
#[derive(Debug, Default)]
pub struct KeyLayout {
    held: HashMap<u16, u16>,
}

impl KeyLayout {
    pub fn new() -> Self {
        Self::default()
    }

    /// The virtual key to inject for an incoming key event.
    ///
    /// `ch` is the letter the sender's key types. `shortcut` is true while Ctrl or Cmd is
    /// held there or here. On key-up the mapping made on key-down is reused and forgotten.
    pub fn resolve(&mut self, vk: u16, ch: Option<&str>, shortcut: bool, is_down: bool) -> u16 {
        if !is_down {
            return self.held.remove(&vk).unwrap_or(vk);
        }
        let mapped = if shortcut {
            ch.and_then(single_char)
                .and_then(vk_for_char)
                .unwrap_or(vk)
        } else {
            vk
        };
        self.held.insert(vk, mapped);
        mapped
    }

    /// Forget held mappings (drive ended or input was released).
    pub fn reset(&mut self) {
        self.held.clear();
    }

    #[cfg(test)]
    fn held_len(&self) -> usize {
        self.held.len()
    }
}

fn global() -> &'static std::sync::Mutex<KeyLayout> {
    static GLOBAL: std::sync::OnceLock<std::sync::Mutex<KeyLayout>> = std::sync::OnceLock::new();
    GLOBAL.get_or_init(|| std::sync::Mutex::new(KeyLayout::new()))
}

/// Resolve against the process-wide layout state.
pub fn resolve(vk: u16, ch: Option<&str>, shortcut: bool, is_down: bool) -> u16 {
    global()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .resolve(vk, ch, shortcut, is_down)
}

/// Clear the process-wide held mappings.
pub fn reset() {
    global()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .reset();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_cmd_z_becomes_english_undo() {
        // The physical key that types 'z' on German is where US has Y (VK 0x59).
        let mut layout = KeyLayout::new();
        let down = layout.resolve(0x59, Some("z"), true, true);
        assert_eq!(down, 0x5A); // US Z
        let up = layout.resolve(0x59, None, false, false);
        assert_eq!(up, 0x5A); // key-up goes to the same key
        assert_eq!(layout.held_len(), 0);
    }

    #[test]
    fn without_shortcut_the_place_is_kept() {
        let mut layout = KeyLayout::new();
        assert_eq!(layout.resolve(0x59, Some("z"), false, true), 0x59);
        assert_eq!(layout.resolve(0x59, None, false, false), 0x59);
    }

    #[test]
    fn older_peer_without_ch_is_unchanged() {
        let mut layout = KeyLayout::new();
        assert_eq!(layout.resolve(0x5A, None, true, true), 0x5A);
        assert_eq!(layout.resolve(0x5A, None, false, false), 0x5A);
    }

    #[test]
    fn non_letter_ch_is_ignored() {
        let mut layout = KeyLayout::new();
        assert_eq!(layout.resolve(0x5A, Some("ж"), true, true), 0x5A);
        assert_eq!(layout.resolve(0x5A, Some("ab"), true, true), 0x5A);
    }

    #[test]
    fn reset_forgets_held_mappings() {
        let mut layout = KeyLayout::new();
        layout.resolve(0x59, Some("z"), true, true);
        assert_eq!(layout.held_len(), 1);
        layout.reset();
        assert_eq!(layout.held_len(), 0);
        assert_eq!(layout.resolve(0x59, None, false, false), 0x59);
    }

    #[test]
    fn us_maps_round_trip() {
        for c in ['a', 'm', 'z', '0', '9', ';', '=', ',', '-', '.', '/', '`', '[', '\\', ']', '\''] {
            let vk = vk_for_char(c).unwrap();
            assert_eq!(us_char(vk), Some(c), "char {:?}", c);
        }
        for c in ['A', 'Z', '!', ')', ':', '+', '<', '_', '>', '?', '~', '{', '|', '}', '"'] {
            let vk = vk_for_char(c).unwrap();
            assert_eq!(us_shift_char(vk), Some(c), "shift char {:?}", c);
        }
    }

    #[test]
    fn clean_text_respects_limits() {
        assert_eq!(clean_text("z").as_deref(), Some("z"));
        assert_eq!(clean_text("ක්ෂ"), Some("ක්ෂ".to_string()));
        assert_eq!(clean_text(""), None);
        assert_eq!(clean_text("123456789"), None); // 9 chars > MAX_CHARS
        assert_eq!(clean_text("a\nb"), None);
    }
}