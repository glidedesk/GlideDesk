// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Native Media Playback & Position Controller (macOS, Windows, Linux, Android Phone)

use tauri::{AppHandle, State};

use crate::commands::push_state;
use crate::network::NetworkManager;
use crate::phone::PhoneManager;
use crate::state::{AppState, NowPlayingState};

/// Validate if the requested media action is supported.
pub fn validate_media_action(action: &str) -> Result<&'static str, String> {
    match action.to_lowercase().as_str() {
        "play" => Ok("play"),
        "pause" => Ok("pause"),
        "toggle" => Ok("toggle"),
        "next" => Ok("next"),
        "prev" | "previous" => Ok("prev"),
        "seek" => Ok("seek"),
        _ => Err(format!("Unsupported media action: '{}'", action)),
    }
}

/// Pure parser for Windows System Media Transport Controls (SMTC) JSON.
pub fn parse_smtc_json(json_str: &str) -> Result<NowPlayingState, String> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Err("No active media session".to_string());
    }
    let val: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|e| format!("Failed to parse SMTC JSON: {}", e))?;

    let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if title.is_empty() {
        return Err("No active media session".to_string());
    }
    let artist = val.get("artist").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let album = val.get("album").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let playing = val.get("playing").and_then(|v| v.as_bool()).unwrap_or(false);
    let duration = val.get("duration").and_then(|v| v.as_u64()).unwrap_or(0);
    let position = val.get("position").and_then(|v| v.as_u64()).unwrap_or(0);

    Ok(NowPlayingState {
        app: "System SMTC".to_string(),
        app_name: "Windows Media".to_string(),
        title,
        artist,
        album,
        playing,
        duration,
        position,
        speed: 1.0,
        next: true,
        prev: true,
        seek: true,
        art: String::new(),
    })
}

/// Pure parser for Linux playerctl MPRIS CLI output.
pub fn parse_playerctl_output(text: &str) -> Result<NowPlayingState, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("No active media session".to_string());
    }
    let parts: Vec<&str> = trimmed.split(":::").collect();
    if parts.len() < 4 {
        return Err("Malformed playerctl output".to_string());
    }
    let title = parts[0].trim().to_string();
    if title.is_empty() {
        return Err("No active media session".to_string());
    }
    let artist = parts[1].trim().to_string();
    let album = parts[2].trim().to_string();
    let playing = parts[3].trim().eq_ignore_ascii_case("Playing");
    let position = if parts.len() > 4 {
        parts[4].trim().parse::<u64>().unwrap_or(0)
    } else {
        0
    };
    let duration = if parts.len() > 5 {
        parts[5].trim().parse::<u64>().unwrap_or(0)
    } else {
        0
    };

    Ok(NowPlayingState {
        app: "MPRIS".to_string(),
        app_name: "Linux Media".to_string(),
        title,
        artist,
        album,
        playing,
        duration,
        position,
        speed: 1.0,
        next: true,
        prev: true,
        seek: true,
        art: String::new(),
    })
}

/// Pure parser for macOS AppleScript / JXA output formatted as:
/// `app:::title:::artist:::album:::playing:::pos:::dur`
pub fn parse_macos_media_output(text: &str) -> Result<NowPlayingState, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("No active media session".to_string());
    }
    let parts: Vec<&str> = trimmed.split(":::").collect();
    if parts.len() < 5 {
        return Err("Malformed macOS media output".to_string());
    }
    let app = parts[0].trim();
    let title = parts[1].trim().to_string();
    if title.is_empty() {
        return Err("No active media session".to_string());
    }
    let artist = parts[2].trim().to_string();
    let album = parts[3].trim().to_string();
    let playing = parts[4].trim().eq_ignore_ascii_case("playing") || parts[4].trim() == "true";
    let position = if parts.len() > 5 {
        parts[5].trim().parse::<f64>().map(|v| v as u64).unwrap_or(0)
    } else {
        0
    };
    let duration = if parts.len() > 6 {
        parts[6].trim().parse::<f64>().map(|v| v as u64).unwrap_or(0)
    } else {
        0
    };

    let app_name = match app {
        "Music" => "Apple Music",
        "Spotify" => "Spotify",
        other => other,
    };

    Ok(NowPlayingState {
        app: app.to_string(),
        app_name: app_name.to_string(),
        title,
        artist,
        album,
        playing,
        duration,
        position,
        speed: 1.0,
        next: true,
        prev: true,
        seek: true,
        art: String::new(),
    })
}

