// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust: Phase 2 Async Networking & Mesh Engine
// Replaces Node.js WebSocket with native async Tokio + Tungstenite (zero-GC latency, bounded backpressure).

use crate::state::{AppState, PeerInfo};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

pub const DEFAULT_PORT: u16 = 47800;
pub const DISCOVERY_PORT: u16 = 47801;
pub const BEACON_INTERVAL: Duration = Duration::from_millis(2000);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(1000);
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(12000);

pub fn current_os() -> &'static str {
    #[cfg(target_os = "macos")]
    return "mac";
    #[cfg(target_os = "linux")]
    return "linux";
    #[cfg(target_os = "windows")]
    return "win";
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return "win";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessage {
    pub t: String,
    #[serde(default)]
    pub v: Option<u32>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub role: Option<String>,

    // Mesh Pooling Stamps (Pool)
    #[serde(default)]
    pub o: Option<String>,         // Origin device id
    #[serde(default)]
    pub s: Option<u64>,            // Origin sequence number
    #[serde(default)]
    pub d: Option<Vec<String>>,    // Direct links of the origin
    #[serde(default)]
    pub h: Option<u32>,            // Hop count
    #[serde(default)]
    pub target: Option<String>,    // Targeted device ID (e.g. phone remote)

    // Input / KVM fields
    #[serde(default)]
    pub dx: Option<f64>,
    #[serde(default)]
    pub dy: Option<f64>,
    #[serde(default)]
    pub b: Option<i32>,
    #[serde(default)]
    pub k: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub vk: Option<u32>,
    #[serde(default)]
    pub mods: Option<u32>,
    #[serde(default)]
    pub smooth: Option<bool>,

    // Clipboard text & image fields
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub ts: Option<u64>,
    #[serde(default)]
    pub png: Option<String>,       // Base64 PNG picture
    #[serde(default)]
    pub w: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub bytes: Option<usize>,
    #[serde(default)]
    pub parts: Option<u32>,
    #[serde(default)]
    pub shot: Option<bool>,

    // File transfer batch broadcast
    #[serde(default)]
    pub files: Option<Vec<String>>,
}

impl Default for WireMessage {
    fn default() -> Self {
        Self {
            t: String::new(),
            v: Some(1),
            id: None,
            name: None,
            os: None,
            port: None,
            role: None,
            o: None,
            s: None,
            d: None,
            h: None,
            target: None,
            dx: None,
            dy: None,
            b: None,
            k: None,
            code: None,
            vk: None,
            mods: None,
            smooth: None,
            text: None,
            ts: None,
            png: None,
            w: None,
            height: None,
            bytes: None,
            parts: None,
            shot: None,
            files: None,
        }
    }
}

impl WireMessage {
    #[allow(dead_code)]
    pub fn new(t: &str) -> Self {
        Self {
            t: t.to_string(),
            ..Default::default()
        }
    }
}

pub struct HeartbeatTracker {
    last_received: Instant,
}

impl HeartbeatTracker {
    pub fn new() -> Self {
        Self {
            last_received: Instant::now(),
        }
    }

    pub fn record_ping(&mut self) {
        self.last_received = Instant::now();
    }

    pub fn is_stale(&self) -> bool {
        self.last_received.elapsed() > HEARTBEAT_TIMEOUT
    }
}

/// UDP LAN Discovery: broadcasts our beacon and discovers nearby GlideDesk peers.
pub struct DiscoveryService {
    app: AppHandle,
    state: AppState,
    local_id: String,
    local_name: String,
    port: u16,
}

impl DiscoveryService {
    pub fn new(app: AppHandle, state: AppState, local_id: String, local_name: String, port: u16) -> Self {
        Self {
            app,
            state,
            local_id,
            local_name,
            port,
        }
    }

    pub fn start(self: Arc<Self>) {
        let s1 = Arc::clone(&self);
        tauri::async_runtime::spawn(async move {
            s1.run_listener().await;
        });

        let s2 = Arc::clone(&self);
        tauri::async_runtime::spawn(async move {
            s2.run_broadcaster().await;
        });

        let s3 = Arc::clone(&self);
        tauri::async_runtime::spawn(async move {
            s3.run_pruner().await;
        });
    }

