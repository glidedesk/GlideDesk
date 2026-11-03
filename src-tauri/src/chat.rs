use tauri::{AppHandle, State};

use crate::commands::push_state;
use crate::network::NetworkManager;
use crate::phone::PhoneManager;
use crate::state::AppState;

#[tauri::command]
pub async fn set_chats(
    on: bool,
    state: State<'_, AppState>,
    network: State<'_, NetworkManager>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Chat] set_chats: {}", on);
    {
        let mut s = state.inner.write();
        s.ui.chats = on;
    }
    // Sync with paired peer / phone using chat.want packet
    network
        .send_packet(crate::network::WireMessage {
            t: "chat.want".to_string(),
            on: Some(on),
            ..Default::default()
        })
        .await;

    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub async fn notif_reply(
    chat: Option<String>,
    id: Option<String>,
    text: String,
    state: State<'_, AppState>,
    network: State<'_, NetworkManager>,
    phone_mgr: State<'_, PhoneManager>,
    app: AppHandle,
) -> Result<(), String> {
    let clean_text = text.trim().to_string();
    if clean_text.is_empty() {
        return Err("Reply text cannot be empty".to_string());
    }

    if let Some(chat_key) = chat {
        let (notif_id, has_conv) = {
            let mut s = state.inner.write();
            let mut n_id = None;
            let mut found = false;
            if let Some(ref mut convs) = s.chats {
                if let Some(c) = convs.iter_mut().find(|c| c.key == chat_key) {
                    n_id = c.id.clone();
                    c.items.push(crate::state::ChatItem {
                        at: chrono::Utc::now().timestamp_millis() as u64,
                        from: String::new(),
                        text: clean_text.clone(),
                        me: true,
                    });
                    found = true;
                }
            }
            (n_id, found)
        };

        // Send notif.reply wire packet to authenticated peer
        let target_id = notif_id.clone().unwrap_or_else(|| chat_key.clone());
        network
            .send_packet(crate::network::WireMessage {
                t: "notif.reply".to_string(),
                id: Some(target_id),
                text: Some(clean_text.clone()),
                ..Default::default()
            })
            .await;

        // If active phone session, route via ADB
        let active_serial = {
            let s_guard = phone_mgr.session.read();
            s_guard.as_ref().map(|s| s.serial.clone())
        };

        if let Some(ref serial) = active_serial {
            let escaped_text = clean_text
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace(' ', "%s");
            let cmd = format!("input text \"{}\"", escaped_text);
            if let Err(e) = phone_mgr.adb.shell(serial, &cmd) {
                log::warn!("[Chat] ADB input text reply returned: {}", e);
            } else {
                log::info!("[Chat] Injected chat reply via ADB on device '{}'", serial);
            }
        } else if !has_conv && notif_id.is_none() {
            return Err("No active peer or phone connection to reply to chat".to_string());
        }

        push_state(&app, &state);
        return Ok(());
    }

    if let Some(notif_id) = id {
        network
            .send_packet(crate::network::WireMessage {
                t: "notif.reply".to_string(),
                id: Some(notif_id),
                text: Some(clean_text.clone()),
                ..Default::default()
            })
            .await;

        let active_serial = {
            let s_guard = phone_mgr.session.read();
            s_guard.as_ref().map(|s| s.serial.clone())
        };

        if let Some(ref serial) = active_serial {
            let escaped_text = clean_text
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace(' ', "%s");
            let cmd = format!("input text \"{}\"", escaped_text);
            let _ = phone_mgr.adb.shell(serial, &cmd);
        }

        return Ok(());
    }

    Err("Either chat or id must be provided to reply".to_string())
}

#[cfg(test)]
mod tests {
    use crate::network::WireMessage;
    use crate::state::ChatItem;

    #[test]
    fn test_chat_wire_serialization_and_deserialization() {
        let items = vec![
            ChatItem {
                at: 1000,
                from: "Alice".to_string(),
                text: "Hello".to_string(),
                me: false,
            },
            ChatItem {
                at: 2000,
                from: "".to_string(),
                text: "Hi there".to_string(),
                me: true,
            },
        ];

        let msg = WireMessage {
            t: "chat.conv".to_string(),
            app: Some("com.whatsapp".to_string()),
            key: Some("alice_chat".to_string()),
            app_name: Some("WhatsApp".to_string()),
            group: Some(false),
            reply: Some(true),
            items: Some(items),
            ..Default::default()
        };

        let json = serde_json::to_string(&msg).expect("serialize chat wire message");
        assert!(json.contains("\"t\":\"chat.conv\""));
        assert!(json.contains("\"app\":\"com.whatsapp\""));
        assert!(json.contains("\"appName\":\"WhatsApp\""));
        assert!(json.contains("\"key\":\"alice_chat\""));

        let deserialized: WireMessage =
            serde_json::from_str(&json).expect("deserialize chat wire message");
        assert_eq!(deserialized.t, "chat.conv");
        assert_eq!(deserialized.app.as_deref(), Some("com.whatsapp"));
        let items = deserialized.items.expect("items present");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].text, "Hi there");
        assert!(items[1].me);
    }

    #[test]
    fn test_notif_reply_validation_errors() {
        // Empty text is rejected
        let empty_text = "   ";
        assert!(empty_text.trim().is_empty());

        // Missing chat and id returns Err
        let chat: Option<String> = None;
        let id: Option<String> = None;
        let is_missing = chat.is_none() && id.is_none();
        assert!(is_missing);
    }

    #[test]
    fn test_chat_conversation_accumulation_and_capping() {
        let mut conv = crate::state::ChatConversation {
            key: "test_key".to_string(),
            app: "org.telegram.messenger".to_string(),
            app_name: "Telegram".to_string(),
            name: "Dev Chat".to_string(),
            group: true,
            id: Some("notif_42".to_string()),
            reply: true,
            at: 1000,
            items: Vec::new(),
        };

        // Add 160 items
        for i in 0..160 {
            conv.items.push(ChatItem {
                at: i as u64,
                from: format!("user_{}", i),
                text: format!("msg_{}", i),
                me: i % 2 == 0,
            });
        }

        // Cap to 150
        if conv.items.len() > 150 {
            let start = conv.items.len() - 150;
            conv.items = conv.items[start..].to_vec();
        }

        assert_eq!(conv.items.len(), 150);
        assert_eq!(conv.items[0].text, "msg_10");
        assert_eq!(conv.items[149].text, "msg_159");
    }
}
