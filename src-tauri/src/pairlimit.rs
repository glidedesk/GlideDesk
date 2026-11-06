// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Pairing time limits.
//
// Every pairing phase that waits on the other computer has a limit, so a click never
// spins forever and the reason says what to do:
//   connecting — nothing answered at that address
//   starting   — it answered but never sent its half of the code
//   waiting    — its user never pressed Allow
// Reading the code ("code") and the host's "request" wait on our own user and have none.
// Every limit is three times as long over Bluetooth (an address starting with "ble:") or
// on a wired link.

/// The protocol version this build speaks (keep in step with ui/shared/protocol.js).
pub const PROTOCOL_VERSION: u32 = 1;

/// Base limits in milliseconds, by phase.
pub const LIMIT_MS: &[(&str, u64)] = &[
    ("connecting", 8_000),
    ("starting", 15_000),
    ("waiting", 60_000),
];

fn base_limit(phase: &str) -> Option<u64> {
    LIMIT_MS.iter().find(|(p, _)| *p == phase).map(|(_, ms)| *ms)
}

/// The limit for `phase`, tripled over Bluetooth or a wired link. None: no limit.
pub fn limit_for(phase: &str, address: &str, wired: bool) -> Option<u64> {
    let base = base_limit(phase)?;
    let triple = wired || address.starts_with("ble:");
    Some(if triple { base * 3 } else { base })
}

/// Why a phase timed out, as the user should read it.
pub fn limit_message(phase: &str, name: &str, address: &str) -> String {
    if phase == "connecting" && address.starts_with("ble:") {
        return format!(
            "Can’t reach {} over Bluetooth. Is GlideDesk open there, and close by?",
            name
        );
    }
    if phase == "connecting" {
        return format!(
            "Can’t reach {} at {}. Is GlideDesk open there, on the same Wi-Fi? Guest networks keep devices apart.",
            name, address
        );
    }
    if phase == "starting" {
        return format!(
            "{} didn’t start pairing. Try again, and update GlideDesk there if it keeps happening.",
            name
        );
    }
    format!("{} didn’t answer.", name)
}

/// A version_mismatch message from the host ("... speaks protocol v7"): which side to
/// update. None when the message carries no version or the versions agree.
pub fn version_message(msg: &str, name: &str) -> Option<String> {
    let theirs = parse_version(msg)?;
    if theirs < PROTOCOL_VERSION {
        Some(format!("{} runs an older GlideDesk. Update it there.", name))
    } else if theirs > PROTOCOL_VERSION {
        Some(format!(
            "This GlideDesk is older than the one on {}. Update it here.",
            name
        ))
    } else {
        None
    }
}

/// First `v<digits>` token in `msg`.
fn parse_version(msg: &str) -> Option<u32> {
    let bytes = msg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            // A digit run counts only when preceded by 'v' and not part of a word.
            if i > 0 && bytes[i - 1] == b'v' && (i < 2 || !bytes[i - 2].is_ascii_alphanumeric()) {
                if let Ok(n) = msg[i..j].parse::<u32>() {
                    return Some(n);
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_per_phase() {
        assert_eq!(limit_for("connecting", "192.168.1.5:4777", false), Some(8_000));
        assert_eq!(limit_for("starting", "192.168.1.5:4777", false), Some(15_000));
        assert_eq!(limit_for("waiting", "192.168.1.5:4777", false), Some(60_000));
        assert_eq!(limit_for("code", "192.168.1.5:4777", false), None);
        assert_eq!(limit_for("request", "192.168.1.5:4777", false), None);
        assert_eq!(limit_for("failed", "192.168.1.5:4777", false), None);
    }

    #[test]
    fn bluetooth_and_wired_multiply_by_three() {
        assert_eq!(limit_for("connecting", "ble:AA:BB:CC", false), Some(24_000));
        assert_eq!(limit_for("waiting", "ble:AA:BB:CC", false), Some(180_000));
        assert_eq!(limit_for("connecting", "192.168.1.5:4777", true), Some(24_000));
    }

    #[test]
    fn messages_say_what_to_do() {
        assert_eq!(
            limit_message("connecting", "Mac", "192.168.1.5:4777"),
            "Can’t reach Mac at 192.168.1.5:4777. Is GlideDesk open there, on the same Wi-Fi? Guest networks keep devices apart."
        );
        assert_eq!(
            limit_message("connecting", "Mac", "ble:AA"),
            "Can’t reach Mac over Bluetooth. Is GlideDesk open there, and close by?"
        );
        assert_eq!(
            limit_message("starting", "Mac", "192.168.1.5"),
            "Mac didn’t start pairing. Try again, and update GlideDesk there if it keeps happening."
        );
        assert_eq!(limit_message("waiting", "Mac", "192.168.1.5"), "Mac didn’t answer.");
    }

    #[test]
    fn version_message_picks_the_side_to_update() {
        assert_eq!(
            version_message("Mac speaks protocol v0", "Mac").as_deref(),
            Some("Mac runs an older GlideDesk. Update it there.")
        );
        assert_eq!(
            version_message("Mac speaks protocol v9", "Mac").as_deref(),
            Some("This GlideDesk is older than the one on Mac. Update it here.")
        );
        assert_eq!(version_message("Mac speaks protocol v1", "Mac"), None);
        assert_eq!(version_message("no version here", "Mac"), None);
    }

    #[test]
    fn parse_version_finds_the_token() {
        assert_eq!(parse_version("speaks protocol v7"), Some(7));
        assert_eq!(parse_version("protocol v12."), Some(12));
        assert_eq!(parse_version("version 7"), None);
        assert_eq!(parse_version("av7"), None);
    }
}