    async fn run_listener(&self) {
        let bind_addr = SocketAddr::from(([0, 0, 0, 0], DISCOVERY_PORT));
        let socket = match UdpSocket::bind(bind_addr).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[Discovery] Failed to bind primary UDP port {}: {}. Trying random port...", DISCOVERY_PORT, e);
                match UdpSocket::bind("0.0.0.0:0").await {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("[Discovery] Failed to bind any UDP socket: {}", e);
                        return;
                    }
                }
            }
        };

        let _ = socket.set_broadcast(true);
        let mut buf = [0u8; 4096];
        log::info!("[Discovery] UDP listener running on {:?}", socket.local_addr());

        loop {
            if let Ok((len, src_addr)) = socket.recv_from(&mut buf).await {
                if let Ok(msg) = serde_json::from_slice::<WireMessage>(&buf[..len]) {
                    if msg.id.as_deref() == Some(&self.local_id) {
                        continue; // Ignore our own beacon
                    }

                    match msg.t.as_str() {
                        "beacon" => {
                            if let (Some(id), Some(name)) = (msg.id, msg.name) {
                                let peer = PeerInfo {
                                    id,
                                    name,
                                    os: msg.os.unwrap_or_else(|| "unknown".to_string()),
                                    address: Some(src_addr.ip().to_string()),
                                    port: msg.port.or(Some(DEFAULT_PORT)),
                                    last_seen: None,
                                    invited: None,
                                };
                                if self.state.update_found_peer(peer) {
                                    let snap = self.state.snapshot();
                                    let _ = self.app.emit("state-change", &snap);
                                }
                            }
                        }
                        "probe" => {
                            // Unicast answer to probe
                            let reply = WireMessage {
                                t: "beacon".to_string(),
                                v: Some(1),
                                id: Some(self.local_id.clone()),
                                name: Some(self.local_name.clone()),
                                os: Some(current_os().to_string()),
                                port: Some(self.port),
                                role: Some("host".to_string()),
                                ..Default::default()
                            };
                            if let Ok(bytes) = serde_json::to_vec(&reply) {
                                let _ = socket.send_to(&bytes, src_addr).await;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    async fn run_broadcaster(&self) {
        let socket = match UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(e) => {
                log::error!("[Discovery] Broadcaster failed to bind: {}", e);
                return;
            }
        };
        let _ = socket.set_broadcast(true);
        let broadcast_addr: SocketAddr = "255.255.255.255:47801".parse().unwrap();

        let beacon = WireMessage {
            t: "beacon".to_string(),
            v: Some(1),
            id: Some(self.local_id.clone()),
            name: Some(self.local_name.clone()),
            os: Some(current_os().to_string()),
            port: Some(self.port),
            role: Some("host".to_string()),
            ..Default::default()
        };

        let mut interval = tokio::time::interval(BEACON_INTERVAL);
        loop {
            interval.tick().await;
            if let Ok(bytes) = serde_json::to_vec(&beacon) {
                let _ = socket.send_to(&bytes, broadcast_addr).await;
                for ip_str in crate::state::local_ip_addresses() {
                    if let Ok(ip) = ip_str.parse::<std::net::Ipv4Addr>() {
                        let octets = ip.octets();
                        let subnet_bcast = SocketAddr::from(([octets[0], octets[1], octets[2], 255], DISCOVERY_PORT));
                        let _ = socket.send_to(&bytes, subnet_bcast).await;
                    }
                }
            }
        }
    }

    async fn run_pruner(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(3000));
        loop {
            interval.tick().await;
            if self.state.prune_stale_peers(10000) {
                let snap = self.state.snapshot();
                let _ = self.app.emit("state-change", &snap);
            }
        }
    }
}

/// Active client connection to a remote host or guest peer in the mesh.
pub struct ActiveClientSession {
    pub id: String,
    pub peer_id: Arc<RwLock<Option<String>>>,
    pub peer_addr: SocketAddr,
    pub tx: mpsc::Sender<WireMessage>,
}

/// Mesh WebSocket Server listening for incoming KVM sessions.
pub struct MeshServer {
    app: AppHandle,
    state: AppState,
    port: u16,
    local_id: String,
    local_name: String,
    active_clients: Arc<RwLock<HashMap<String, ActiveClientSession>>>,
}

impl MeshServer {
    pub fn new(
        app: AppHandle,
        state: AppState,
        port: u16,
        local_id: String,
        local_name: String,
        active_clients: Arc<RwLock<HashMap<String, ActiveClientSession>>>,
    ) -> Self {
        Self {
            app,
            state,
            port,
            local_id,
            local_name,
            active_clients,
        }
    }

    pub fn start(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = self.run().await {
                log::warn!("[MeshServer] Server stopped: {}", e);
            }
        });
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                log::error!("[MeshServer] TCP port {} is already in use (is TwinDesk or another instance running?).", self.port);
                let _ = self.app.emit("port-conflict", serde_json::json!({
                    "port": self.port,
                    "protocol": "tcp",
                    "message": format!("TCP Port {} is already in use (e.g. TwinDesk is active). Please close the conflicting application.", self.port),
                }));
                return Err(e.into());
            }
            Err(e) => return Err(e.into()),
        };
        log::info!("[MeshServer] WebSocket server listening on {}", addr);

        while let Ok((stream, peer_addr)) = listener.accept().await {
            let app = self.app.clone();
            let state = self.state.clone();
            let local_id = self.local_id.clone();
            let local_name = self.local_name.clone();
            let active_clients = Arc::clone(&self.active_clients);
            let port = self.port;

            tauri::async_runtime::spawn(async move {
                if let Err(e) = Self::handle_session(stream, peer_addr, app, state, local_id, local_name, port, active_clients).await {
                    log::info!("[MeshServer] Peer {} disconnected: {}", peer_addr, e);
                }
            });
        }

        Ok(())
    }

    async fn handle_session(
        stream: TcpStream,
        peer_addr: SocketAddr,
        app: AppHandle,
        state: AppState,
        local_id: String,
        local_name: String,
        port: u16,
        active_clients: Arc<RwLock<HashMap<String, ActiveClientSession>>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let ws = accept_async(stream).await?;
        let (mut write, mut read) = ws.split();

        let session_id = peer_addr.to_string();
        let peer_id_holder = Arc::new(RwLock::new(None));
        let (out_tx, mut out_rx) = mpsc::channel::<WireMessage>(2048);
        {
            let mut guard = active_clients.write();
            guard.insert(
                session_id.clone(),
                ActiveClientSession {
                    id: session_id.clone(),
                    peer_id: Arc::clone(&peer_id_holder),
                    peer_addr,
                    tx: out_tx.clone(),
                },
            );
        }

        // Send welcome
        let welcome = WireMessage {
            t: "welcome".to_string(),
            v: Some(1),
            id: Some(local_id),
            name: Some(local_name),
            os: Some(current_os().to_string()),
            port: Some(port),
            role: Some("host".to_string()),
            ..Default::default()
        };
        let welcome_str = serde_json::to_string(&welcome)?;
        write.send(Message::Text(welcome_str)).await?;

        let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
        let mut tracker = HeartbeatTracker::new();

        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if tracker.is_stale() {
                        log::warn!("[MeshServer] Peer {} heartbeat timeout (12s silence)", peer_addr);
                        break;
                    }
                    let ping = WireMessage {
                        t: "ping".to_string(),
                        v: Some(1),
                        ..Default::default()
                    };
                    if let Ok(json) = serde_json::to_string(&ping) {
                        let _ = write.send(Message::Text(json)).await;
                    }
                }

                Some(pkt) = out_rx.recv() => {
                    if let Ok(json) = serde_json::to_string(&pkt) {
                        if write.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }

                Some(msg_res) = read.next() => {
                    match msg_res {
                        Ok(msg) => {
                            tracker.record_ping();
                            match msg {
                                Message::Text(txt) => {
                                    if let Ok(pkt) = serde_json::from_str::<WireMessage>(&txt) {
                                        let is_bye = pkt.t == "bye";
                                        Self::dispatch_incoming(&pkt, &peer_addr, &app, &state, &peer_id_holder).await;
                                        if is_bye {
                                            break;
                                        }
                                    }
                                }
                                Message::Binary(bin) => {
                                    if let Ok(pkt) = serde_json::from_slice::<WireMessage>(&bin) {
                                        let is_bye = pkt.t == "bye";
                                        Self::dispatch_incoming(&pkt, &peer_addr, &app, &state, &peer_id_holder).await;
                                        if is_bye {
                                            break;
                                        }
                                    }
                                }
                                Message::Ping(payload) => {
                                    let _ = write.send(Message::Pong(payload)).await;
                                }
                                Message::Close(_) => break,
                                _ => {}
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        let recorded_peer_id = peer_id_holder.read().clone();
        {
            let mut guard = active_clients.write();
            guard.remove(&session_id);
        }
        if let Some(ref pid) = recorded_peer_id {
            state.remove_client(pid);
        }
        state.remove_client(&session_id);
        state.remove_client(&peer_addr.ip().to_string());
        state.set_where_input("home");
        let snap = state.snapshot();
        let _ = app.emit("state-change", &snap);

        // Auto-recovery: Restore all parked windows if link drops or peer heartbeat times out
        let window_reg = app.state::<crate::window_manager::ParkedWindowRegistry>();
        let restored = window_reg.restore_all();
        if !restored.is_empty() {
            log::info!("[WindowHandoff] Server disconnect: restored {} parked window(s)", restored.len());
        }

        Ok(())
    }

    async fn dispatch_incoming(
        pkt: &WireMessage,
        peer_addr: &SocketAddr,
        app: &AppHandle,
        state: &AppState,
        peer_id_holder: &Arc<RwLock<Option<String>>>,
    ) {
        match pkt.t.as_str() {
            "hello" | "welcome" => {
                if let Some(ref pid) = pkt.id {
                    *peer_id_holder.write() = Some(pid.clone());
                }
                let addr_str = if peer_addr.ip().is_unspecified() || peer_addr.ip().is_loopback() {
                    None
                } else {
                    Some(peer_addr.ip().to_string())
                };
                let info = PeerInfo {
                    id: pkt.id.clone().unwrap_or_else(|| peer_addr.to_string()),
                    name: pkt.name.clone().unwrap_or_else(|| "Remote Peer".to_string()),
                    os: pkt.os.clone().unwrap_or_else(|| "unknown".to_string()),
                    address: addr_str,
                    port: pkt.port.or(Some(peer_addr.port())),
                    last_seen: Some(chrono::Utc::now().timestamp_millis() as u64),
                    invited: None,
                };
                state.add_client(info);
                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
            }
            "bye" => {
                if let Some(ref pid) = pkt.id {
                    state.remove_client(pid);
                }
                state.remove_client(&peer_addr.ip().to_string());
                state.set_where_input("home");
                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
                let window_reg = app.state::<crate::window_manager::ParkedWindowRegistry>();
                let _ = window_reg.restore_all();
            }
            "k.mm" | "remote.mm" => {
                // Inbound relative mouse / phone remote move with sub-pixel preservation
                if let (Some(dx), Some(dy)) = (pkt.dx, pkt.dy) {
                    let ix = dx.round() as i32;
                    let iy = dy.round() as i32;
                    let final_x = if ix == 0 && dx.abs() > 0.001 {
                        if dx > 0.0 { 1 } else { -1 }
                    } else {
                        ix
                    };
                    let final_y = if iy == 0 && dy.abs() > 0.001 {
                        if dy > 0.0 { 1 } else { -1 }
                    } else {
                        iy
                    };
                    crate::kvm::NativeInputInjector::move_cursor_relative(final_x, final_y);
                }
            }
            "k.md" | "remote.md" => {
                // Mouse down
                if let Some(b) = pkt.b {
                    let btn = match b {
                        0 => crate::kvm::MouseButton::Left,
                        1 => crate::kvm::MouseButton::Right,
                        _ => crate::kvm::MouseButton::Middle,
                    };
                    crate::kvm::NativeInputInjector::mouse_button(btn, true);
                }
            }
            "k.mu" | "remote.mu" => {
                // Mouse up
                if let Some(b) = pkt.b {
                    let btn = match b {
                        0 => crate::kvm::MouseButton::Left,
                        1 => crate::kvm::MouseButton::Right,
                        _ => crate::kvm::MouseButton::Middle,
                    };
                    crate::kvm::NativeInputInjector::mouse_button(btn, false);
                }
            }
            "k.wh" | "remote.wh" => {
                // Mouse wheel / trackpad scroll
                let dx = pkt.dx.unwrap_or(0.0) as i32;
                let dy = pkt.dy.unwrap_or(0.0) as i32;
                crate::kvm::NativeInputInjector::mouse_wheel(dx, dy);
            }
            "k.kd" | "remote.kd" => {
                if let Some(vk) = pkt.vk {
                    crate::kvm::NativeInputInjector::key_event(vk as u16, true, false);
                }
            }
            "k.ku" | "remote.ku" => {
                if let Some(vk) = pkt.vk {
                    crate::kvm::NativeInputInjector::key_event(vk as u16, false, false);
                }
            }
            "clip" => {
                if let Some(ref text) = pkt.text {
                    let mut cb = crate::kvm::NativeClipboard::new();
                    cb.set_text(text);
                }
            }
            "clip.image" => {
                if let Some(ref b64) = pkt.png {
                    use base64::Engine;
                    if let Ok(png_bytes) = base64::prelude::BASE64_STANDARD.decode(b64) {
                        let mut cb = crate::kvm::NativeClipboard::new();
                        cb.set_image_png(&png_bytes);
                    }
                }
            }
            "kvm.take" => {
                state.set_where_input("driven");
                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
            }
            "kvm.home" | "kvm.release" => {
                state.set_where_input("home");
                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
            }
            "window.handoff.offer" => {
                if let (Some(ref wid), Some(ref wname)) = (&pkt.id, &pkt.name) {
                    let w = pkt.w.unwrap_or(1024);
                    let h = pkt.height.unwrap_or(768);
                    let payload = serde_json::json!({
                        "id": wid,
                        "title": wname,
                        "width": w,
                        "height": h,
                        "peer": peer_addr.to_string(),
                    });
                    let _ = app.emit("window-handoff-received", &payload);

                    if let Some(win) = app.get_webview_window("projection") {
                        let _ = win.set_title(&format!("GlideDesk — {}", wname));
                        let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                            width: (w as f64).clamp(320.0, 3840.0),
                            height: (h as f64).clamp(240.0, 2160.0),
                        }));
                        let _ = win.show();
                        let _ = win.set_focus();
                        let _ = win.emit("projection-setup", &payload);
                    }
                }
            }
            "window.handoff.return" => {
                if let Some(ref wid) = pkt.id {
                    let window_reg = app.state::<crate::window_manager::ParkedWindowRegistry>();
                    window_reg.unpark(wid);
                    let _ = app.emit("window-handoff-returned", serde_json::json!({ "id": wid }));
                    if let Some(win) = app.get_webview_window("projection") {
                        let _ = win.hide();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Motion coalescer accumulating sub-millisecond mouse deltas during high frequency moves
/// or backpressure on the network channel.
#[derive(Debug, Default)]
pub struct MotionCoalescer {
    pub accum_dx: f64,
    pub accum_dy: f64,
}

impl MotionCoalescer {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn add(&mut self, dx: f64, dy: f64) {
        self.accum_dx += dx;
        self.accum_dy += dy;
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.accum_dx = 0.0;
        self.accum_dy = 0.0;
    }

    pub fn pending(&self) -> bool {
        self.accum_dx.abs() > 0.001 || self.accum_dy.abs() > 0.001
    }

    pub fn take(&mut self) -> (f64, f64) {
        let res = (self.accum_dx, self.accum_dy);
        self.accum_dx = 0.0;
        self.accum_dy = 0.0;
        res
    }
}

#[derive(Clone)]
pub struct NetworkManager {
    app: AppHandle,
    state: AppState,
    active_clients: Arc<RwLock<HashMap<String, ActiveClientSession>>>,
    motion: Arc<Mutex<MotionCoalescer>>,
    local_id: String,
    local_name: String,
    seq: Arc<Mutex<u64>>,
}

impl NetworkManager {
    pub fn new(app: AppHandle, state: AppState) -> Self {
        let local_id = state.inner.read().about.pc.host.to_lowercase();
        let local_name = state.inner.read().about.pc.host.clone();

        Self {
            app,
            state,
            active_clients: Arc::new(RwLock::new(HashMap::new())),
            motion: Arc::new(Mutex::new(MotionCoalescer::new())),
            local_id,
            local_name,
            seq: Arc::new(Mutex::new(chrono::Utc::now().timestamp_millis() as u64)),
        }
    }

    pub fn start(&self) {
        let local_id = self.local_id.clone();
        let local_name = self.local_name.clone();

        // 1. Start UDP discovery
        let discovery = Arc::new(DiscoveryService::new(
            self.app.clone(),
            self.state.clone(),
            local_id.clone(),
            local_name.clone(),
            DEFAULT_PORT,
        ));
        discovery.start();

        // 2. Start WebSocket server
        let server = Arc::new(MeshServer::new(
            self.app.clone(),
            self.state.clone(),
            DEFAULT_PORT,
            local_id,
            local_name,
            Arc::clone(&self.active_clients),
        ));
        server.start();
    }

    pub async fn connect_to(&self, target_addr: &str) -> Result<(), String> {
        let clean_addr = target_addr.trim_start_matches("ws://").trim_start_matches("wss://");
        let target_ip = clean_addr.split(':').next().unwrap_or(clean_addr);

        // Avoid duplicate concurrent connection if already connected to this peer
        {
            let guard = self.active_clients.read();
            if guard.values().any(|c| c.peer_addr.ip().to_string() == target_ip || c.id == target_addr) {
                log::info!("[NetworkManager] Already connected to {}", target_addr);
                return Ok(());
            }
        }

        let url = if target_addr.starts_with("ws://") || target_addr.starts_with("wss://") {
            target_addr.to_string()
        } else if target_addr.contains(':') {
            format!("ws://{}", target_addr)
        } else {
            format!("ws://{}:{}", target_addr, DEFAULT_PORT)
        };

        log::info!("[NetworkManager] Connecting to {}", url);
        let (ws, _) = connect_async(&url).await.map_err(|e| format!("Failed to connect: {}", e))?;
        let (mut write, mut read) = ws.split();

        let resolved_addr: SocketAddr = target_addr
            .trim_start_matches("ws://")
            .trim_start_matches("wss://")
            .parse::<SocketAddr>()
            .or_else(|_| {
                let host = target_addr.split(':').next().unwrap_or(target_addr);
                format!("{}:{}", host, DEFAULT_PORT).parse::<SocketAddr>()
            })
            .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

        let session_id = target_addr.to_string();
        let peer_id_holder = Arc::new(RwLock::new(None));
        let (out_tx, mut out_rx) = mpsc::channel::<WireMessage>(2048);
        {
            let mut guard = self.active_clients.write();
            guard.insert(
                session_id.clone(),
                ActiveClientSession {
                    id: session_id.clone(),
                    peer_id: Arc::clone(&peer_id_holder),
                    peer_addr: resolved_addr,
                    tx: out_tx.clone(),
                },
            );
        }

        // Send hello
        let hello = WireMessage {
            t: "hello".to_string(),
            v: Some(1),
            id: Some(self.local_id.clone()),
            name: Some(self.local_name.clone()),
            os: Some(current_os().to_string()),
            port: Some(DEFAULT_PORT),
            role: Some("guest".to_string()),
            ..Default::default()
        };
        let hello_str = serde_json::to_string(&hello).map_err(|e| e.to_string())?;
        write.send(Message::Text(hello_str)).await.map_err(|e| e.to_string())?;

        let app = self.app.clone();
        let state = self.state.clone();
        let clients_lock = Arc::clone(&self.active_clients);
        let cur_id = session_id.clone();
        let pid_holder = Arc::clone(&peer_id_holder);

        tauri::async_runtime::spawn(async move {
            let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
            let mut tracker = HeartbeatTracker::new();

            loop {
                tokio::select! {
                    _ = heartbeat.tick() => {
                        if tracker.is_stale() {
                            log::warn!("[MeshClient] Remote peer heartbeat timeout (12s silence)");
                            break;
                        }
                        let ping = WireMessage {
                            t: "ping".to_string(),
                            v: Some(1),
                            ..Default::default()
                        };
                        if let Ok(json) = serde_json::to_string(&ping) {
                            let _ = write.send(Message::Text(json)).await;
                        }
                    }

                    Some(pkt) = out_rx.recv() => {
                        if let Ok(json) = serde_json::to_string(&pkt) {
                            if write.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                    }

                    Some(msg_res) = read.next() => {
                        match msg_res {
                            Ok(msg) => {
                                tracker.record_ping();
                                match msg {
                                    Message::Text(txt) => {
                                        if let Ok(pkt) = serde_json::from_str::<WireMessage>(&txt) {
                                            let is_bye = pkt.t == "bye";
                                            MeshServer::dispatch_incoming(&pkt, &resolved_addr, &app, &state, &pid_holder).await;
                                            if is_bye {
                                                break;
                                            }
                                        }
                                    }
                                    Message::Binary(bin) => {
                                        if let Ok(pkt) = serde_json::from_slice::<WireMessage>(&bin) {
                                            let is_bye = pkt.t == "bye";
                                            MeshServer::dispatch_incoming(&pkt, &resolved_addr, &app, &state, &pid_holder).await;
                                            if is_bye {
                                                break;
                                            }
                                        }
                                    }
                                    Message::Ping(payload) => {
                                        let _ = write.send(Message::Pong(payload)).await;
                                    }
                                    Message::Close(_) => break,
                                    _ => {}
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }

            let recorded_peer_id = pid_holder.read().clone();
            {
                let mut guard = clients_lock.write();
                guard.remove(&cur_id);
            }
            if let Some(ref pid) = recorded_peer_id {
                state.remove_client(pid);
            }
            state.remove_client(&cur_id);
            state.remove_client(&resolved_addr.ip().to_string());
            state.set_where_input("home");
            let snap = state.snapshot();
            let _ = app.emit("state-change", &snap);

            // Auto-recovery: Restore all parked windows if link drops or peer heartbeat times out
            let window_reg = app.state::<crate::window_manager::ParkedWindowRegistry>();
            let restored = window_reg.restore_all();
            if !restored.is_empty() {
                log::info!("[WindowHandoff] Client disconnect: restored {} parked window(s)", restored.len());
            }
        });

        Ok(())
    }

    /// Fast non-blocking relative motion dispatch with delta coalescing under backpressure.
    pub fn send_motion(&self, dx: f64, dy: f64) {
        let mut motion = self.motion.lock();
        motion.add(dx, dy);

        if motion.accum_dx.abs() >= 0.5 || motion.accum_dy.abs() >= 0.5 {
            let (cdx, cdy) = motion.take();
            let msg = WireMessage {
                t: "k.mm".to_string(),
                v: Some(1),
                dx: Some(cdx),
                dy: Some(cdy),
                ..Default::default()
            };
            self.send_packet_sync(msg);
        }
    }

    /// Flushes any pending accumulated motion prior to sending mouse clicks, wheel, or keystrokes.
    pub fn flush_motion(&self) {
        let mut motion = self.motion.lock();
        if !motion.pending() {
            return;
        }
        let (cdx, cdy) = motion.take();
        let msg = WireMessage {
            t: "k.mm".to_string(),
            v: Some(1),
            dx: Some(cdx),
            dy: Some(cdy),
            ..Default::default()
        };
        self.send_packet_sync(msg);
    }

    /// Broadcast event to all connected mesh peers.
    pub fn send_packet_sync(&self, mut pkt: WireMessage) {
        self.flush_motion();
        if pkt.o.is_none() {
            pkt.o = Some(self.local_id.clone());
            let mut s = self.seq.lock();
            *s += 1;
            pkt.s = Some(*s);
            let client_ids: Vec<String> = self.active_clients.read().keys().cloned().collect();
            pkt.d = Some(client_ids);
            pkt.h = Some(0);
        }

        let guard = self.active_clients.read();
        for client in guard.values() {
            let _ = client.tx.try_send(pkt.clone());
        }
    }

    /// Async send/broadcast to all active mesh peers.
    pub async fn send_packet(&self, mut pkt: WireMessage) {
        self.flush_motion();
        if pkt.o.is_none() {
            pkt.o = Some(self.local_id.clone());
            let mut s = self.seq.lock();
            *s += 1;
            pkt.s = Some(*s);
            let client_ids: Vec<String> = self.active_clients.read().keys().cloned().collect();
            pkt.d = Some(client_ids);
            pkt.h = Some(0);
        }

        let senders: Vec<mpsc::Sender<WireMessage>> = {
            let guard = self.active_clients.read();
            guard.values().map(|c| c.tx.clone()).collect()
        };

        for tx in senders {
            let _ = tx.send(pkt.clone()).await;
        }
    }

    /// Send to a specific peer in the mesh.
    #[allow(dead_code)]
    pub async fn send_to(&self, target_id: &str, pkt: WireMessage) {
        self.flush_motion();
        let tx_opt = {
            let guard = self.active_clients.read();
            guard.get(target_id).map(|c| c.tx.clone()).or_else(|| {
                guard.values().find(|c| {
                    c.id == target_id
                        || c.peer_id.read().as_deref() == Some(target_id)
                        || c.peer_addr.ip().to_string() == target_id
                }).map(|c| c.tx.clone())
            })
        };

        if let Some(tx) = tx_opt {
            let _ = tx.send(pkt).await;
        }
    }

    pub async fn disconnect(&self) {
        self.flush_motion();
        let bye = WireMessage {
            t: "bye".to_string(),
            v: Some(1),
            id: Some(self.local_id.clone()),
            ..Default::default()
        };
        self.send_packet(bye).await;

        {
            let mut guard = self.active_clients.write();
            guard.clear();
        }
        self.state.set_client(None);
        self.state.set_where_input("home");
        let snap = self.state.snapshot();
        let _ = self.app.emit("state-change", &snap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_message_roundtrip() {
        let original = WireMessage {
            t: "k.mm".to_string(),
            v: Some(1),
            dx: Some(12.5),
            dy: Some(-4.2),
            ..WireMessage::new("k.mm")
        };
        let serialized = serde_json::to_string(&original).expect("serialize wire message");
        let parsed: WireMessage = serde_json::from_str(&serialized).expect("deserialize wire message");
        assert_eq!(parsed.t, "k.mm");
        assert_eq!(parsed.dx, Some(12.5));
        assert_eq!(parsed.dy, Some(-4.2));
    }

    #[test]
    fn test_motion_coalescer_accumulation() {
        let mut coalescer = MotionCoalescer::new();
        assert!(!coalescer.pending());
        coalescer.add(1.5, 2.0);
        coalescer.add(0.5, -1.0);
        assert!(coalescer.pending());
        let (dx, dy) = coalescer.take();
        assert_eq!(dx, 2.0);
        assert_eq!(dy, 1.0);
        assert!(!coalescer.pending());
    }

    #[test]
    fn test_heartbeat_tracker_freshness() {
        let mut tracker = HeartbeatTracker::new();
        assert!(!tracker.is_stale());
        tracker.record_ping();
        assert!(!tracker.is_stale());
    }
}
