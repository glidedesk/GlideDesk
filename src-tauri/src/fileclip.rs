//! Copying files between the computers with Ctrl+Alt+V.
//!
//! The shortcut is read where the pointer is: on the computer being driven, the
//! key stream arrives from the other one, so both directions work from the same
//! place. That computer reads the files copied on its own clipboard and asks the
//! computer they came from for them; they travel over the same batch transfer the
//! Files page uses and land in the receiving computer's received folder.
//!
//! Only files are sent — a folder on the clipboard is skipped, because the batch
//! transfer carries files.

use std::sync::Mutex;

/// Ctrl and Alt held together, tracked from the key stream a driven computer
/// receives. The whole stream is forwarded key by key, so the two modifiers
/// arrive before the V and their state can be remembered here.
#[derive(Default)]
struct Modifiers {
    ctrl: bool,
    alt: bool,
    /// The V that started the paste, so its release is used up as well.
    pasted_v: bool,
}

static MODIFIERS: Mutex<Modifiers> = Mutex::new(Modifiers {
    ctrl: false,
    alt: false,
    pasted_v: false,
});

/// Feed one received key. Returns true when it is the paste shortcut and the key
/// should be used up instead of typed.
pub fn note_key(vk: u16, is_down: bool) -> bool {
    let mut m = MODIFIERS.lock().unwrap_or_else(|e| e.into_inner());
    match vk {
        // VK_CONTROL, VK_LCONTROL, VK_RCONTROL. VK_LWIN/VK_RWIN are here as well:
        // the Ctrl of the computer driving a Mac arrives as Command, so the same
        // gesture reads the same on both sides of the link.
        0x11 | 0xA2 | 0xA3 | 0x5B | 0x5C => m.ctrl = is_down,
        // VK_MENU, VK_LMENU, VK_RMENU
        0x12 | 0xA4 | 0xA5 => m.alt = is_down,
        // V. The shortcut acts on the press, not on the release, and the release of
        // the V that started it is used up too so nothing types a stray letter.
        0x56 => {
            let take = if is_down {
                let shortcut = m.ctrl && m.alt;
                if shortcut {
                    m.pasted_v = true;
                }
                shortcut
            } else if m.pasted_v {
                m.pasted_v = false;
                true
            } else {
                false
            };
            return take;
        }
        _ => {}
    }
    false
}

/// Forget held modifiers, e.g. when the link ends mid-shortcut so a stale Ctrl
/// held down cannot turn a later V into a paste.
pub fn reset() {
    let mut m = MODIFIERS.lock().unwrap_or_else(|e| e.into_inner());
    m.ctrl = false;
    m.alt = false;
    m.pasted_v = false;
}

/// The files currently copied on this computer's clipboard.
///
/// A shell puts the files it copies or drags on the clipboard as a list, so this
/// reads that list; anything that is not an existing file (folders, plain text, a
/// picture) is left out.
pub fn clipboard_files() -> Vec<String> {
    let files = platform_clipboard_files();
    files
        .into_iter()
        .filter(|p| std::path::Path::new(p).is_file())
        .collect()
}

#[cfg(target_os = "macos")]
fn platform_clipboard_files() -> Vec<String> {
    // Finder puts either one `furl` or a list of aliases on the clipboard, so try
    // the list first and fall back to the single item. The script is Standard
    // Additions only, so it raises no Automation prompt.
    const SCRIPT: &str = r#"
set out to ""
try
  set xs to (the clipboard as list)
  repeat with x in xs
    try
      set out to out & POSIX path of (x as alias) & linefeed
    end try
  end repeat
end try
if out is "" then
  try
    set out to POSIX path of (the clipboard as «class furl») & linefeed
  end try
end if
return out
"#;
    let out = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(SCRIPT)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(windows)]
fn platform_clipboard_files() -> Vec<String> {
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    const CF_HDROP: u32 = 15;

    // Another app can hold the clipboard for a moment, so give it a few tries.
    let mut opened = false;
    for _ in 0..5 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    if !opened {
        log::debug!("[FileClip] clipboard is held by another app");
        return Vec::new();
    }

    let mut files = Vec::new();
    unsafe {
        if let Ok(handle) = GetClipboardData(CF_HDROP) {
            let drop = HDROP(handle.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            for index in 0..count {
                let len = DragQueryFileW(drop, index, None);
                if len == 0 {
                    continue;
                }
                let mut buf = vec![0u16; len as usize + 1];
                let written = DragQueryFileW(drop, index, Some(&mut buf));
                if written > 0 {
                    files.push(String::from_utf16_lossy(&buf[..written as usize]));
                }
            }
        }
        let _ = CloseClipboard();
    }
    files
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_clipboard_files() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(vk: u16) -> bool {
        note_key(vk, true)
    }

    fn release(vk: u16) {
        note_key(vk, false);
    }

    // One test: the tracker is a single piece of state, and tests run in threads.
    #[test]
    fn ctrl_alt_v_is_the_paste_shortcut() {
        reset();
        assert!(!press(0x11)); // Ctrl alone is not it
        assert!(!press(0x12)); // Ctrl+Alt is not it yet
        assert!(press(0x56)); // V completes it
        release(0x56);
        release(0x12);
        release(0x11);

        // Ctrl+V without Alt is an ordinary paste.
        reset();
        assert!(!press(0x11));
        assert!(!press(0x56));
        release(0x56);
        release(0x11);

        // Alt+V without Ctrl is not it either.
        reset();
        assert!(!press(0x12));
        assert!(!press(0x56));
        release(0x56);
        release(0x12);

        // A Windows computer driving a Mac sends its Ctrl as Command.
        reset();
        assert!(!press(0x5B)); // Command
        assert!(!press(0x12)); // Alt
        assert!(press(0x56));
        release(0x56);
        release(0x12);
        release(0x5B);

        // The side-specific codes a keyboard adapter may send count the same.
        reset();
        assert!(!press(0xA2)); // left Ctrl
        assert!(!press(0xA5)); // right Alt
        assert!(press(0x56));
        release(0x56);
        release(0xA5);
        release(0xA2);

        // A modifier that was let go no longer counts.
        reset();
        press(0x11);
        release(0x11);
        assert!(!press(0x56));
        release(0x56);

        // A link that ends mid-shortcut must not leave Ctrl and Alt held.
        reset();
        press(0x11);
        press(0x12);
        reset();
        assert!(!press(0x56));
        release(0x56);
    }
}