/// Query current now-playing status from local OS or active session.
#[tauri::command]
pub fn get_now_playing(state: State<'_, AppState>) -> Result<NowPlayingState, String> {
    let s = state.inner.read();
    if let Some(ref np) = s.now_playing {
        return Ok(np.clone());
    }

    #[cfg(target_os = "macos")]
    {
        let script = r#"
            function run() {
                var se = Application("System Events");
                var procs = se.processes.name();
                if (procs.indexOf("Music") !== -1) {
                    try {
                        var m = Application("Music");
                        var state = m.playerState();
                        if (state !== "stopped") {
                            var tr = m.currentTrack;
                            return "Music:::" + tr.name() + ":::" + tr.artist() + ":::" + tr.album() + ":::" + state + ":::" + Math.floor(m.playerPosition()) + ":::" + Math.floor(tr.duration());
                        }
                    } catch(e) {}
                }
                if (procs.indexOf("Spotify") !== -1) {
                    try {
                        var s = Application("Spotify");
                        var state = s.playerState();
                        if (state !== "stopped") {
                            var tr = s.currentTrack;
                            return "Spotify:::" + tr.name() + ":::" + tr.artist() + ":::" + tr.album() + ":::" + state + ":::" + Math.floor(s.playerPosition()) + ":::" + Math.floor(tr.duration() / 1000);
                        }
                    } catch(e) {}
                }
                return "";
            }
        "#;
        if let Ok(output) = std::process::Command::new("osascript")
            .args(["-l", "JavaScript", "-e", script])
            .output()
        {
            if output.status.success() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                if let Ok(np) = parse_macos_media_output(&out_str) {
                    return Ok(np);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let script = r#"
            Add-Type -AssemblyName System.Runtime.WindowsRuntime
            $await = ([System.WindowsRuntimeSystemExtensions].GetMethods() | ? { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
            function Await($op, $t) { $task = $await.MakeGenericMethod($t).Invoke($null, @($op)); $task.Wait(-1) | Out-Null; $task.Result }
            [Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager,Windows.Media.Control,ContentType=WindowsRuntime] | Out-Null
            $m = Await ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager]::RequestAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager])
            $s = $m.GetCurrentSession()
            if ($s) {
                $p = $s.GetPlaybackInfo()
                $m2 = Await ($s.TryGetMediaPropertiesAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionMediaProperties])
                $t = $s.GetTimelineProperties()
                @{
                    title = $m2.Title
                    artist = $m2.Artist
                    album = $m2.AlbumTitle
                    playing = ($p.PlaybackStatus -eq 'Playing')
                    position = [math]::Floor($t.Position.TotalSeconds)
                    duration = [math]::Floor($t.EndTime.TotalSeconds)
                } | ConvertTo-Json -Compress
            }
        "#;
        if let Ok(output) = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
            .output()
        {
            if output.status.success() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                if let Ok(np) = parse_smtc_json(&out_str) {
                    return Ok(np);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = std::process::Command::new("playerctl")
            .args(["metadata", "--format", "{{title}}:::{{artist}}:::{{album}}:::{{status}}:::{{position}}:::{{mpris:length}}"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Ok(np) = parse_playerctl_output(&text) {
                    return Ok(np);
                }
            }
        }
    }

    Err("No active media session".to_string())
}

#[tauri::command]
pub async fn control_media(
    action: String,
    position: Option<u64>,
    state: State<'_, AppState>,
    network: State<'_, NetworkManager>,
    phone_mgr: State<'_, PhoneManager>,
    app: AppHandle,
) -> Result<(), String> {
    let canonical = validate_media_action(&action)?;
    if canonical == "seek" && position.is_none() {
        return Err("Position required for seek action".to_string());
    }

    log::info!("[Media] control_media: action={}, pos={:?}", canonical, position);

    // 1. If active phone session, send ADB command
    let has_phone = {
        let guard = phone_mgr.session.read();
        if let Some(ref sess) = *guard {
            let keycode = match canonical {
                "play" | "pause" | "toggle" => 85, // KEYCODE_MEDIA_PLAY_PAUSE
                "next" => 87,                      // KEYCODE_MEDIA_NEXT
                "prev" => 88,                      // KEYCODE_MEDIA_PREVIOUS
                _ => 0,
            };
            if keycode != 0 {
                let _ = sess.inject_key(keycode, true);
                let _ = sess.inject_key(keycode, false);
            }
            true
        } else {
            false
        }
    };

    // 2. Broadcast wire packet to authenticated peer
    network
        .send_packet(crate::network::WireMessage {
            t: "phone.media.ctl".to_string(),
            action: Some(canonical.to_string()),
            pos: position,
            ..Default::default()
        })
        .await;

    // 3. Update local state
    {
        let mut s = state.inner.write();
        if let Some(ref mut np) = s.now_playing {
            match canonical {
                "play" => np.playing = true,
                "pause" => np.playing = false,
                "toggle" => np.playing = !np.playing,
                "seek" => {
                    if let Some(p) = position {
                        np.position = p;
                    }
                }
                _ => {}
            }
        }
    }

    push_state(&app, &state);

    if !has_phone {
        log::info!("[Media] Sent media command to active network peers");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_media_action() {
        assert_eq!(validate_media_action("play").unwrap(), "play");
        assert_eq!(validate_media_action("PAUSE").unwrap(), "pause");
        assert_eq!(validate_media_action("toggle").unwrap(), "toggle");
        assert_eq!(validate_media_action("next").unwrap(), "next");
        assert_eq!(validate_media_action("prev").unwrap(), "prev");
        assert_eq!(validate_media_action("previous").unwrap(), "prev");
        assert_eq!(validate_media_action("seek").unwrap(), "seek");

        assert!(validate_media_action("rewind").is_err());
        assert!(validate_media_action("fast_forward").is_err());
    }

    #[test]
    fn test_now_playing_state_serialization_roundtrip() {
        let np = NowPlayingState {
            app: "com.spotify.music".to_string(),
            app_name: "Spotify".to_string(),
            title: "Bohemian Rhapsody".to_string(),
            artist: "Queen".to_string(),
            album: "A Night at the Opera".to_string(),
            playing: true,
            duration: 354,
            position: 120,
            speed: 1.0,
            next: true,
            prev: true,
            seek: true,
            art: "data:image/jpeg;base64,abc123xyz".to_string(),
        };

        let json = serde_json::to_string(&np).expect("serialize NowPlayingState");
        assert!(json.contains("\"appName\":\"Spotify\""));
        assert!(json.contains("\"title\":\"Bohemian Rhapsody\""));
        assert!(json.contains("\"playing\":true"));
        assert!(json.contains("\"duration\":354"));

        let deserialized: NowPlayingState =
            serde_json::from_str(&json).expect("deserialize NowPlayingState");
        assert_eq!(deserialized.title, "Bohemian Rhapsody");
        assert_eq!(deserialized.artist, "Queen");
        assert_eq!(deserialized.position, 120);
        assert!(deserialized.playing);
    }

    #[test]
    fn test_media_seek_validation() {
        let canonical = validate_media_action("seek").unwrap();
        assert_eq!(canonical, "seek");

        // Missing position for seek is rejected
        let pos_none: Option<u64> = None;
        let is_valid = canonical != "seek" || pos_none.is_some();
        assert!(!is_valid);

        // Provided position is accepted
        let pos_some = Some(45);
        let is_valid_with_pos = canonical != "seek" || pos_some.is_some();
        assert!(is_valid_with_pos);
    }

    #[test]
    fn test_parse_smtc_json_valid_and_empty() {
        let fixture = r#"{
            "title": "Comfortably Numb",
            "artist": "Pink Floyd",
            "album": "The Wall",
            "playing": true,
            "position": 142,
            "duration": 382
        }"#;
        let np = parse_smtc_json(fixture).expect("valid smtc json");
        assert_eq!(np.title, "Comfortably Numb");
        assert_eq!(np.artist, "Pink Floyd");
        assert_eq!(np.album, "The Wall");
        assert!(np.playing);
        assert_eq!(np.position, 142);
        assert_eq!(np.duration, 382);
        assert_eq!(np.app_name, "Windows Media");

        // Empty json or empty title returns Err
        assert!(parse_smtc_json("").is_err());
        assert!(parse_smtc_json(r#"{"title": ""}"#).is_err());
    }

    #[test]
    fn test_parse_playerctl_output_valid_and_empty() {
        let fixture = "Stairway to Heaven:::Led Zeppelin:::Led Zeppelin IV:::Playing:::180:::482";
        let np = parse_playerctl_output(fixture).expect("valid playerctl output");
        assert_eq!(np.title, "Stairway to Heaven");
        assert_eq!(np.artist, "Led Zeppelin");
        assert_eq!(np.album, "Led Zeppelin IV");
        assert!(np.playing);
        assert_eq!(np.position, 180);
        assert_eq!(np.duration, 482);
        assert_eq!(np.app_name, "Linux Media");

        // Empty or short returns Err
        assert!(parse_playerctl_output("").is_err());
        assert!(parse_playerctl_output("Too:::Few").is_err());
    }

    #[test]
    fn test_parse_macos_media_output_valid_and_empty() {
        let fixture_music = "Music:::Get Back:::The Beatles:::Let It Be:::playing:::45:::192";
        let np_music = parse_macos_media_output(fixture_music).expect("valid music output");
        assert_eq!(np_music.app, "Music");
        assert_eq!(np_music.app_name, "Apple Music");
        assert_eq!(np_music.title, "Get Back");
        assert_eq!(np_music.artist, "The Beatles");
        assert_eq!(np_music.album, "Let It Be");
        assert!(np_music.playing);
        assert_eq!(np_music.position, 45);
        assert_eq!(np_music.duration, 192);

        let fixture_spotify = "Spotify:::Starboy:::The Weeknd:::Starboy:::playing:::60:::230";
        let np_spot = parse_macos_media_output(fixture_spotify).expect("valid spotify output");
        assert_eq!(np_spot.app, "Spotify");
        assert_eq!(np_spot.app_name, "Spotify");
        assert_eq!(np_spot.title, "Starboy");
        assert!(np_spot.playing);

        assert!(parse_macos_media_output("").is_err());
        assert!(parse_macos_media_output("Music:::").is_err());
    }
}
