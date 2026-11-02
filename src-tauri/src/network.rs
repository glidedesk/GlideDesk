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

pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .filter_map(|i| {
            if i + 2 <= hex.len() {
                u8::from_str_radix(&hex[i..i + 2], 16).ok()
            } else {
                None
            }
        })
        .collect()
}

pub fn random_hex(bytes_count: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; bytes_count];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn commit_of(nonce_hex: &str) -> String {
    use sha2::{Digest, Sha256};
    let bytes = hex_to_bytes(nonce_hex);
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

pub fn sas_code(fp: &str, nw_hex: &str, nm_hex: &str) -> String {
    use sha2::{Digest, Sha256};
    let norm_fp = fp.replace(':', "").to_lowercase();
    let fp_bytes = hex_to_bytes(&norm_fp);
    let nw_bytes = hex_to_bytes(nw_hex);
    let nm_bytes = hex_to_bytes(nm_hex);

    let mut hasher = Sha256::new();
    hasher.update(b"macshare-sas-v1");
    hasher.update(&fp_bytes);
    hasher.update(&nw_bytes);
    hasher.update(&nm_bytes);
    let digest = hasher.finalize();

    let val = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:06}", val % 1_000_000)
}

#[derive(Clone)]
pub struct PendingPairSession {
    pub role: String, // "host" | "guest"
    pub peer_id: String,
    pub peer_name: String,
    pub peer_os: String,
    pub nw: Option<String>,
    pub nm: Option<String>,
    pub commit: Option<String>,
    pub code: Option<String>,
    pub confirmed: bool,
    pub allowed: bool,
    pub tx: mpsc::Sender<WireMessage>,
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

    // Pairing & Layout fields
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(rename = "macId", default)]
    pub mac_id: Option<String>,
    #[serde(rename = "macName", default)]
    pub mac_name: Option<String>,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(rename = "pcSide", default)]
    pub pc_side: Option<String>,
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
            commit: None,
            nonce: None,
            token: None,
            mac_id: None,
            mac_name: None,
            side: None,
            pc_side: None,
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
    pending_pair: Arc<RwLock<Option<PendingPairSession>>>,
}

