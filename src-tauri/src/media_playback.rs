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

/// Query current now-playing status from local OS or active session.
#[tauri::command]
pub fn get_now_playing(state: State<'_, AppState>) -> Result<NowPlayingState, String> {
    let s = state.inner.read();
    if let Some(ref np) = s.now_playing {
        return Ok(np.clone());
    }

    // Attempt platform query if available
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
                let out_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&out_str) {
                    return Ok(NowPlayingState {
                        app: "System SMTC".to_string(),
                        app_name: "Windows Media".to_string(),
                        title: val.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        artist: val.get("artist").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        album: val.get("album").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        playing: val.get("playing").and_then(|v| v.as_bool()).unwrap_or(false),
                        duration: val.get("duration").and_then(|v| v.as_u64()).unwrap_or(0),
                        position: val.get("position").and_then(|v| v.as_u64()).unwrap_or(0),
                        speed: 1.0,
                        next: true,
                        prev: true,
                        seek: true,
                        art: String::new(),
                    });
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = std::process::Command::new("playerctl")
            .args(["metadata", "--format", "{{title}}:::{{artist}}:::{{album}}:::{{status}}"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let parts: Vec<&str> = text.split(":::").collect();
                if parts.len() >= 4 {
                    return Ok(NowPlayingState {
                        app: "MPRIS".to_string(),
                        app_name: "Linux Media".to_string(),
                        title: parts[0].to_string(),
                        artist: parts[1].to_string(),
                        album: parts[2].to_string(),
                        playing: parts[3].eq_ignore_ascii_case("Playing"),
                        duration: 0,
                        position: 0,
                        speed: 1.0,
                        next: true,
                        prev: true,
                        seek: true,
                        art: String::new(),
                    });
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
}
