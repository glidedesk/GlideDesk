#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Active Scrcpy Session: connects TCP sockets, streams video frames, and transmits control packets.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use base64::Engine;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use crate::phone::adb::AdbClient;
use crate::phone::scrcpy::{parse_stream_header, ControlMessage, StreamHeader, SCRCPY_VERSION, DEVICE_SERVER_PATH};

pub struct ActivePhoneSession {
    pub serial: String,
    pub scid: String,
    pub port: u16,
    pub running: Arc<AtomicBool>,
    control_stream: Arc<Mutex<Option<TcpStream>>>,
    server_process: Arc<Mutex<Option<Child>>>,
}

impl ActivePhoneSession {
    pub async fn start(
        adb: &AdbClient,
        serial: &str,
        app: AppHandle,
    ) -> Result<Self, String> {
        let server_jar = adb.server_jar_path().ok_or_else(|| "scrcpy-server jar not found in bundle or vendor paths".to_string())?;

        // 1. Push scrcpy-server to phone
        adb.push(serial, server_jar, DEVICE_SERVER_PATH)?;

        // 2. Prepare unique session scid and temporary jar copy
        let scid = format!("{:08x}", rand::random::<u32>() & 0x7fffffff);
        let session_jar = format!("/data/local/tmp/twindesk-scrcpy-{}.jar", scid);
        let cp_cmd = format!("cp {} {}", DEVICE_SERVER_PATH, session_jar);
        let _ = adb.shell(serial, &cp_cmd);

        // 3. Setup adb forward for abstract socket
        let abstract_socket = format!("scrcpy_{}", scid);
        let forward_port = adb.forward(serial, &abstract_socket)?;
        log::info!("[PhoneADB] Forwarded {} to local port {}", abstract_socket, forward_port);

        // 4. Spawn scrcpy server on device
        let server_cmd = format!(
            "CLASSPATH={} app_process / com.genymobile.scrcpy.Server {} scid={} tunnel_forward=true video=true audio=false control=true max_size=1920 max_fps=30",
            session_jar, SCRCPY_VERSION, scid
        );
        let child = adb.spawn_shell(serial, &server_cmd)?;

        // 5. Connect to local forwarded port (video stream socket)
        let mut video_stream = None;
        for _ in 0..50 {
            if let Ok(mut s) = TcpStream::connect(("127.0.0.1", forward_port)) {
                let mut dummy = [0u8; 1];
                s.set_read_timeout(Some(Duration::from_millis(1500))).ok();
                if s.read_exact(&mut dummy).is_ok() {
                    video_stream = Some(s);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let video_sock = video_stream.ok_or_else(|| "Failed to connect to scrcpy video socket on phone".to_string())?;

        // 6. Connect second socket (control socket)
        let control_sock = TcpStream::connect(("127.0.0.1", forward_port)).ok();

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // 7. Spawn background thread for reading video stream
        std::thread::spawn(move || {
            let mut sock = video_sock;
            sock.set_read_timeout(Some(Duration::from_millis(2000))).ok();

            // Read 64 bytes device name
            let mut dev_name = [0u8; 64];
            if sock.read_exact(&mut dev_name).is_err() {
                return;
            }

            // Read 4 bytes video codec
            let mut codec_bytes = [0u8; 4];
            if sock.read_exact(&mut codec_bytes).is_err() {
                return;
            }

            let mut header_buf = [0u8; 12];
            while running_clone.load(Ordering::SeqCst) {
                if sock.read_exact(&mut header_buf).is_err() {
                    break;
                }

                match parse_stream_header(&header_buf) {
                    StreamHeader::Resolution { width, height } => {
                        log::info!("[Scrcpy] Device stream resolution: {}x{}", width, height);
                        let _ = app.emit("phone-screen-resolution", serde_json::json!({
                            "width": width,
                            "height": height,
                        }));
                    }
                    StreamHeader::Frame { pts, size } => {
                        if size > 0 && size < 10_000_000 {
                            let mut frame_buf = vec![0u8; size as usize];
                            if sock.read_exact(&mut frame_buf).is_ok() {
                                // Emit video frame to webview
                                let _ = app.emit("media-screen-received", serde_json::json!({
                                    "source": "phone",
                                    "pts": pts,
                                    "size": size,
                                    "data": base64::engine::general_purpose::STANDARD.encode(&frame_buf),
                                }));
                            }
                        }
                    }
                }
            }
            log::info!("[Scrcpy] Phone video stream reader finished");
        });

        Ok(Self {
            serial: serial.to_string(),
            scid,
            port: forward_port,
            running,
            control_stream: Arc::new(Mutex::new(control_sock)),
            server_process: Arc::new(Mutex::new(Some(child))),
        })
    }

    pub fn send_control(&self, msg: &[u8]) -> Result<(), String> {
        let mut guard = self.control_stream.lock();
        if let Some(ref mut s) = *guard {
            s.write_all(msg).map_err(|e| format!("Failed to send control message: {}", e))?;
            s.flush().map_err(|e| format!("Failed to flush control stream: {}", e))?;
            Ok(())
        } else {
            Err("Control socket is not connected".to_string())
        }
    }

    pub fn inject_key(&self, keycode: u32, is_down: bool) -> Result<(), String> {
        let action = if is_down { ControlMessage::ACTION_DOWN } else { ControlMessage::ACTION_UP };
        let msg = ControlMessage::inject_keycode(action, keycode, 0, 0);
        self.send_control(&msg)
    }

    pub fn inject_text(&self, text: &str) -> Result<(), String> {
        let msg = ControlMessage::inject_text(text);
        self.send_control(&msg)
    }

    pub fn back_or_screen_on(&self) -> Result<(), String> {
        let msg = ControlMessage::back_or_screen_on(ControlMessage::ACTION_DOWN);
        self.send_control(&msg)
    }

    pub fn stop(&self, adb: &AdbClient) {
        self.running.store(false, Ordering::SeqCst);
        let mut child_guard = self.server_process.lock();
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill();
        }
        let _ = adb.unforward(&self.serial, self.port);
    }
}