impl MeshServer {
    pub fn new(
        app: AppHandle,
        state: AppState,
        port: u16,
        local_id: String,
        local_name: String,
        active_clients: Arc<RwLock<HashMap<String, ActiveClientSession>>>,
        pending_pair: Arc<RwLock<Option<PendingPairSession>>>,
    ) -> Self {
        Self {
            app,
            state,
            port,
            local_id,
            local_name,
            active_clients,
            pending_pair,
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
            let pending_pair = Arc::clone(&self.pending_pair);
            let port = self.port;

            tauri::async_runtime::spawn(async move {
                if let Err(e) = Self::handle_session(stream, peer_addr, app, state, local_id, local_name, port, active_clients, pending_pair).await {
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
        _port: u16,
        active_clients: Arc<RwLock<HashMap<String, ActiveClientSession>>>,
        pending_pair: Arc<RwLock<Option<PendingPairSession>>>,
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
                                        Self::dispatch_incoming(
                                            &pkt,
                                            &peer_addr,
                                            &app,
                                            &state,
                                            &peer_id_holder,
                                            &pending_pair,
                                            &out_tx,
                                            &local_id,
                                            &local_name,
                                        ).await;
                                        if is_bye {
                                            break;
                                        }
                                    }
                                }
                                Message::Binary(bin) => {
                                    if let Ok(pkt) = serde_json::from_slice::<WireMessage>(&bin) {
                                        let is_bye = pkt.t == "bye";
                                        Self::dispatch_incoming(
                                            &pkt,
                                            &peer_addr,
                                            &app,
                                            &state,
                                            &peer_id_holder,
                                            &pending_pair,
                                            &out_tx,
                                            &local_id,
                                            &local_name,
                                        ).await;
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
        {
            let mut pair_guard = pending_pair.write();
            if let Some(ref sess) = *pair_guard {
                if sess.peer_id == session_id || sess.peer_id == peer_addr.ip().to_string() {
                    *pair_guard = None;
                    state.set_pairing(None);
                }
            }
        }
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

    pub async fn finish_host_pairing(
        state: &AppState,
        app: &AppHandle,
        pending_pair: &Arc<RwLock<Option<PendingPairSession>>>,
        local_id: &str,
        local_name: &str,
    ) {
        let session = {
            let mut guard = pending_pair.write();
            guard.take()
        };
        if let Some(sess) = session {
            let token = random_hex(16);
            {
                let mut s = state.inner.write();
                s.pairing = None;
                s.link.state = "connected".to_string();
                s.link.mac_name = Some(sess.peer_name.clone());
                s.link.peer_name = Some(sess.peer_name.clone());
                let peer = PeerInfo {
                    id: sess.peer_id.clone(),
                    name: sess.peer_name.clone(),
                    os: sess.peer_os.clone(),
                    address: None,
                    port: Some(DEFAULT_PORT),
                    last_seen: Some(chrono::Utc::now().timestamp_millis() as u64),
                    invited: None,
                };
                if !s.guests.iter().any(|g| g.id == sess.peer_id) {
                    s.guests.push(peer.clone());
                }
                s.paired = Some(crate::state::PairedPeer {
                    mac_id: sess.peer_id.clone(),
                    mac_name: sess.peer_name.clone(),
                    os: sess.peer_os.clone(),
                    address: None,
                });
                s.peer = Some(peer.clone());
                s.client = Some(peer);
            }
            let ok_msg = WireMessage {
                t: "pair.ok".to_string(),
                v: Some(1),
                token: Some(token),
                mac_id: Some(local_id.to_string()),
                mac_name: Some(local_name.to_string()),
                os: Some(current_os().to_string()),
                ..Default::default()
            };
            let _ = sess.tx.send(ok_msg).await;

            let welcome = WireMessage {
                t: "welcome".to_string(),
                v: Some(1),
                id: Some(local_id.to_string()),
                name: Some(local_name.to_string()),
                os: Some(current_os().to_string()),
                port: Some(DEFAULT_PORT),
                role: Some("host".to_string()),
                ..Default::default()
            };
            let _ = sess.tx.send(welcome).await;

            let snap = state.snapshot();
            let _ = app.emit("state-change", &snap);
        }
    }

    pub async fn dispatch_incoming(
        pkt: &WireMessage,
        peer_addr: &SocketAddr,
        app: &AppHandle,
        state: &AppState,
        peer_id_holder: &Arc<RwLock<Option<String>>>,
        pending_pair: &Arc<RwLock<Option<PendingPairSession>>>,
        out_tx: &mpsc::Sender<WireMessage>,
        local_id: &str,
        local_name: &str,
    ) {
        match pkt.t.as_str() {
            "pair.start" => {
                log::info!("[Pairing] Received pair.start from {:?}", pkt.name);
                let sender_id = pkt.id.clone().unwrap_or_else(|| peer_addr.to_string());
                let sender_name = pkt.name.clone().unwrap_or_else(|| "Remote Device".to_string());
                let sender_os = pkt.os.clone().unwrap_or_else(|| "unknown".to_string());
                let commit = pkt.commit.clone();

                if let Some(commit_val) = commit {
                    let nm = random_hex(16);
                    {
                        let mut pair_guard = pending_pair.write();
                        *pair_guard = Some(PendingPairSession {
                            role: "host".to_string(),
                            peer_id: sender_id.clone(),
                            peer_name: sender_name.clone(),
                            peer_os: sender_os.clone(),
                            nw: None,
                            nm: Some(nm.clone()),
                            commit: Some(commit_val),
                            code: None,
                            confirmed: false,
                            allowed: false,
                            tx: out_tx.clone(),
                        });
                    }

                    let nonce_msg = WireMessage {
                        t: "pair.nonce".to_string(),
                        v: Some(1),
                        nonce: Some(nm),
                        os: Some(current_os().to_string()),
                        ..Default::default()
                    };
                    let _ = out_tx.send(nonce_msg).await;
                }
            }
            "pair.nonce" => {
                log::info!("[Pairing] Received pair.nonce");
                let mut should_reveal = None;
                {
                    let mut pair_guard = pending_pair.write();
                    if let Some(ref mut session) = *pair_guard {
                        if session.role == "guest" {
                            if let (Some(ref nm_val), Some(ref nw_val)) = (&pkt.nonce, &session.nw) {
                                session.nm = Some(nm_val.clone());
                                let code = sas_code("", nw_val, nm_val);
                                session.code = Some(code.clone());
                                should_reveal = Some((nw_val.clone(), code, session.peer_name.clone(), session.peer_os.clone()));
                            }
                        }
                    }
                }

                if let Some((nw, code, name, os)) = should_reveal {
                    let reveal_msg = WireMessage {
                        t: "pair.reveal".to_string(),
                        v: Some(1),
                        nonce: Some(nw),
                        ..Default::default()
                    };
                    let _ = out_tx.send(reveal_msg).await;

                    state.set_pairing(Some(crate::state::PairingState {
                        phase: "code".to_string(),
                        role: "guest".to_string(),
                        mac_name: name,
                        os,
                        code,
                        allowed: false,
                        wired: false,
                        address: peer_addr.ip().to_string(),
                        error: String::new(),
                    }));
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            "pair.reveal" => {
                log::info!("[Pairing] Received pair.reveal");
                let mut verified = false;
                let mut code_res = String::new();
                let mut peer_name_res = String::new();
                let mut peer_os_res = String::new();

                {
                    let mut pair_guard = pending_pair.write();
                    if let Some(ref mut session) = *pair_guard {
                        if session.role == "host" {
                            if let (Some(ref nw_val), Some(ref nm_val), Some(ref commit_val)) = (&pkt.nonce, &session.nm, &session.commit) {
                                if commit_of(nw_val).eq_ignore_ascii_case(commit_val) {
                                    session.nw = Some(nw_val.clone());
                                    let code = sas_code("", nw_val, nm_val);
                                    session.code = Some(code.clone());
                                    verified = true;
                                    code_res = code;
                                    peer_name_res = session.peer_name.clone();
                                    peer_os_res = session.peer_os.clone();
                                } else {
                                    log::warn!("[Pairing] Nonce commitment mismatch!");
                                }
                            }
                        }
                    }
                }

                if verified {
                    state.set_pairing(Some(crate::state::PairingState {
                        phase: "request".to_string(),
                        role: "host".to_string(),
                        mac_name: peer_name_res,
                        os: peer_os_res,
                        code: code_res,
                        allowed: false,
                        wired: false,
                        address: peer_addr.ip().to_string(),
                        error: String::new(),
                    }));
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            "pair.confirm" => {
                log::info!("[Pairing] Received pair.confirm from peer");
                let mut should_finish = false;
                {
                    let mut pair_guard = pending_pair.write();
                    if let Some(ref mut session) = *pair_guard {
                        if session.role == "host" {
                            session.confirmed = true;
                            if session.allowed {
                                should_finish = true;
                            }
                        }
                    }
                }
                if should_finish {
                    Self::finish_host_pairing(state, app, pending_pair, local_id, local_name).await;
                }
            }
            "pair.ok" => {
                log::info!("[Pairing] Received pair.ok! Pairing successful.");
                let host_id = pkt.mac_id.clone().unwrap_or_else(|| peer_addr.to_string());
                let host_name = pkt.mac_name.clone().unwrap_or_else(|| "Remote Host".to_string());
                let host_os = pkt.os.clone().unwrap_or_else(|| "unknown".to_string());

                {
                    let mut s = state.inner.write();
                    s.pairing = None;
                    s.link.state = "connected".to_string();
                    s.link.mac_name = Some(host_name.clone());
                    s.link.peer_name = Some(host_name.clone());
                    let p_info = PeerInfo {
                        id: host_id.clone(),
                        name: host_name.clone(),
                        os: host_os.clone(),
                        address: Some(peer_addr.ip().to_string()),
                        port: Some(peer_addr.port()),
                        last_seen: Some(chrono::Utc::now().timestamp_millis() as u64),
                        invited: None,
                    };
                    s.peer = Some(p_info.clone());
                    s.client = Some(p_info);
                    s.paired = Some(crate::state::PairedPeer {
                        mac_id: host_id.clone(),
                        mac_name: host_name.clone(),
                        os: host_os.clone(),
                        address: Some(peer_addr.ip().to_string()),
                    });
                }
                {
                    let mut pair_guard = pending_pair.write();
                    *pair_guard = None;
                }
                let hello = WireMessage {
                    t: "hello".to_string(),
                    v: Some(1),
                    id: Some(local_id.to_string()),
                    name: Some(local_name.to_string()),
                    os: Some(current_os().to_string()),
                    port: Some(DEFAULT_PORT),
                    role: Some("guest".to_string()),
                    ..Default::default()
                };
                let _ = out_tx.send(hello).await;

                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
            }
            "pair.cancel" => {
                log::info!("[Pairing] Received pair.cancel");
                {
                    let mut pair_guard = pending_pair.write();
                    *pair_guard = None;
                }
                state.set_pairing(None);
                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
            }
            "kvm.layout" => {
                let side = pkt.side.clone().or_else(|| pkt.pc_side.clone());
                if let Some(s_val) = side {
                    log::info!("[KVM] Remote layout updated: side = {}", s_val);
                    {
                        let mut s = state.inner.write();
                        s.layout.side = s_val;
                    }
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
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
                state.set_pairing(None);

                if pkt.t == "hello" {
                    let welcome = WireMessage {
                        t: "welcome".to_string(),
                        v: Some(1),
                        id: Some(local_id.to_string()),
                        name: Some(local_name.to_string()),
                        os: Some(current_os().to_string()),
                        port: Some(DEFAULT_PORT),
                        role: Some("host".to_string()),
                        ..Default::default()
                    };
                    let _ = out_tx.send(welcome).await;
                }

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
    pending_pair: Arc<RwLock<Option<PendingPairSession>>>,
    motion: Arc<Mutex<MotionCoalescer>>,
    local_id: String,
    local_name: String,
    seq: Arc<Mutex<u64>>,
}

impl NetworkManager {
    pub fn new(app: AppHandle, state: AppState) -> Self {
        let local_id = state.inner.read().about.pc.host.to_lowercase();
        let local_name = state.inner.read().about.pc.host.clone();
        let pending_pair = Arc::new(RwLock::new(None));

        Self {
            app,
            state,
            active_clients: Arc::new(RwLock::new(HashMap::new())),
            pending_pair,
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
            Arc::clone(&self.pending_pair),
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
        let pending_pair = Arc::clone(&self.pending_pair);
        let local_id = self.local_id.clone();
        let local_name = self.local_name.clone();
        let client_tx = out_tx.clone();

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
                                            MeshServer::dispatch_incoming(
                                                &pkt,
                                                &resolved_addr,
                                                &app,
                                                &state,
                                                &pid_holder,
                                                &pending_pair,
                                                &client_tx,
                                                &local_id,
                                                &local_name,
                                            ).await;
                                            if is_bye {
                                                break;
                                            }
                                        }
                                    }
                                    Message::Binary(bin) => {
                                        if let Ok(pkt) = serde_json::from_slice::<WireMessage>(&bin) {
                                            let is_bye = pkt.t == "bye";
                                            MeshServer::dispatch_incoming(
                                                &pkt,
                                                &resolved_addr,
                                                &app,
                                                &state,
                                                &pid_holder,
                                                &pending_pair,
                                                &client_tx,
                                                &local_id,
                                                &local_name,
                                            ).await;
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

    pub async fn start_pair(&self, target_addr: &str) -> Result<(), String> {
        let clean_addr = target_addr.trim_start_matches("ws://").trim_start_matches("wss://");
        let target_ip = clean_addr.split(':').next().unwrap_or(clean_addr);

        // Drain any existing active client sessions to this peer before starting pairing
        let to_remove: Vec<String> = {
            let guard = self.active_clients.read();
            guard.iter()
                .filter(|(id, c)| c.peer_addr.ip().to_string() == target_ip || *id == target_addr)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in to_remove {
            let sess = {
                let mut guard = self.active_clients.write();
                guard.remove(&id)
            };
            if let Some(s) = sess {
                let bye = WireMessage { t: "bye".to_string(), v: Some(1), ..Default::default() };
                let _ = s.tx.send(bye).await;
            }
        }

        let url = if target_addr.starts_with("ws://") || target_addr.starts_with("wss://") {
            target_addr.to_string()
        } else if target_addr.contains(':') {
            format!("ws://{}", target_addr)
        } else {
            format!("ws://{}:{}", target_addr, DEFAULT_PORT)
        };

        log::info!("[NetworkManager] Pairing with {}", url);
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

        let nw = random_hex(16);
        let commit = commit_of(&nw);

        let (target_name, target_os) = {
            let s = self.state.inner.read();
            if let Some(peer) = s.found.iter().find(|p| p.id.eq_ignore_ascii_case(target_addr) || p.address.as_deref() == Some(target_ip)) {
                (peer.name.clone(), peer.os.clone())
            } else {
                (target_ip.to_string(), "unknown".to_string())
            }
        };

        {
            let mut guard = self.pending_pair.write();
            *guard = Some(PendingPairSession {
                role: "guest".to_string(),
                peer_id: target_addr.to_string(),
                peer_name: target_name.clone(),
                peer_os: target_os.clone(),
                nw: Some(nw),
                nm: None,
                commit: Some(commit.clone()),
                code: None,
                confirmed: false,
                allowed: false,
                tx: out_tx.clone(),
            });
        }

        self.state.set_pairing(Some(crate::state::PairingState {
            phase: "starting".to_string(),
            role: "guest".to_string(),
            mac_name: target_name,
            os: target_os,
            code: String::new(),
            allowed: false,
            wired: false,
            address: target_ip.to_string(),
            error: String::new(),
        }));
        let snap = self.state.snapshot();
        let _ = self.app.emit("state-change", &snap);

        let pair_start = WireMessage {
            t: "pair.start".to_string(),
            v: Some(1),
            id: Some(self.local_id.clone()),
            name: Some(self.local_name.clone()),
            os: Some(current_os().to_string()),
            commit: Some(commit),
            ..Default::default()
        };
        let start_str = serde_json::to_string(&pair_start).map_err(|e| e.to_string())?;
        write.send(Message::Text(start_str)).await.map_err(|e| e.to_string())?;

        let app = self.app.clone();
        let state = self.state.clone();
        let clients_lock = Arc::clone(&self.active_clients);
        let cur_id = session_id.clone();
        let pid_holder = Arc::clone(&peer_id_holder);
        let pending_pair = Arc::clone(&self.pending_pair);
        let local_id = self.local_id.clone();
        let local_name = self.local_name.clone();
        let client_tx = out_tx.clone();

        tauri::async_runtime::spawn(async move {
            let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
            let mut tracker = HeartbeatTracker::new();

            loop {
                tokio::select! {
                    _ = heartbeat.tick() => {
                        if tracker.is_stale() {
                            log::warn!("[NetworkManager] Peer {} heartbeat timeout", resolved_addr);
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
                                            MeshServer::dispatch_incoming(
                                                &pkt,
                                                &resolved_addr,
                                                &app,
                                                &state,
                                                &pid_holder,
                                                &pending_pair,
                                                &client_tx,
                                                &local_id,
                                                &local_name,
                                            ).await;
                                            if is_bye {
                                                break;
                                            }
                                        }
                                    }
                                    Message::Binary(bin) => {
                                        if let Ok(pkt) = serde_json::from_slice::<WireMessage>(&bin) {
                                            let is_bye = pkt.t == "bye";
                                            MeshServer::dispatch_incoming(
                                                &pkt,
                                                &resolved_addr,
                                                &app,
                                                &state,
                                                &pid_holder,
                                                &pending_pair,
                                                &client_tx,
                                                &local_id,
                                                &local_name,
                                            ).await;
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
            {
                let mut pair_guard = pending_pair.write();
                if let Some(ref sess) = *pair_guard {
                    if sess.peer_id == cur_id || sess.peer_id == resolved_addr.ip().to_string() {
                        *pair_guard = None;
                        state.set_pairing(None);
                    }
                }
            }
            let snap = state.snapshot();
            let _ = app.emit("state-change", &snap);

            let window_reg = app.state::<crate::window_manager::ParkedWindowRegistry>();
            let _ = window_reg.restore_all();
        });

        Ok(())
    }

    pub async fn respond_pair(&self, allowed: bool) -> Result<(), String> {
        if !allowed {
            let session = {
                let mut guard = self.pending_pair.write();
                guard.take()
            };
            if let Some(sess) = session {
                let cancel_msg = WireMessage {
                    t: "pair.cancel".to_string(),
                    v: Some(1),
                    ..Default::default()
                };
                let _ = sess.tx.send(cancel_msg).await;
            }
            self.state.set_pairing(None);
            let snap = self.state.snapshot();
            let _ = self.app.emit("state-change", &snap);
            return Ok(());
        }

        let mut should_finish = false;
        {
            let mut guard = self.pending_pair.write();
            if let Some(ref mut session) = *guard {
                if session.role == "host" {
                    session.allowed = true;
                    if session.confirmed {
                        should_finish = true;
                    }
                }
            }
        }
        if should_finish {
            MeshServer::finish_host_pairing(&self.state, &self.app, &self.pending_pair, &self.local_id, &self.local_name).await;
        } else {
            {
                let mut s = self.state.inner.write();
                if let Some(ref mut p) = s.pairing {
                    p.allowed = true;
                }
            }
            let snap = self.state.snapshot();
            let _ = self.app.emit("state-change", &snap);
        }
        Ok(())
    }

    pub async fn confirm_pair(&self) -> Result<(), String> {
        let tx = {
            let mut guard = self.pending_pair.write();
            if let Some(ref mut session) = *guard {
                if session.role == "guest" {
                    session.confirmed = true;
                    Some(session.tx.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(tx) = tx {
            let confirm_msg = WireMessage {
                t: "pair.confirm".to_string(),
                v: Some(1),
                ..Default::default()
            };
            let _ = tx.send(confirm_msg).await;

            {
                let mut s = self.state.inner.write();
                if let Some(ref mut p) = s.pairing {
                    p.phase = "waiting".to_string();
                }
            }
            let snap = self.state.snapshot();
            let _ = self.app.emit("state-change", &snap);
        }
        Ok(())
    }

    pub async fn cancel_pair(&self) -> Result<(), String> {
        let session = {
            let mut guard = self.pending_pair.write();
            guard.take()
        };
        if let Some(sess) = session {
            let cancel_msg = WireMessage {
                t: "pair.cancel".to_string(),
                v: Some(1),
                ..Default::default()
            };
            let _ = sess.tx.send(cancel_msg).await;
        }
        self.state.set_pairing(None);
        let snap = self.state.snapshot();
        let _ = self.app.emit("state-change", &snap);
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
        self.state.set_pairing(None);
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

    #[test]
    fn test_sas_code_generation_node_parity() {
        let code = sas_code("", "0123456789abcdef0123456789abcdef", "fedcba9876543210fedcba9876543210");
        assert_eq!(code, "286217");
        assert_eq!(code.len(), 6);
    }

    #[test]
    fn test_commit_of_verification() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let commit = commit_of(nonce);
        assert!(!commit.is_empty());
        assert_eq!(commit_of(nonce), commit);
    }
}
