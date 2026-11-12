// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust: Phase 2 Async Networking & Mesh Engine
// Replaces Node.js WebSocket with native async Tokio + Tungstenite (zero-GC latency, bounded backpressure).

use crate::state::{AppState, PeerInfo};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::accept_async;

pub const DEFAULT_PORT: u16 = 47800;
pub const DISCOVERY_PORT: u16 = 47801;
pub const BEACON_INTERVAL: Duration = Duration::from_millis(2000);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(1000);
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(12000);

/// Pointer-motion coalescing.
const MOTION_BACKLOG_LIMIT: u64 = 64 * 1024;
const MOTION_MAX_WAIT: Duration = Duration::from_millis(20);
const MOTION_RETRY_MS: u64 = 2;

/// Peer ids are host names; macOS uses typographic apostrophes in some names while
/// the same computer can report a straight one elsewhere. Compare them leniently.
fn same_peer_id(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.trim()
            .chars()
            .map(|c| match c {
                '\u{2018}' | '\u{2019}' | '\u{02BC}' | '\u{2032}' => '\'',
                '\u{201C}' | '\u{201D}' => '"',
                _ => c,
            })
            .collect::<String>()
            .to_lowercase()
    }
    a.eq_ignore_ascii_case(b) || norm(a) == norm(b)
}

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

pub enum ServerStream {
    Plain(TcpStream),
    Tls(tokio_rustls::server::TlsStream<TcpStream>),
}

impl tokio::io::AsyncRead for ServerStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ServerStream::Plain(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            ServerStream::Tls(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for ServerStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            ServerStream::Plain(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            ServerStream::Tls(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ServerStream::Plain(s) => std::pin::Pin::new(s).poll_flush(cx),
            ServerStream::Tls(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ServerStream::Plain(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            ServerStream::Tls(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub enum ClientStream {
    Plain(TcpStream),
    Tls(tokio_rustls::client::TlsStream<TcpStream>),
}

impl tokio::io::AsyncRead for ClientStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ClientStream::Plain(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            ClientStream::Tls(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for ClientStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            ClientStream::Plain(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            ClientStream::Tls(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ClientStream::Plain(s) => std::pin::Pin::new(s).poll_flush(cx),
            ClientStream::Tls(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ClientStream::Plain(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            ClientStream::Tls(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub async fn dial_peer_client(
    target_addr: &str,
    expected_fingerprint: Option<&str>,
    peer_id: Option<&str>,
    allow_plaintext: bool,
) -> Result<(tokio_tungstenite::WebSocketStream<ClientStream>, Option<String>), String> {
    let decision = crate::transport::tls::decide_client_transport(
        target_addr,
        expected_fingerprint,
        allow_plaintext,
    )?;

    match decision {
        crate::transport::tls::ClientTransportDecision::Tls { wss_url, host, port, expected_fingerprint } => {
            let tcp_stream = TcpStream::connect((host.as_str(), port))
                .await
                .map_err(|e| format!("Failed to connect to {}:{}: {}", host, port, e))?;
            tcp_stream.set_nodelay(true).ok();

            let captured = Arc::new(parking_lot::Mutex::new(None));
            let client_config = crate::transport::tls::create_pinned_client_config_with_peer(
                expected_fingerprint.as_deref().unwrap_or(""),
                peer_id,
                Some(Arc::clone(&captured)),
            );

            let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
            let server_name = rustls::pki_types::ServerName::try_from("glidedesk.local")
                .map_err(|e| format!("Invalid TLS server name: {}", e))?;

            let tls_stream = match connector.connect(server_name, tcp_stream).await {
                Ok(s) => s,
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("certificate fingerprint changed") {
                        return Err(err_msg);
                    }
                    if !allow_plaintext {
                        return Err("peer does not support encrypted transport; refusing plaintext".to_string());
                    }
                    log::warn!(
                        "[NetworkManager] TLS handshake failed ({}); falling back to plaintext because allow_plaintext=true",
                        e
                    );
                    let plain_tcp = TcpStream::connect((host.as_str(), port)).await
                        .map_err(|pe| format!("Plaintext fallback connect failed: {}", pe))?;
                    plain_tcp.set_nodelay(true).ok();
                    let ws_url = format!("ws://{}:{}", host, port);
                    let (ws, _) = tokio_tungstenite::client_async(ws_url, ClientStream::Plain(plain_tcp))
                        .await
                        .map_err(|we| format!("Plaintext WebSocket handshake failed: {}", we))?;
                    return Ok((ws, None));
                }
            };

            let (ws, _) = tokio_tungstenite::client_async(wss_url, ClientStream::Tls(tls_stream))
                .await
                .map_err(|e| format!("WebSocket handshake over TLS failed: {}", e))?;

            let established_fp = captured.lock().clone();
            Ok((ws, established_fp))
        }
        crate::transport::tls::ClientTransportDecision::Plaintext { ws_url, host, port } => {
            let tcp_stream = TcpStream::connect((host.as_str(), port))
                .await
                .map_err(|e| format!("Failed to connect to {}:{}: {}", host, port, e))?;
            tcp_stream.set_nodelay(true).ok();
            let (ws, _) = tokio_tungstenite::client_async(ws_url, ClientStream::Plain(tcp_stream))
                .await
                .map_err(|e| format!("WebSocket handshake failed: {}", e))?;
            Ok((ws, None))
        }
    }
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

/// The six digits both computers show, from inputs that are the same on both sides:
/// `host_fp` is the *host's* TLS certificate fingerprint — the host knows it locally, the guest
/// sees it on the wire during the handshake — `guest_nonce` is the guest's nonce (nw) and
/// `host_nonce` the host's (nm). Hashing each side's own fingerprint instead made the two
/// windows show different codes, so "Codes match" could never be pressed honestly.
pub fn pair_code(host_fp: &str, guest_nonce: &str, host_nonce: &str) -> String {
    sas_code(host_fp, guest_nonce, host_nonce)
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
    /// The peer's certificate fingerprint: on the guest, the one seen in the TLS handshake
    /// (the host's, whether or not its `pair.nonce` says the same); on the host, the one the
    /// guest claims. The pairing code hashes the host's real certificate, so a machine in the
    /// middle (whose certificate the guest would see instead) makes the two codes differ.
    pub peer_fp: Option<String>,
    pub code: Option<String>,
    pub confirmed: bool,
    pub allowed: bool,
    pub tx: mpsc::Sender<WireMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireFileItem {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
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
    #[serde(default)]
    pub fx: Option<i32>,           // Fast file transfer capability (fx2)

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
    // Key layout: `ch` is what the key types here, `tx` non-Latin
    // text, `im` the input method's language. All optional: older peers omit them.
    #[serde(default)]
    pub ch: Option<String>,
    #[serde(default)]
    pub tx: Option<String>,
    #[serde(default)]
    pub im: Option<String>,
    #[serde(default)]
    pub mods: Option<u32>,
    /// Where along the screen edge the other computer's pointer entered, 0..1.
    /// Older peers omit it; the pointer then starts in the middle of the edge.
    #[serde(default, rename = "edgeAt")]
    pub edge_at: Option<f64>,
    #[serde(default)]
    pub smooth: Option<bool>,

    // Clipboard text & image fields
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
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
    #[serde(default)]
    pub on: Option<bool>,

    // Chat / Messages fields
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(rename = "appName", default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub group: Option<bool>,
    #[serde(default)]
    pub reply: Option<bool>,
    #[serde(default)]
    pub items: Option<Vec<crate::state::ChatItem>>,

    // Notification fields: a notification on the sender (`notif`) and whether the
    // receiver wants ours (`notif.policy`). Older peers omit all of them.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub at: Option<u64>,
    #[serde(default)]
    pub accept: Option<bool>,

    // Wake-on-LAN fields
    #[serde(default)]
    pub macs: Option<Vec<String>>,
    #[serde(default)]
    pub address: Option<String>,

    // Terminal fields: `user` is the login name on the sending computer and `ssh`
    // the public key it offers so the receiver can let it in. Older peers omit both.
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub ssh: Option<String>,

    // Media & Device Action fields
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub pos: Option<u64>,

    // ReZume Shelf fields
    #[serde(default)]
    pub rezume_item: Option<crate::state::RezumeItem>,

    // TLS Certificate Fingerprint
    #[serde(default)]
    pub fingerprint: Option<String>,

    // File transfer protocol fields
    #[serde(default)]
    pub files: Option<Vec<String>>,
    #[serde(rename = "batchId", default)]
    pub batch_id: Option<String>,
    #[serde(rename = "fileId", default)]
    pub file_id: Option<String>,
    #[serde(rename = "batchFiles", default)]
    pub batch_files: Option<Vec<WireFileItem>>,

    // Shared folder announcement: which folder, on which port, behind which token.
    #[serde(rename = "sharedFolder", default)]
    pub shared_folder: Option<WireShareFolder>,
    #[serde(rename = "totalBytes", default)]
    pub total_bytes: Option<u64>,
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub len: Option<u64>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(rename = "sha256Chunk", default)]
    pub sha256_chunk: Option<String>,
    #[serde(rename = "sha256Total", default)]
    pub sha256_total: Option<String>,

    // Media & Screen streaming fields
    #[serde(default)]
    pub audio_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub frame_png: Option<Vec<u8>>,
    /// One screen frame, ready for `img.src` ("data:image/jpeg;base64,…"). Much
    /// smaller than `frame_png`, which is only kept for older peers.
    #[serde(rename = "frameB64", default)]
    pub frame_b64: Option<String>,
    #[serde(default)]
    pub fps: Option<u32>,

    // Screen sharing state (screen.state / screen.ctl)
    /// mirror | extend | off
    #[serde(default)]
    pub mode: Option<String>,
    /// Whether the sharing computer lets the other one start and switch it.
    #[serde(default)]
    pub remote: Option<bool>,
    /// The modes the sharing computer can be switched to.
    #[serde(default)]
    pub modes: Option<Vec<String>>,

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

/// What the other computer says about the folder it is sharing. Optional: older
/// peers never send it, and this one then simply says nothing is shared.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WireShareFolder {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub files: u32,
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
            fx: None,
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
            ch: None,
            tx: None,
            im: None,
            mods: None,
            edge_at: None,
            smooth: None,
            text: None,
            message: None,
            ts: None,
            png: None,
            w: None,
            height: None,
            bytes: None,
            parts: None,
            shot: None,
            on: None,
            app: None,
            key: None,
            app_name: None,
            group: None,
            reply: None,
            items: None,
            title: None,
            subtitle: None,
            body: None,
            at: None,
            accept: None,
            macs: None,
            address: None,
            user: None,
            ssh: None,
            action: None,
            pos: None,
            rezume_item: None,
            fingerprint: None,
            files: None,
            batch_id: None,
            file_id: None,
            batch_files: None,
            shared_folder: None,
            total_bytes: None,
            offset: None,
            len: None,
            data: None,
            sha256_chunk: None,
            sha256_total: None,
            commit: None,
            nonce: None,
            token: None,
            mac_id: None,
            mac_name: None,
            side: None,
            pc_side: None,
            audio_bytes: None,
            frame_png: None,
            frame_b64: None,
            fps: None,
            mode: None,
            remote: None,
            modes: None,
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

    fn bind_reuse_udp(bind_addr: SocketAddr) -> std::io::Result<UdpSocket> {
        let domain = match bind_addr {
            SocketAddr::V4(_) => socket2::Domain::IPV4,
            SocketAddr::V6(_) => socket2::Domain::IPV6,
        };
        let sock = socket2::Socket::new(domain, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
        let _ = sock.set_reuse_address(true);
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        let _ = sock.set_reuse_port(true);
        let _ = sock.set_broadcast(true);
        let _ = sock.set_nonblocking(true);
        sock.bind(&bind_addr.into())?;
        let std_socket: std::net::UdpSocket = sock.into();
        UdpSocket::from_std(std_socket)
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

        let s4 = Arc::clone(&self);
        tauri::async_runtime::spawn(async move {
            s4.send_probe().await;
        });
    }

    pub async fn send_probe(&self) {
        let socket = match UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[Discovery] Broadcaster probe failed to bind: {}", e);
                return;
            }
        };
        let _ = socket.set_broadcast(true);
        let broadcast_addr: SocketAddr = "255.255.255.255:47801".parse().unwrap();
        let probe = WireMessage {
            t: "probe".to_string(),
            v: Some(1),
            id: Some(self.local_id.clone()),
            name: Some(self.local_name.clone()),
            os: Some(current_os().to_string()),
            port: Some(self.port),
            role: Some("host".to_string()),
            ..Default::default()
        };
        if let Ok(bytes) = serde_json::to_vec(&probe) {
            log::info!("[Discovery] Broadcasting probe to discover nearby peers...");
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

    async fn run_listener(&self) {
        let bind_addr = SocketAddr::from(([0, 0, 0, 0], DISCOVERY_PORT));
        let socket = match Self::bind_reuse_udp(bind_addr) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[Discovery] Primary UDP bind with reuse failed on {}: {}. Trying default bind...", DISCOVERY_PORT, e);
                match UdpSocket::bind(bind_addr).await {
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
                                log::info!("[Discovery] Discovered nearby peer '{}' ({}) at {}:{:?}", name, msg.os.as_deref().unwrap_or("unknown"), src_addr.ip(), msg.port);
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
                            log::info!("[Discovery] Received probe from {:?}; replying with unicast beacon", src_addr);
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

/// Snapshot of active client metadata for routing and destination resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInfo {
    pub id: String,
    pub peer_id: Option<String>,
    pub peer_ip: String,
    pub authenticated: bool,
}

impl ClientInfo {
    pub fn new(id: impl Into<String>, peer_id: Option<String>, peer_ip: impl Into<String>, authenticated: bool) -> Self {
        Self {
            id: id.into(),
            peer_id,
            peer_ip: peer_ip.into(),
            authenticated,
        }
    }
}

/// Checks whether a packet destination list targets this local node.
///
/// `None` means the sender did not address the packet at all (legacy broadcast
/// semantics); callers decide whether to accept that.
pub fn is_addressed_to_local(dests: &Option<Vec<String>>, local_id: &str) -> bool {
    match dests {
        None => true,
        Some(list) => list.iter().any(|d| d == local_id),
    }
}

/// Receiver-side guard for `files.*` packets.
///
/// Addressed packets must name this node. Unaddressed packets are still accepted
/// for compatibility with senders that predate unicast addressing, but they are
/// logged so the remaining broadcast path is observable instead of silent.
fn file_packet_targets_local(pkt: &WireMessage, local_id: &str) -> bool {
    if pkt.d.is_none() {
        log::warn!(
            "[Files] {} has no destination list; accepting for legacy broadcast compatibility (local_id={})",
            pkt.t,
            local_id
        );
        return true;
    }
    is_addressed_to_local(&pkt.d, local_id)
}

/// Resolves a transfer target string to a routing destination list.
pub fn resolve_file_dest(target: Option<&str>, active: &[ClientInfo]) -> Result<Vec<String>, String> {
    if let Some(t) = target {
        if let Some(client) = active.iter().find(|c| {
            c.id == t
                || c.peer_id.as_deref() == Some(t)
                || c.peer_ip == t
        }) {
            if !client.authenticated {
                return Err(format!("target peer '{}' is not authenticated", t));
            }
            if let Some(ref pid) = client.peer_id {
                Ok(vec![pid.clone()])
            } else {
                Err(format!("target peer '{}' has not identified itself yet", t))
            }
        } else {
            Err(format!("target peer '{}' is not connected", t))
        }
    } else {
        // Two computers that dialled each other can hold two sessions at once, so
        // count peers, not sessions: one computer is still one destination.
        let mut peers: Vec<String> = Vec::new();
        let mut unidentified: Option<String> = None;
        for client in active.iter().filter(|c| c.authenticated) {
            match client.peer_id.as_deref().filter(|p| !p.is_empty()) {
                Some(pid) => {
                    if !peers.iter().any(|p| p == pid) {
                        peers.push(pid.to_string());
                    }
                }
                None => unidentified = Some(client.id.clone()),
            }
        }
        if peers.is_empty() {
            match unidentified {
                Some(id) => Err(format!("target peer '{}' has not identified itself yet", id)),
                None => Err("no peers connected".to_string()),
            }
        } else if peers.len() == 1 {
            Ok(peers)
        } else {
            Err("send_files requires a target peer (multiple peers connected)".to_string())
        }
    }
}

/// Which side this computer should sit on, given what the other computer just said.
///
/// `ours` is the edge we cross, `peer_claim` the edge the other computer says it
/// crosses. The two must be opposite: if the other computer is on our left, we are
/// on its right. When both computers claim the same edge (a fresh launch, or a
/// choice made on both sides at once) the computer with the smaller id keeps its
/// choice and the other one turns round, so a pair settles instead of swapping for
/// ever. Returns `Some(side)` only when our side has to change.
pub fn layout_side_to_adopt(
    local_id: &str,
    peer_id: &str,
    ours: &str,
    peer_claim: &str,
) -> Option<&'static str> {
    let expected = crate::commands::opposite_direction(peer_claim);
    if expected == ours {
        return None; // the two screens already meet at the right edge
    }
    if peer_claim == ours && local_id < peer_id {
        return None; // both claim the same edge: the smaller id keeps it
    }
    Some(expected)
}

/// Active client connection to a remote host or guest peer in the mesh.
pub struct ActiveClientSession {
    pub id: String,
    pub peer_id: Arc<RwLock<Option<String>>>,
    pub peer_addr: SocketAddr,
    pub tx: mpsc::Sender<WireMessage>,
    pub authenticated: Arc<RwLock<bool>>,
    /// True when this side dialled the other computer, false when it accepted the call.
    pub outbound: bool,
}

/// Which sessions to actually send on, out of a set that may hold two per peer.
///
/// Two computers can dial each other at the same moment, which leaves a pair of
/// sessions; sending on both would deliver every pointer move and keystroke twice.
/// Both ends keep the session dialled by the computer with the smaller id, a choice
/// each side can make on its own and still agree with the other.
pub fn preferred_session_ids(
    local_id: &str,
    clients: &HashMap<String, ActiveClientSession>,
) -> std::collections::HashSet<String> {
    use std::collections::HashMap as Map;
    let mut chosen: Map<String, &ActiveClientSession> = Map::new();
    for client in clients.values() {
        let peer_id = match client.peer_id.read().clone() {
            Some(pid) if !pid.is_empty() => pid,
            _ => continue, // has not introduced itself yet; it carries no input
        };
        let wanted = (local_id < peer_id.as_str()) == client.outbound;
        match chosen.get(&peer_id) {
            Some(current) => {
                let current_wanted = (local_id < peer_id.as_str()) == current.outbound;
                let better = if wanted != current_wanted {
                    wanted // the agreed direction beats a leftover
                } else {
                    client.id < current.id // a stable tie-break
                };
                if better {
                    chosen.insert(peer_id, client);
                }
            }
            None => {
                chosen.insert(peer_id, client);
            }
        }
    }

    let mut keep: std::collections::HashSet<String> =
        clients.values().map(|c| c.id.clone()).collect();
    for client in clients.values() {
        let Some(peer_id) = client.peer_id.read().clone().filter(|p| !p.is_empty()) else {
            continue;
        };
        if let Some(keep_client) = chosen.get(&peer_id) {
            if keep_client.id != client.id {
                keep.remove(&client.id);
            }
        }
    }
    keep
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
    tls_identity: Arc<crate::transport::tls::LocalTlsIdentity>,
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
        tls_identity: Arc<crate::transport::tls::LocalTlsIdentity>,
    ) -> Self {
        Self {
            app,
            state,
            port,
            local_id,
            local_name,
            active_clients,
            pending_pair,
            tls_identity,
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
                log::error!("[MeshServer] TCP port {} is already in use (is GlideDesk or another instance running?).", self.port);
                let _ = self.app.emit("port-conflict", serde_json::json!({
                    "port": self.port,
                    "protocol": "tcp",
                    "message": format!("TCP Port {} is already in use (e.g. GlideDesk is active). Please close the conflicting application.", self.port),
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
            let tls_identity = Arc::clone(&self.tls_identity);
            let port = self.port;

            tauri::async_runtime::spawn(async move {
                if let Err(e) = Self::handle_session(stream, peer_addr, app, state, local_id, local_name, port, active_clients, pending_pair, tls_identity).await {
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
        tls_identity: Arc<crate::transport::tls::LocalTlsIdentity>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut peek_buf = [0u8; 3];
        let is_tls = if let Ok(n) = stream.peek(&mut peek_buf).await {
            n >= 3 && peek_buf[0] == 0x16 && peek_buf[1] == 0x03
        } else {
            false
        };

        let allow_plaintext = state.inner.read().net.allow_plaintext;
        let server_stream = if is_tls {
            if let Ok(srv_cfg) = tls_identity.server_config() {
                let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(srv_cfg));
                match acceptor.accept(stream).await {
                    Ok(tls_s) => {
                        log::info!("[MeshServer] Accepted encrypted TLS session from {}", peer_addr);
                        ServerStream::Tls(tls_s)
                    }
                    Err(e) => {
                        log::warn!("[MeshServer] TLS handshake failed for {}: {}", peer_addr, e);
                        return Err(e.into());
                    }
                }
            } else {
                return Err("Failed to create TLS server config".into());
            }
        } else if allow_plaintext {
            log::warn!(
                "[MeshServer] INSECURE: Accepting unencrypted plaintext session from {} (allow_plaintext=true)",
                peer_addr
            );
            ServerStream::Plain(stream)
        } else {
            log::error!(
                "[MeshServer] Downgrade refused for {}: peer does not support encrypted transport; refusing plaintext",
                peer_addr
            );
            return Err("peer does not support encrypted transport; refusing plaintext".into());
        };

        let ws = accept_async(server_stream).await?;
        let (mut write, mut read) = ws.split();

        let session_id = peer_addr.to_string();
        let peer_id_holder = Arc::new(RwLock::new(None));
        let authenticated = Arc::new(RwLock::new(false));
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
                    authenticated: Arc::clone(&authenticated),
                    outbound: false,
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
                            log::warn!(
                                "[Net] server session {} write failed ({})",
                                peer_addr, pkt.t
                            );
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
                                            &authenticated,
                                            Some(&active_clients),
                                            &out_tx,
                                            &local_id,
                                            &local_name,
                                            &tls_identity.fingerprint_sha256,
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
                                            &authenticated,
                                            Some(&active_clients),
                                            &out_tx,
                                            &local_id,
                                            &local_name,
                                            &tls_identity.fingerprint_sha256,
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
        if let Some(net) = app.try_state::<NetworkManager>() {
            net.set_notif_accept(false);
        }
        let was_driving = state.inner.read().input == "driving";
        state.set_where_input("home");
        crate::kvm::NativeInputInjector::release_all_keys();
        if was_driving {
            crate::kvm::NativeInputInjector::recenter_local_cursor();
        }
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
        active_clients: Option<&Arc<RwLock<HashMap<String, ActiveClientSession>>>>,
        local_id: &str,
        local_name: &str,
        tls_fingerprint: &str,
    ) {
        let session = {
            let mut guard = pending_pair.write();
            guard.take()
        };
        if let Some(sess) = session {
            let raw_token = random_hex(16);
            let token = crate::transport::tls::format_beam_auth_token(&raw_token, tls_fingerprint);
            // The guest's address, so this side can reconnect on its own next launch.
            let mut peer_ip: Option<String> = None;
            if let Some(clients_map) = active_clients {
                let guard = clients_map.read();
                for client in guard.values() {
                    let pid = client.peer_id.read().clone();
                    if pid.as_deref() == Some(&sess.peer_id)
                        || client.id == sess.peer_id
                        || client.peer_addr.ip().to_string() == sess.peer_id
                    {
                        *client.authenticated.write() = true;
                        if peer_ip.is_none() {
                            peer_ip = Some(client.peer_addr.ip().to_string());
                        }
                    }
                }
            }
            {
                let mut s = state.inner.write();
                s.pairing = None;
                s.link.state = "connected".to_string();
                s.link.manual_off = false;
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
                let fp = s.peer_fingerprints.get(&sess.peer_id).cloned();
                let addr = peer_ip.clone().or_else(|| {
                    s.found
                        .iter()
                        .find(|f| same_peer_id(&f.id, &sess.peer_id))
                        .and_then(|f| f.address.clone())
                });
                s.paired = Some(crate::state::PairedPeer {
                    mac_id: sess.peer_id.clone(),
                    mac_name: sess.peer_name.clone(),
                    os: sess.peer_os.clone(),
                    address: addr,
                    token: Some(token.clone()),
                    tls_fingerprint: fp,
                });
                s.peer = Some(peer.clone());
                s.client = Some(peer);
            }
            // Paired for good: write it down so the next launch comes up paired.
            state.persist_pairs();
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

            // Tell the connecting guest which side we are on; it puts us on the opposite
            // side of its own screen.
            let side = state.inner.read().layout.side.clone();
            let layout_sync = WireMessage {
                t: "kvm.layout".to_string(),
                v: Some(1),
                side: Some(side.clone()),
                pc_side: Some(side),
                ..Default::default()
            };
            let _ = sess.tx.send(layout_sync).await;

            let snap = state.snapshot();
            let _ = app.emit("state-change", &snap);
        }
    }

    /// Inject one key on this computer. Kept in one place because macOS has to run both the
    /// input-method switch and the key itself on the main thread, in that order.
    fn inject_key(vk: u16, down: bool, ch: Option<String>, im: Option<String>) {
        log::debug!("[KVM] inject key vk={} down={}", vk, down);
        if down {
            let shortcut = crate::kvm::NativeInputInjector::ctrl_or_meta_held();
            // Only typing keys carry `ch`; an arrow key must not end an input method.
            if ch.is_some() {
                crate::inputmethod::incoming(im.as_deref(), shortcut);
            }
            let mapped = crate::keylayout::resolve(vk, ch.as_deref(), shortcut, true);
            crate::kvm::NativeInputInjector::key_event(mapped, true, false);
        } else {
            let mapped = crate::keylayout::resolve(vk, None, false, false);
            crate::kvm::NativeInputInjector::key_event(mapped, false, false);
        }
    }

    pub async fn dispatch_incoming(
        pkt: &WireMessage,
        peer_addr: &SocketAddr,
        app: &AppHandle,
        state: &AppState,
        peer_id_holder: &Arc<RwLock<Option<String>>>,
        pending_pair: &Arc<RwLock<Option<PendingPairSession>>>,
        authenticated: &Arc<RwLock<bool>>,
        active_clients: Option<&Arc<RwLock<HashMap<String, ActiveClientSession>>>>,
        out_tx: &mpsc::Sender<WireMessage>,
        local_id: &str,
        local_name: &str,
        tls_fingerprint: &str,
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
                            peer_fp: pkt.fingerprint.clone(),
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
                        fingerprint: Some(tls_fingerprint.to_string()),
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
                                // The host's real certificate (seen in the handshake); only a
                                // plaintext link, which has none, falls back to its claim.
                                let fp = session.peer_fp.as_deref()
                                    .filter(|f| !f.is_empty())
                                    .or(pkt.fingerprint.as_deref())
                                    .unwrap_or("");
                                let code = pair_code(fp, nw_val, nm_val);
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
                        fingerprint: Some(tls_fingerprint.to_string()),
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
                    crate::platform::set_window_visible_policy(&app, true);
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.unminimize();
                        let _ = win.set_focus();
                    }
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
                                    // The host's own certificate fingerprint, not the one the guest
                                    // sends in reveal: the guest hashes the same certificate, which
                                    // it saw during the handshake.
                                    let code = pair_code(tls_fingerprint, nw_val, nm_val);
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
                    crate::platform::set_window_visible_policy(&app, true);
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.unminimize();
                        let _ = win.set_focus();
                    }
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
                    Self::finish_host_pairing(state, app, pending_pair, active_clients, local_id, local_name, tls_fingerprint).await;
                }
            }
            "pair.ok" => {
                log::info!("[Pairing] Received pair.ok! Pairing successful.");
                let host_id = pkt.mac_id.clone().unwrap_or_else(|| peer_addr.to_string());
                let host_name = pkt.mac_name.clone().unwrap_or_else(|| "Remote Host".to_string());
                let host_os = pkt.os.clone().unwrap_or_else(|| "unknown".to_string());
                let host_token = pkt.token.clone();

                {
                    let mut s = state.inner.write();
                    s.pairing = None;
                    s.link.state = "connected".to_string();
                    s.link.manual_off = false;
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
                    let fp = s.peer_fingerprints.get(&host_id).cloned();
                    s.paired = Some(crate::state::PairedPeer {
                        mac_id: host_id.clone(),
                        mac_name: host_name.clone(),
                        os: host_os.clone(),
                        address: Some(peer_addr.ip().to_string()),
                        token: host_token.clone(),
                        tls_fingerprint: fp,
                    });
                }
                *authenticated.write() = true;
                // Paired for good: write it down so the next launch comes up paired.
                state.persist_pairs();
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
                    token: host_token,
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
                if let Some(peer_side) = side {
                    let peer_id = pkt.o.clone().unwrap_or_default();
                    let ours = state.inner.read().layout.side.clone();
                    match layout_side_to_adopt(local_id, &peer_id, &ours, &peer_side) {
                        None => {}
                        Some(new_side) => {
                            log::info!(
                                "[KVM] The other computer is on our {}, so we sit on its {}",
                                peer_side,
                                new_side
                            );
                            {
                                let mut s = state.inner.write();
                                s.layout.side = new_side.to_string();
                                s.kvm.pc_side = Some(peer_side.clone());
                            }
                            crate::pairstore::save_layout(new_side, state.inner.read().layout.along);
                            // Tell the other computer where we ended up, so both agree.
                            let sync = WireMessage {
                                t: "kvm.layout".to_string(),
                                v: Some(1),
                                side: Some(new_side.to_string()),
                                pc_side: Some(new_side.to_string()),
                                ..Default::default()
                            };
                            let _ = out_tx.send(sync).await;
                            let snap = state.snapshot();
                            let _ = app.emit("state-change", &snap);
                        }
                    }
                }
            }
            "media.audio" => {
                if !*authenticated.read() {
                    log::warn!("[Auth] Dropping unauthenticated media.audio packet from {}", peer_addr);
                } else if let Some(ref bytes) = pkt.audio_bytes {
                    let _ = app.emit("media-audio-received", bytes);
                }
            }
            "media.screen" => {
                if !*authenticated.read() {
                    log::warn!("[Auth] Dropping unauthenticated media.screen packet from {}", peer_addr);
                } else if let Some(ref data) = pkt.frame_b64 {
                    log::info!("[ScreenShare] frame received ({} bytes)", data.len());
                    let _ = app.emit(
                        "media-screen-received",
                        serde_json::json!({
                            "frame": data,
                            "w": pkt.w,
                            "h": pkt.height,
                            "fps": pkt.fps,
                        }),
                    );
                } else if let Some(ref frame) = pkt.frame_png {
                    // Older peers send the picture as bytes; wrap it the same way.
                    use base64::Engine;
                    let b64 = base64::prelude::BASE64_STANDARD.encode(frame);
                    let _ = app.emit(
                        "media-screen-received",
                        serde_json::json!({
                            "frame": format!("data:image/png;base64,{}", b64),
                            "w": pkt.w,
                            "h": pkt.height,
                        }),
                    );
                }
            }
            "files.folder" => {
                // What the other computer is offering to let this one read: a folder
                // name, a port and a one-time token. Turning it off withdraws the
                // address here as well.
                if !*authenticated.read() {
                    log::warn!(
                        "[Auth] Dropping unauthenticated files.folder packet from {}",
                        peer_addr
                    );
                } else {
                    let ip = peer_addr.ip().to_string();
                    let on = pkt.on.unwrap_or(false);
                    let (available, name, count) = match (on, pkt.shared_folder.clone()) {
                        (true, Some(info)) if info.port != 0 && !info.token.trim().is_empty() => {
                            crate::sharefolder::peer_shared(
                                &ip,
                                &info.name,
                                info.port,
                                &info.token,
                                info.files,
                            );
                            (true, info.name.clone(), info.files)
                        }
                        _ => {
                            crate::sharefolder::peer_cleared();
                            (false, String::new(), 0)
                        }
                    };
                    {
                        let mut s = state.inner.write();
                        s.files.smb.peer = Some(crate::state::SmbPeer {
                            mounted: available,
                            error: String::new(),
                            available,
                            name: name.clone(),
                            count,
                        });
                        s.files.smb.mount_message = if !available {
                            String::new()
                        } else {
                            let who = if name.is_empty() { "The other computer" } else { &name };
                            if count == 0 {
                                format!("{} is sharing an empty folder", who)
                            } else {
                                format!(
                                    "{} is sharing {} item{}",
                                    who,
                                    count,
                                    if count == 1 { "" } else { "s" }
                                )
                            }
                        };
                    }
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            "terminal.key" => {
                // The other computer asking to be let in. Add its key here, then answer
                // with this computer's login name and whether its SSH server is up —
                // that answer is what the other side's Open Terminal needs.
                if !*authenticated.read() {
                    log::warn!(
                        "[Auth] Dropping unauthenticated terminal.key packet from {}",
                        peer_addr
                    );
                } else if let Some(pubkey) = pkt.ssh.as_deref() {
                    let allow = state.inner.read().terminal.allow_mac;
                    // Say which computer is speaking, because this line is shown on the
                    // other computer's Terminal page.
                    let here = {
                        let host = state.inner.read().about.pc.host.clone();
                        if host.is_empty() {
                            "This computer".to_string()
                        } else {
                            host
                        }
                    };
                    let (granted, message) = if !allow {
                        (
                            false,
                            format!("{} does not let the other computer open a terminal.", here),
                        )
                    } else {
                        match crate::terminal::install_authorized_key(pubkey) {
                            Ok(()) => {
                                let (service, svc_message) = crate::terminal::sshd_service_cached();
                                let running = service == "running";
                                let accepted =
                                    running && crate::terminal::peer_key_accepted(pubkey);
                                {
                                    let mut s = state.inner.write();
                                    s.terminal.server.service = service;
                                    s.terminal.server.message = svc_message.clone();
                                    s.terminal.server.mac_asked = true;
                                    s.terminal.server.info.enabled = accepted;
                                }
                                if accepted {
                                    (true, String::new())
                                } else if running {
                                    (
                                        false,
                                        format!(
                                            "{} needs its one-time setup before a terminal can open there.",
                                            here
                                        ),
                                    )
                                } else {
                                    (false, format!("{}: {}", here, svc_message))
                                }
                            }
                            Err(e) => (false, e),
                        }
                    };
                    let reply = WireMessage {
                        t: "terminal.ok".to_string(),
                        v: Some(1),
                        on: Some(granted),
                        user: Some(crate::terminal::local_user()),
                        port: Some(22),
                        message: Some(message),
                        ..Default::default()
                    };
                    let _ = out_tx.send(reply).await;
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            "terminal.ok" => {
                // The other computer's answer: its login name when it will let us in,
                // otherwise why not.
                if !*authenticated.read() {
                    log::warn!(
                        "[Auth] Dropping unauthenticated terminal.ok packet from {}",
                        peer_addr
                    );
                } else {
                    let granted = pkt.on.unwrap_or(false);
                    let user = pkt.user.clone().unwrap_or_default();
                    let message = pkt.message.clone().unwrap_or_default();
                    {
                        let mut s = state.inner.write();
                        s.terminal.user = if granted { user } else { String::new() };
                        s.terminal.status = if granted {
                            "ready".to_string()
                        } else {
                            "unavailable".to_string()
                        };
                        s.terminal.message = message;
                    }
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            "screen.state" => {
                // What the other computer is sharing, so this page shows the same
                // thing its Share screen page shows.
                if !*authenticated.read() {
                    log::warn!("[Auth] Dropping unauthenticated screen.state packet from {}", peer_addr);
                } else {
                    let on = pkt.on.unwrap_or(false);
                    let mode = if on {
                        pkt.mode.clone().unwrap_or_else(|| "mirror".to_string())
                    } else {
                        "off".to_string()
                    };
                    let remote = pkt.remote.unwrap_or(true);
                    let modes = pkt
                        .modes
                        .clone()
                        .unwrap_or_else(|| vec!["mirror".to_string(), "extend".to_string()]);
                    {
                        let mut s = state.inner.write();
                        if let Some(ref name) = pkt.name {
                            if !name.is_empty() {
                                s.stream.name = name.clone();
                            }
                        }
                        s.stream.remote = Some(serde_json::json!({
                            "on": on,
                            "mode": mode,
                            "remote": remote,
                            "modes": modes,
                        }));
                    }
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            "screen.ctl" => {
                // The other computer asking to start, stop or switch this screen.
                if !*authenticated.read() {
                    log::warn!("[Auth] Dropping unauthenticated screen.ctl packet from {}", peer_addr);
                } else {
                    let want_on = pkt.on.unwrap_or(false);
                    let allowed = state.inner.read().pc_screen.remote;
                    if want_on && !allowed {
                        log::info!("[Sharing] Refused a screen request: remote start is off here");
                        // Say where we stand, so the other page stops waiting.
                        crate::streaming::announce_screen_state(state, app);
                    } else {
                        if want_on {
                            if let Some(ref mode) = pkt.mode {
                                if mode == "mirror" || mode == "extend" {
                                    state.inner.write().pc_screen.mode = mode.clone();
                                }
                            }
                        }
                        crate::streaming::apply_sharing(want_on, state, app);
                    }
                }
            }
            "hello" | "welcome" => {
                if let Some(ref pid) = pkt.id {
                    *peer_id_holder.write() = Some(pid.clone());
                }

                // Verify authorization strictly via pairing token
                let is_auth = {
                    let guard = state.inner.read();
                    if let Some(ref paired) = guard.paired {
                        let id_match = pkt.id.as_deref()
                            .map(|id| same_peer_id(id, &paired.mac_id))
                            .unwrap_or(false)
                            || peer_addr.ip().to_string() == paired.mac_id;
                        let token_match = match (&paired.token, &pkt.token) {
                            (Some(exp), Some(recv)) => {
                                exp == recv
                                    || exp.strip_prefix("BEAM_AUTH_V1:").and_then(|s| s.split(':').next()) == Some(recv.as_str())
                                    || recv.strip_prefix("BEAM_AUTH_V1:").and_then(|s| s.split(':').next()) == Some(exp.as_str())
                            }
                            _ => false,
                        };
                        id_match && token_match
                    } else {
                        false
                    }
                };

                if is_auth {
                    *authenticated.write() = true;
                    log::info!("[Auth] Peer {} ({:?}) authenticated successfully via pairing token", peer_addr, pkt.id);
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

                if is_auth {
                    state.add_client(info.clone());
                    state.set_pairing(None);
                    if let Some(ref macs) = pkt.macs {
                        crate::wol::record_peer_wol_info(
                            state,
                            &info.id,
                            &info.name,
                            &info.os,
                            macs.clone(),
                            info.address.as_deref(),
                        );
                    }
                } else {
                    log::warn!("[Auth] Unauthenticated hello/welcome from {} ({:?}) - not adding client", peer_addr, pkt.id);
                }

                if pkt.t == "hello" {
                    // Prove who we are as well: the peer checks this token against the
                    // pairing it stored, the same way we checked theirs. Without it, a
                    // reconnecting pair cannot authenticate each other from a bare welcome.
                    let our_token = state.inner.read().paired.as_ref().and_then(|p| p.token.clone());
                    let welcome = WireMessage {
                        t: "welcome".to_string(),
                        v: Some(1),
                        id: Some(local_id.to_string()),
                        name: Some(local_name.to_string()),
                        os: Some(current_os().to_string()),
                        port: Some(DEFAULT_PORT),
                        role: Some("host".to_string()),
                        token: our_token,
                        ..Default::default()
                    };
                    let _ = out_tx.send(welcome).await;

                    // Tell the connecting peer which side we are on; it flips it for itself.
                    let side = state.inner.read().layout.side.clone();
                    let layout_sync = WireMessage {
                        t: "kvm.layout".to_string(),
                        v: Some(1),
                        side: Some(side.clone()),
                        pc_side: Some(side),
                        ..Default::default()
                    };
                    let _ = out_tx.send(layout_sync).await;

                    // And what we share, so its Receive screen page starts out right.
                    let _ = out_tx
                        .send(crate::streaming::screen_state_message(state))
                        .await;
                }

                // Notifications: tell the other computer whether this one shows its.
                let accept = {
                    let s = state.inner.read();
                    s.notifs.enabled && s.notifs.receive
                };
                let _ = out_tx
                    .send(WireMessage {
                        t: "notif.policy".to_string(),
                        v: Some(1),
                        accept: Some(accept),
                        ..Default::default()
                    })
                    .await;

                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
            }
            "bye" => {
                if let Some(ref pid) = pkt.id {
                    state.remove_client(pid);
                }
                state.remove_client(&peer_addr.ip().to_string());
                if let Some(net) = app.try_state::<NetworkManager>() {
                    net.set_notif_accept(false);
                }
                let was_driving = state.inner.read().input == "driving";
                state.set_where_input("home");
                crate::kvm::NativeInputInjector::release_all_keys();
                if was_driving {
                    crate::kvm::NativeInputInjector::recenter_local_cursor();
                }
                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
                let window_reg = app.state::<crate::window_manager::ParkedWindowRegistry>();
                let _ = window_reg.restore_all();
            }
            "k.mm" | "remote.mm" | "k.md" | "remote.md" | "k.mu" | "remote.mu"
                | "k.wh" | "remote.wh" | "k.kd" | "remote.kd" | "k.ku" | "remote.ku"
                | "clip" | "clip.image" | "kvm.take" | "window.handoff.offer"
                | "window.handoff.return" | "gamepad.event" | "chat.conv" | "notif.reply"
                | "notif" | "notif.policy"
                | "chat.want" | "wol.info" | "phone.media" | "phone.media.ctl"
                | "rz.offer" | "rz.take" | "files.batch" | "files.chunk"
                | "files.done" | "files.ack" | "files.cancel" | "fclip.paste"
                if !*authenticated.read() =>
            {
                log::warn!(
                    "[Auth] Dropping unauthenticated packet '{}' from {}",
                    pkt.t,
                    peer_addr
                );
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
                // Typing arrives only while this computer is set to accept it.
                let accepted = {
                    let s = state.inner.read();
                    s.kvm.keyboard && s.kvm.linked && s.kvm.be_driven
                };
                if !accepted {
                    log::debug!("[KVM] Ignoring a key: keyboard sharing is off here");
                } else if let Some(vk) = pkt.vk {
                    // Ctrl+Alt+V is the file paste: ask the computer the files were
                    // copied on for them, and do not type the V into an app here.
                    if crate::fileclip::note_key(vk as u16, true) {
                        log::info!("[FileClip] Ctrl+Alt+V: asking the other computer for its copied files");
                        let ask = WireMessage {
                            t: "fclip.paste".to_string(),
                            v: Some(1),
                            ..Default::default()
                        };
                        let _ = out_tx.send(ask).await;
                    } else {
                        let ch = pkt.ch.clone();
                        let im = pkt.im.clone();
                        // The input-method switch and the key it applies to both run on the
                        // main thread, in that order: macOS requires it for its text-input
                        // services, and on Windows the worker threads are not guaranteed to
                        // be attached to the interactive desktop, which makes SendInput fail.
                        {
                            let _ = app.run_on_main_thread(move || {
                                Self::inject_key(vk as u16, true, ch, im)
                            });
                        }
                    }
                }
            }
            "k.ku" | "remote.ku" => {
                if let Some(vk) = pkt.vk {
                    // The release of the V that started a file paste is used up with it.
                    if !crate::fileclip::note_key(vk as u16, false) {
                        let _ = app.run_on_main_thread(move || {
                            Self::inject_key(vk as u16, false, None, None)
                        });
                    }
                }
            }
            "fclip.paste" => {
                // The other computer pressed Ctrl+Alt+V while its pointer is here:
                // the files copied on this computer travel over as a normal batch
                // transfer and land in the receiving computer's received folder.
                if !*authenticated.read() {
                    log::warn!(
                        "[Auth] Dropping unauthenticated fclip.paste packet from {}",
                        peer_addr
                    );
                } else {
                    let copied = crate::fileclip::clipboard_files();
                    if copied.is_empty() {
                        log::info!(
                            "[FileClip] Nothing to paste: no files are copied on this computer"
                        );
                    } else {
                        log::info!(
                            "[FileClip] Sending {} copied file(s) for the paste",
                            copied.len()
                        );
                        let app = app.clone();
                        let state = state.clone();
                        tauri::async_runtime::spawn(async move {
                            let network = app.state::<NetworkManager>().inner().clone();
                            let transfer = app
                                .state::<crate::transport::transfer::TransferManager>()
                                .inner()
                                .clone();
                            let phone = app.state::<crate::phone::PhoneManager>().inner().clone();
                            if let Err(e) = crate::commands::send_files_to_peer(
                                copied, None, &network, &state, &phone, &transfer, &app,
                            )
                            .await
                            {
                                log::warn!("[FileClip] paste failed: {}", e);
                            }
                        });
                    }
                }
            }
            "clip" => {
                if !*authenticated.read() {
                    log::warn!("[Auth] Dropping unauthenticated clip packet from {}", peer_addr);
                } else if let Some(ref text) = pkt.text {
                    let (on, receive, max_kb) = {
                        let s = state.inner.read();
                        (s.clipboard.enabled, s.clipboard.receive, s.clipboard.max_kb.max(1))
                    };
                    if !on || !receive {
                        log::debug!(
                            "[Clipboard] Ignoring incoming text: clipboard off or receive direction off"
                        );
                    } else if text.len() as u64 > max_kb as u64 * 1024 {
                        log::debug!(
                            "[Clipboard] Ignoring incoming text of {} bytes (limit {} KB)",
                            text.len(),
                            max_kb
                        );
                    } else {
                        let mut cb = crate::kvm::NativeClipboard::new();
                        cb.set_text(text);
                        // Let the watcher know this is not a fresh local copy.
                        crate::kvm::note_remote_clipboard_text(text);
                        if state.record_clipboard_item(text.clone(), "in", text.len() as u64, None) {
                            let snap = state.snapshot();
                            let _ = app.emit("state-change", &snap);
                        }
                    }
                }
            }
            "clip.image" => {
                if !*authenticated.read() {
                    log::warn!("[Auth] Dropping unauthenticated clip.image packet from {}", peer_addr);
                } else if let Some(ref b64) = pkt.png {
                    let (on, receive, images) = {
                        let s = state.inner.read();
                        (s.clipboard.enabled, s.clipboard.receive, s.clipboard.images)
                    };
                    if !on || !receive || !images {
                        log::debug!(
                            "[Clipboard] Ignoring incoming picture: off, direction off, or pictures off"
                        );
                    } else {
                        use base64::Engine;
                        if let Ok(png_bytes) = base64::prelude::BASE64_STANDARD.decode(b64) {
                            let mut cb = crate::kvm::NativeClipboard::new();
                            if cb.set_image_png(&png_bytes) {
                                crate::kvm::note_remote_clipboard_image(png_bytes.len());
                                let w = pkt.w.unwrap_or(0);
                                let h = pkt.height.unwrap_or(0);
                                let thumb = crate::kvm::clipboard_thumb(
                                    &png_bytes,
                                    w,
                                    h,
                                    pkt.shot.unwrap_or(false),
                                );
                                if state.record_clipboard_item(
                                    format!("Picture · {} × {}", w, h),
                                    "in",
                                    png_bytes.len() as u64,
                                    thumb,
                                ) {
                                    let snap = state.snapshot();
                                    let _ = app.emit("state-change", &snap);
                                }
                            }
                        }
                    }
                }
            }
            "kvm.take" => {
                // Only take the pointer when this computer is set to accept it: the
                // Cursor master switch, the "use the other computer's mouse here"
                // direction, and the overall link must all be on. Otherwise say so, so
                // the other computer hands control straight back instead of hiding its
                // pointer on a screen nobody is driving.
                let (accepted, driving_now) = {
                    let s = state.inner.read();
                    (
                        s.kvm.mouse && s.kvm.linked && s.kvm.be_driven,
                        s.input == "driving",
                    )
                };
                if driving_now {
                    // Both computers reached across at the same moment. The one already
                    // driving keeps control and the other goes home, so the pair cannot
                    // end up each waiting for the other with neither taking input.
                    log::info!("[KVM] Ignored the other computer's pointer: this computer is already driving it");
                    let home = WireMessage {
                        t: "kvm.home".to_string(),
                        v: Some(1),
                        ..Default::default()
                    };
                    let _ = out_tx.send(home).await;
                } else if !accepted {
                    log::info!("[KVM] Refused the other computer's pointer: cursor sharing is off here");
                    let home = WireMessage {
                        t: "kvm.home".to_string(),
                        v: Some(1),
                        ..Default::default()
                    };
                    let _ = out_tx.send(home).await;
                } else {
                    state.set_where_input("driven");
                    // The other computer counts movement from the edge it crossed and
                    // gives control back when the movement returns to that point, so
                    // this pointer has to start there too.
                    let snap = state.snapshot();
                    crate::kvm::NativeInputInjector::place_cursor_at_entry_edge(
                        &snap.layout.side,
                        pkt.edge_at.unwrap_or(0.5),
                    );
                    let _ = app.emit("state-change", &snap);
                }
            }
            "kvm.home" | "kvm.release" => {
                state.set_where_input("home");
                // A shortcut held over the link must not survive the pointer leaving,
                // or a later V here would turn into a file paste.
                crate::fileclip::reset();
                crate::kvm::NativeInputInjector::release_all_keys();
                // This computer may have followed the other pointer just past the edge.
                crate::kvm::NativeInputInjector::keep_cursor_visible();
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
            "gamepad.event" => {
                log::info!("[Gamepad] Inbound gamepad event from {}: button={:?}, pressed={:?}", peer_addr, pkt.k, pkt.b);
                let _ = app.emit("gamepad-event", &pkt);
            }
            "notif.policy" => {
                if *authenticated.read() {
                    crate::notifications::set_peer_policy(
                        app,
                        state,
                        pkt.accept.unwrap_or(false),
                    );
                }
            }
            "notif" => {
                if !*authenticated.read() {
                    log::warn!(
                        "[Auth] Dropping unauthenticated notif packet from {}",
                        peer_addr
                    );
                } else {
                    crate::notifications::incoming(app, state, pkt);
                }
            }
            "chat.conv" => {
                if let (Some(app_id), Some(key)) = (pkt.app.clone(), pkt.key.clone()) {
                    let mut s = state.inner.write();
                    let chats_vec = s.chats.get_or_insert_with(Vec::new);
                    let mut found = false;
                    for conv in chats_vec.iter_mut() {
                        if conv.key == key {
                            if let Some(ref items) = pkt.items {
                                for item in items {
                                    if !conv.items.iter().any(|x| x.text == item.text && (x.at as i64 - item.at as i64).abs() < 1500) {
                                        conv.items.push(item.clone());
                                    }
                                }
                                conv.items.sort_by_key(|i| i.at);
                                if conv.items.len() > 150 {
                                    let start = conv.items.len() - 150;
                                    conv.items = conv.items[start..].to_vec();
                                }
                            }
                            if let Some(ref app_name) = pkt.app_name {
                                conv.app_name = app_name.clone();
                            }
                            if let Some(ref name) = pkt.name {
                                conv.name = name.clone();
                            }
                            if let Some(ref id) = pkt.id {
                                conv.id = Some(id.clone());
                            }
                            if let Some(reply) = pkt.reply {
                                conv.reply = reply;
                            }
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        chats_vec.push(crate::state::ChatConversation {
                            key,
                            app: app_id,
                            app_name: pkt.app_name.clone().unwrap_or_default(),
                            name: pkt.name.clone().unwrap_or_default(),
                            group: pkt.group.unwrap_or(false),
                            id: pkt.id.clone(),
                            reply: pkt.reply.unwrap_or(false),
                            at: chrono::Utc::now().timestamp_millis() as u64,
                            items: pkt.items.clone().unwrap_or_default(),
                        });
                    }
                    drop(s);
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            "rz.offer" => {
                if let Some(item) = pkt.rezume_item.clone() {
                    let mut s = state.inner.write();
                    if s.rezume.on && s.rezume.receive {
                        s.rezume_shelf.incoming = Some(item.clone());
                        let exists = s.rezume_shelf.shelf.iter().any(|x| x.id == item.id);
                        if !exists {
                            s.rezume_shelf.shelf.insert(0, item);
                            if s.rezume_shelf.shelf.len() > 50 {
                                s.rezume_shelf.shelf.truncate(50);
                            }
                        }
                    }
                    drop(s);
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            "phone.media" => {
                let mut s = state.inner.write();
                if let Some(ref title) = pkt.name {
                    s.now_playing = Some(crate::state::NowPlayingState {
                        app: pkt.app.clone().unwrap_or_default(),
                        app_name: pkt.app_name.clone().unwrap_or_default(),
                        title: title.clone(),
                        artist: pkt.text.clone().unwrap_or_default(),
                        album: String::new(),
                        playing: pkt.on.unwrap_or(false),
                        duration: 0,
                        position: pkt.pos.unwrap_or(0),
                        speed: 1.0,
                        next: true,
                        prev: true,
                        seek: true,
                        art: pkt.png.clone().unwrap_or_default(),
                    });
                } else {
                    s.now_playing = None;
                }
                drop(s);
                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
            }
            "files.batch" => {
                if !file_packet_targets_local(pkt, local_id) {
                    log::warn!("[Files] Ignoring files.batch not addressed to local peer (dests={:?}, local_id={})", pkt.d, local_id);
                    return;
                }
                log::info!("[Files] Inbound files.batch from {}: batch_id={:?}", peer_addr, pkt.batch_id);
                let batch_id = pkt.batch_id.clone().or(pkt.id.clone())
                    .unwrap_or_else(|| format!("batch_{}", chrono::Utc::now().timestamp_millis()));
                let transfer_mgr = app.state::<crate::transport::transfer::TransferManager>();

                let batch = if let Some(ref bfiles) = pkt.batch_files {
                    let file_tuples: Vec<(String, u64, String, Option<String>)> = bfiles.iter().map(|f| {
                        (f.name.clone(), f.size, f.sha256.clone(), None)
                    }).collect();
                    let mut b = crate::transport::transfer::TransferBatch::new_with_checksums(
                        batch_id.clone(),
                        file_tuples,
                    );
                    let temp_dir = crate::transport::transfer::default_temp_dir();
                    for it in &mut b.items {
                        it.temp_path = Some(temp_dir.join(format!("{}_{}.part", batch_id, it.file_id)).to_string_lossy().to_string());
                    }
                    b
                } else if let Some(ref files) = pkt.files {
                    let file_tuples: Vec<(String, u64)> = files.iter().map(|f| (f.clone(), 1024)).collect();
                    crate::transport::transfer::TransferBatch::new(batch_id.clone(), file_tuples)
                } else {
                    crate::transport::transfer::TransferBatch::new(batch_id.clone(), Vec::new())
                };

                transfer_mgr.batches.write().insert(batch_id.clone(), batch);
                let _ = app.emit(
                    "files-batch-started",
                    &serde_json::json!({
                        "batch_id": batch_id,
                        "sender": pkt.name.clone().unwrap_or_else(|| peer_addr.to_string()),
                    }),
                );
            }
            "files.chunk" => {
                if !file_packet_targets_local(pkt, local_id) {
                    log::warn!("[Files] Ignoring files.chunk not addressed to local peer (dests={:?}, local_id={})", pkt.d, local_id);
                    return;
                }
                let batch_id = match pkt.batch_id.as_deref().or(pkt.id.as_deref()) {
                    Some(b) => b.to_string(),
                    None => return,
                };
                let file_id = match pkt.file_id.as_deref() {
                    Some(f) => f.to_string(),
                    None => return,
                };
                let offset = pkt.offset.unwrap_or(0);
                let chunk_len = pkt.len.unwrap_or(0);
                let data_b64 = match pkt.data.as_deref() {
                    Some(d) => d,
                    None => return,
                };
                let expected_chunk_hash = match pkt.sha256_chunk.as_deref() {
                    Some(h) => h.to_lowercase(),
                    None => return,
                };

                // 1. Decode base64
                let chunk_bytes = match base64::engine::general_purpose::STANDARD.decode(data_b64) {
                    Ok(b) => b,
                    Err(e) => {
                        log::error!("[Files] Failed to decode chunk base64: {}", e);
                        return;
                    }
                };

                if chunk_bytes.len() as u64 != chunk_len {
                    log::error!("[Files] Chunk len mismatch: expected {}, decoded {}", chunk_len, chunk_bytes.len());
                    return;
                }

                // 2. Cryptographic checksum verification
                use sha2::Digest;
                let mut hasher = sha2::Sha256::new();
                hasher.update(&chunk_bytes);
                let computed_hash = format!("{:x}", hasher.finalize());
                if computed_hash != expected_chunk_hash {
                    log::error!(
                        "[Files] Chunk sha256 mismatch! Expected {}, got {}",
                        expected_chunk_hash, computed_hash
                    );
                    return; // Corrupted chunk rejected; do not write to disk, do not ack
                }

                // 3. Write chunk into temp file at offset
                let transfer_mgr = app.state::<crate::transport::transfer::TransferManager>();
                let temp_path = {
                    let batches = transfer_mgr.batches.read();
                    batches.get(&batch_id).and_then(|b| {
                        b.items.iter().find(|it| it.file_id == file_id).and_then(|it| it.temp_path.clone())
                    })
                };

                let temp_path_buf = temp_path
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| {
                        crate::transport::transfer::default_temp_dir()
                            .join(format!("{}_{}.part", batch_id, file_id))
                    });

                use std::io::{Seek, SeekFrom, Write};
                let write_res = std::fs::OpenOptions::new()
                    .create(true)
                    .read(true)
                    .write(true)
                    .open(&temp_path_buf)
                    .and_then(|mut f| {
                        f.seek(SeekFrom::Start(offset))?;
                        f.write_all(&chunk_bytes)?;
                        f.flush()
                    });

                if let Err(e) = write_res {
                    log::error!("[Files] Failed to write chunk to {:?}: {}", temp_path_buf, e);
                    return;
                }

                // 4. Update progress
                let new_offset = offset + chunk_len;
                {
                    let mut batches = transfer_mgr.batches.write();
                    if let Some(b) = batches.get_mut(&batch_id) {
                        let _ = b.set_file_offset(&file_id, new_offset);
                    }
                }

                // 5. Reply files.ack
                let ack_pkt = WireMessage {
                    t: "files.ack".to_string(),
                    v: Some(1),
                    batch_id: Some(batch_id.clone()),
                    file_id: Some(file_id.clone()),
                    offset: Some(new_offset),
                    target: pkt.o.clone(),
                    ..Default::default()
                };
                let _ = out_tx.send(ack_pkt).await;

                let _ = app.emit(
                    "transfer-progress",
                    &serde_json::json!({
                        "batch_id": batch_id,
                        "file_id": file_id,
                        "offset": new_offset,
                    }),
                );
            }
            "files.done" => {
                if !file_packet_targets_local(pkt, local_id) {
                    log::warn!("[Files] Ignoring files.done not addressed to local peer (dests={:?}, local_id={})", pkt.d, local_id);
                    return;
                }
                let batch_id = match pkt.batch_id.as_deref().or(pkt.id.as_deref()) {
                    Some(b) => b.to_string(),
                    None => return,
                };
                let file_id = match pkt.file_id.as_deref() {
                    Some(f) => f.to_string(),
                    None => return,
                };
                let sender_total_hash = match pkt.sha256_total.as_deref() {
                    Some(h) => h.to_lowercase(),
                    None => return,
                };

                let transfer_mgr = app.state::<crate::transport::transfer::TransferManager>();
                let (filename, temp_path) = {
                    let batches = transfer_mgr.batches.read();
                    if let Some(b) = batches.get(&batch_id) {
                        if let Some(it) = b.items.iter().find(|i| i.file_id == file_id) {
                            (it.filename.clone(), it.temp_path.clone())
                        } else {
                            (file_id.clone(), None)
                        }
                    } else {
                        (file_id.clone(), None)
                    }
                };

                let temp_path_buf = temp_path
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| {
                        crate::transport::transfer::default_temp_dir()
                            .join(format!("{}_{}.part", batch_id, file_id))
                    });

                let computed_res = crate::transport::transfer::compute_file_sha256(&temp_path_buf);
                match computed_res {
                    Ok((_size, actual_total_hash)) => {
                        if actual_total_hash.to_lowercase() == sender_total_hash {
                            let chosen = {
                                let s = app.state::<crate::state::AppState>();
                                let dir = s.inner.read().files.received_folder.clone();
                                dir
                            };
                            let dl_dir =
                                crate::transport::transfer::effective_received_dir(&chosen);
                            let _ = std::fs::create_dir_all(&dl_dir);
                            let dest_path = dl_dir.join(&filename);
                            let _ = std::fs::rename(&temp_path_buf, &dest_path)
                                .or_else(|_| {
                                    std::fs::copy(&temp_path_buf, &dest_path)
                                        .and_then(|_| std::fs::remove_file(&temp_path_buf))
                                });

                            {
                                let mut batches = transfer_mgr.batches.write();
                                if let Some(b) = batches.get_mut(&batch_id) {
                                    let _ = b.verify_and_complete_file_path(
                                        &file_id,
                                        &dest_path,
                                        &sender_total_hash,
                                    );
                                }
                            }

                            // The Files page shows what arrived, so the person who was
                            // sent the file can find it again after the bar is gone.
                            {
                                let app_state = app.state::<crate::state::AppState>();
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0);
                                let name = dest_path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                {
                                    let mut s = app_state.inner.write();
                                    s.files.incoming.last = Some(crate::state::FilesEvent {
                                        ok: true,
                                        name,
                                        text: dest_path.to_string_lossy().to_string(),
                                        at: now,
                                    });
                                }
                                crate::commands::push_state(&app, &app_state);
                            }

                            let _ = app.emit(
                                "file-completed",
                                &serde_json::json!({
                                    "batch_id": batch_id,
                                    "file_id": file_id,
                                    "path": dest_path.to_string_lossy(),
                                    "checksum": sender_total_hash,
                                }),
                            );
                        } else {
                            log::error!(
                                "[Files] Total checksum mismatch for batch {} file {}! Sender hash: {}, computed: {}",
                                batch_id, file_id, sender_total_hash, actual_total_hash
                            );
                            let _ = std::fs::remove_file(&temp_path_buf);
                            {
                                let mut batches = transfer_mgr.batches.write();
                                if let Some(b) = batches.get_mut(&batch_id) {
                                    b.phase = crate::transport::transfer::TransferPhase::Failed;
                                }
                            }
                            let _ = app.emit(
                                "transfer-failed",
                                &serde_json::json!({
                                    "batch_id": batch_id,
                                    "file_id": file_id,
                                    "error": "Total checksum mismatch against sender-supplied hash",
                                }),
                            );
                        }
                    }
                    Err(e) => {
                        log::error!("[Files] Failed to compute total checksum: {}", e);
                        let _ = std::fs::remove_file(&temp_path_buf);
                    }
                }
            }
            "files.ack" => {
                if let (Some(ref b_id), Some(ref f_id), Some(ack_offset)) =
                    (&pkt.batch_id, &pkt.file_id, pkt.offset)
                {
                    let transfer_mgr = app.state::<crate::transport::transfer::TransferManager>();
                    let _ = transfer_mgr.ack_tx.send(crate::transport::transfer::AckNotification {
                        batch_id: b_id.clone(),
                        file_id: f_id.clone(),
                        offset: ack_offset,
                    });

                    {
                        let mut batches = transfer_mgr.batches.write();
                        if let Some(b) = batches.get_mut(b_id) {
                            let _ = b.set_file_offset(f_id, ack_offset);
                        }
                    }
                }
            }
            "files.cancel" => {
                if !file_packet_targets_local(pkt, local_id) {
                    log::warn!("[Files] Ignoring files.cancel not addressed to local peer (dests={:?}, local_id={})", pkt.d, local_id);
                    return;
                }
                if let Some(ref b_id) = pkt.batch_id {
                    let transfer_mgr = app.state::<crate::transport::transfer::TransferManager>();
                    transfer_mgr.cancel_batch(b_id);
                    let _ = app.emit("transfer-cancelled", &serde_json::json!({ "batch_id": b_id }));
                }
            }
            "wol.info" => {
                log::info!("[WoL] Inbound wol.info packet from {}", peer_addr);
                let sender_id = pkt
                    .id
                    .clone()
                    .or_else(|| peer_id_holder.read().clone())
                    .unwrap_or_else(|| peer_addr.ip().to_string());
                let sender_name = pkt.name.clone().unwrap_or_else(|| "Remote Peer".to_string());
                let sender_os = pkt.os.clone().unwrap_or_else(|| "unknown".to_string());
                let macs = pkt.macs.clone().unwrap_or_default();
                let address = pkt.address.clone().unwrap_or_else(|| peer_addr.ip().to_string());

                crate::wol::record_peer_wol_info(
                    state,
                    &sender_id,
                    &sender_name,
                    &sender_os,
                    macs,
                    Some(&address),
                );
                let snap = state.snapshot();
                let _ = app.emit("state-change", &snap);
            }
            "version_mismatch" => {
                // A host said it speaks a different protocol version: say which side to update.
                let name = state
                    .inner
                    .read()
                    .pairing
                    .as_ref()
                    .map(|p| p.mac_name.clone())
                    .unwrap_or_else(|| peer_addr.to_string());
                let text = pkt
                    .message
                    .clone()
                    .or_else(|| pkt.text.clone())
                    .unwrap_or_default();
                if let Some(reason) = crate::pairlimit::version_message(&text, &name) {
                    {
                        let mut s = state.inner.write();
                        if let Some(p) = s.pairing.as_mut() {
                            p.phase = "failed".to_string();
                            p.error = reason;
                            p.code.clear();
                            p.allowed = false;
                        }
                    }
                    let snap = state.snapshot();
                    let _ = app.emit("state-change", &snap);
                }
            }
            _ => {}
        }
    }
}

/// Motion coalescer accumulating sub-millisecond mouse deltas during high frequency moves
/// or backpressure on the network channel.
///
/// Moves are summed and sent on the next runtime turn instead of one message per OS event.
/// While the outbound queue is above `MOTION_BACKLOG_LIMIT`, moves keep summing rather
/// than queueing stale positions; one goes anyway after `MOTION_MAX_WAIT`. Any non-move
/// message flushes first, so a click lands where the moves took the pointer.
#[derive(Debug, Default)]
pub struct MotionCoalescer {
    pub accum_dx: f64,
    pub accum_dy: f64,
    /// A flush is already scheduled; further adds join it.
    scheduled: bool,
    /// When the first move was held back because the link was backed up.
    held_since: Option<std::time::Instant>,
}

impl MotionCoalescer {
    pub fn new() -> Self {
        Self::default()
    }

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
    pair_limit_gen: Arc<AtomicU64>,
    /// The other computer's last `notif.policy`: whether it wants ours.
    notif_accept: Arc<AtomicBool>,
    pub tls_identity: Arc<crate::transport::tls::LocalTlsIdentity>,
    discovery: Arc<RwLock<Option<Arc<DiscoveryService>>>>,
}

impl NetworkManager {
    pub fn new(app: AppHandle, state: AppState) -> Self {
        let local_id = state.inner.read().about.pc.host.to_lowercase();
        let local_name = state.inner.read().about.pc.host.clone();
        let pending_pair = Arc::new(RwLock::new(None));
        let tls_identity = Arc::new(
            crate::transport::tls::LocalTlsIdentity::load_or_generate()
                .unwrap_or_else(|e| {
                    log::warn!("[TLS] Fallback TLS identity: {}", e);
                    crate::transport::tls::LocalTlsIdentity {
                        cert_der: Vec::new(),
                        key_der: Vec::new(),
                        fingerprint_sha256: "00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00".to_string(),
                    }
                }),
        );

        Self {
            app,
            state,
            active_clients: Arc::new(RwLock::new(HashMap::new())),
            pending_pair,
            motion: Arc::new(Mutex::new(MotionCoalescer::new())),
            local_id,
            local_name,
            seq: Arc::new(Mutex::new(chrono::Utc::now().timestamp_millis() as u64)),
            pair_limit_gen: Arc::new(AtomicU64::new(0)),
            notif_accept: Arc::new(AtomicBool::new(false)),
            tls_identity,
            discovery: Arc::new(RwLock::new(None)),
        }
    }

    /// Approximate bytes waiting in the outbound queues of every connected session.
    ///
    /// Each queued message is counted at a nominal 2 KiB: KVM traffic is small, file
    /// chunks are the outliers. Precise byte accounting arrives with the kvm-moves port;
    /// what matters for the `[link]` log line is the trend, not the exact figure.
    pub fn queued_bytes(&self) -> u64 {
        let guard = self.active_clients.read();
        guard
            .values()
            .map(|c| {
                let max = c.tx.max_capacity().min(1 << 20);
                let free = c.tx.capacity();
                (max.saturating_sub(free) as u64).saturating_mul(2048)
            })
            .sum()
    }

    /// Put the pairing state into `phase` (guest side) and tell the UI.
    fn set_pairing_phase(&self, phase: &str, role: &str, name: &str, address: &str) {
        self.state.set_pairing(Some(crate::state::PairingState {
            phase: phase.to_string(),
            role: role.to_string(),
            mac_name: name.to_string(),
            os: String::new(),
            code: String::new(),
            allowed: false,
            wired: false,
            address: address.to_string(),
            error: String::new(),
        }));
        let snap = self.state.snapshot();
        let _ = self.app.emit("state-change", snap);
    }

    /// Mark the current pairing failed with a user-facing reason.
    fn fail_pairing(&self, message: String) {
        {
            let mut s = self.state.inner.write();
            if let Some(p) = s.pairing.as_mut() {
                p.phase = "failed".to_string();
                p.error = message;
                p.code.clear();
                p.allowed = false;
            }
        }
        let snap = self.state.snapshot();
        let _ = self.app.emit("state-change", snap);
    }

    /// Arm the time limit for `phase`, replacing any earlier timer. On expiry the phase
    /// becomes "failed" with the reason to show, unless the pairing moved on meanwhile.
    pub fn arm_pair_limit(&self, phase: &str, name: &str, address: &str) {
        let wired = self
            .state
            .inner
            .read()
            .pairing
            .as_ref()
            .map(|p| p.wired)
            .unwrap_or(false);
        let Some(limit) = crate::pairlimit::limit_for(phase, address, wired) else {
            return;
        };
        let generation = self.pair_limit_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let app = self.app.clone();
        let state = self.state.clone();
        let gen_arc = Arc::clone(&self.pair_limit_gen);
        let message = crate::pairlimit::limit_message(phase, name, address);
        let phase = phase.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(limit)).await;
            if gen_arc.load(Ordering::SeqCst) != generation {
                return; // a newer phase replaced this timer
            }
            let expired = {
                let mut s = state.inner.write();
                match s.pairing.as_mut() {
                    Some(p) if p.phase == phase => {
                        p.phase = "failed".to_string();
                        p.error = message.clone();
                        p.code.clear();
                        p.allowed = false;
                        true
                    }
                    _ => false,
                }
            };
            if expired {
                log::warn!("[Pairing] phase '{}' timed out", phase);
                let snap = state.snapshot();
                let _ = app.emit("state-change", snap);
            }
        });
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
        *self.discovery.write() = Some(Arc::clone(&discovery));
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
            Arc::clone(&self.tls_identity),
        ));
        server.start();
    }

    pub async fn refresh_discovery(&self) {
        let disc = self.discovery.read().clone();
        if let Some(d) = disc {
            d.send_probe().await;
        }
    }

    /// Dial the computer paired last time, and keep it connected: called once at startup.
    /// After a restart, or after the other computer sleeps, GlideDesk comes back by itself
    /// instead of asking for a new pairing code (or sitting on "Connecting…" for ever).
    pub async fn connect_last_paired(&self) {
        loop {
            // Let discovery and the listener settle on the very first pass.
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let (target, connected, pairing, manual_off) = {
                let s = self.state.inner.read();
                // The stored address, else where discovery last saw that computer.
                let addr = s.paired.as_ref().and_then(|p| {
                    p.address.clone().or_else(|| {
                        s.found
                            .iter()
                            .find(|f| same_peer_id(&f.id, &p.mac_id) || f.name.eq_ignore_ascii_case(&p.mac_name))
                            .and_then(|f| f.address.clone())
                    })
                });
                (
                    addr,
                    s.link.state == "connected",
                    s.pairing.is_some(),
                    s.link.manual_off,
                )
            };
            let Some(addr) = target else {
                return; // forgotten: nothing to reconnect to
            };
            if pairing || manual_off {
                // A user-started pairing is in charge, or the user chose Disconnect.
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
            if !connected {
                let addr = if addr.contains(':') { addr } else { format!("{}:{}", addr, DEFAULT_PORT) };
                match self.connect_to(&addr).await {
                    Ok(()) => log::info!("[Startup] Reconnected to the paired computer at {}", addr),
                    Err(e) => {
                        log::info!("[Startup] Could not reconnect to {} yet: {}", addr, e);
                        if e.contains("fingerprint changed") {
                            // The other computer's certificate is not the one we trust:
                            // say so instead of retrying quietly, until it is paired again.
                            {
                                let mut s = self.state.inner.write();
                                s.link.state = "identity_changed".to_string();
                                s.link.message = Some(e.clone());
                            }
                            let _ = self.app.emit("state-change", self.state.snapshot());
                        }
                    }
                }
            }
            // Check again soon while it's down, and now and then once it's up, so a
            // dropped link (the other computer slept, Wi-Fi changed) comes back.
            let wait = if connected { 20 } else { 8 };
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        }
    }

    pub async fn connect_to(&self, target_addr: &str) -> Result<(), String> {
        let clean_addr = target_addr.trim_start_matches("ws://").trim_start_matches("wss://");
        let target_ip = clean_addr.split(':').next().unwrap_or(clean_addr);

        // Somebody asked to connect (the user, or our own retry): not "stay disconnected" any more.
        self.state.inner.write().link.manual_off = false;

        // The other computer may already have dialled us; don't open a second session.
        if self.state.inner.read().link.state == "connected" {
            return Ok(());
        }

        // Avoid duplicate concurrent connection if already connected to this peer
        {
            let guard = self.active_clients.read();
            if guard.values().any(|c| c.peer_addr.ip().to_string() == target_ip || c.id == target_addr) {
                log::info!("[NetworkManager] Already connected to {}", target_addr);
                return Ok(());
            }
        }

        let (stored_fp, target_peer_id) = {
            let guard = self.state.inner.read();
            let pid = guard.paired.as_ref().map(|p| p.mac_id.clone());
            let fp = self.state.get_peer_fingerprint(target_addr)
                .or_else(|| self.state.get_peer_fingerprint(target_ip))
                .or_else(|| guard.paired.as_ref().and_then(|p| p.tls_fingerprint.clone()));
            (fp, pid)
        };

        let allow_plaintext = self.state.inner.read().net.allow_plaintext;
        let (ws, established_fp) = dial_peer_client(
            target_addr,
            stored_fp.as_deref(),
            target_peer_id.as_deref(),
            allow_plaintext,
        ).await?;

        if let (Some(ref pid), Some(ref fp)) = (&target_peer_id, &established_fp) {
            let _ = self.state.record_peer_fingerprint(pid, fp);
        }

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
        let authenticated = Arc::new(RwLock::new(false));
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
                    authenticated: Arc::clone(&authenticated),
                    outbound: true,
                },
            );
        }

        // Send hello with saved pairing token if available
        let saved_token = self.state.inner.read().paired.as_ref().and_then(|p| p.token.clone());
        let hello = WireMessage {
            t: "hello".to_string(),
            v: Some(1),
            id: Some(self.local_id.clone()),
            name: Some(self.local_name.clone()),
            os: Some(current_os().to_string()),
            port: Some(DEFAULT_PORT),
            role: Some("guest".to_string()),
            token: saved_token,
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
        let tls_fingerprint = self.tls_identity.fingerprint_sha256.clone();
        let client_tx = out_tx.clone();

        tauri::async_runtime::spawn(async move {
            log::debug!("[Net] client session {} started", cur_id);
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
                                                &authenticated,
                                                Some(&clients_lock),
                                                &client_tx,
                                                &local_id,
                                                &local_name,
                                                &tls_fingerprint,
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
                                                &authenticated,
                                                Some(&clients_lock),
                                                &client_tx,
                                                &local_id,
                                                &local_name,
                                                &tls_fingerprint,
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

            log::debug!("[Net] pair/client session {} ended", cur_id);
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
            let was_driving = state.inner.read().input == "driving";
            state.set_where_input("home");
            crate::kvm::NativeInputInjector::release_all_keys();
            if was_driving {
                crate::kvm::NativeInputInjector::recenter_local_cursor();
            }
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

        // Guest pairing: bound the dial, and say what to do if nothing answers.
        self.set_pairing_phase("connecting", "guest", target_ip, target_ip);
        self.arm_pair_limit("connecting", target_ip, target_ip);

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

        let (stored_fp, target_peer_id) = {
            let guard = self.state.inner.read();
            let pid = guard.paired.as_ref().map(|p| p.mac_id.clone());
            let fp = self.state.get_peer_fingerprint(target_addr)
                .or_else(|| self.state.get_peer_fingerprint(target_ip))
                .or_else(|| guard.paired.as_ref().and_then(|p| p.tls_fingerprint.clone()));
            (fp, pid)
        };
        log::info!("[Pair] start_pair: target = {}, stored_fp = {:?}, target_peer_id = {:?}", target_addr, stored_fp, target_peer_id);

        let allow_plaintext = self.state.inner.read().net.allow_plaintext;
        let dial = dial_peer_client(
            target_addr,
            stored_fp.as_deref(),
            target_peer_id.as_deref(),
            allow_plaintext,
        ).await;
        let (ws, established_fp) = match dial {
            Ok(v) => {
                log::info!("[Pair] dial_peer_client succeeded to {}", target_addr);
                v
            }
            Err(e) => {
                log::error!("[Pair] dial_peer_client FAILED to {}: {}", target_addr, e);
                let reason = crate::pairlimit::limit_message("connecting", target_ip, target_ip);
                self.fail_pairing(reason);
                return Err(e);
            }
        };

        if let (Some(ref pid), Some(ref fp)) = (&target_peer_id, &established_fp) {
            let _ = self.state.record_peer_fingerprint(pid, fp);
        }

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
        let authenticated = Arc::new(RwLock::new(false));
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
                    authenticated: Arc::clone(&authenticated),
                    outbound: true,
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
                // What the guest actually saw on the wire, not what the host claims later.
                peer_fp: established_fp.clone(),
                code: None,
                confirmed: false,
                allowed: false,
                tx: out_tx.clone(),
            });
        }

        self.state.set_pairing(Some(crate::state::PairingState {
            phase: "starting".to_string(),
            role: "guest".to_string(),
            mac_name: target_name.clone(),
            os: target_os,
            code: String::new(),
            allowed: false,
            wired: false,
            address: target_ip.to_string(),
            error: String::new(),
        }));
        let snap = self.state.snapshot();
        let _ = self.app.emit("state-change", &snap);
        // It answered; now bound how long we wait for its half of the code.
        self.arm_pair_limit("starting", &target_name, target_ip);

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
        write.send(Message::Text(start_str)).await.map_err(|e| {
            log::error!("[Pair] Failed to send pair.start: {}", e);
            e.to_string()
        })?;
        log::info!("[Pair] Sent pair.start to {}, spawned receiver loop", target_addr);

        let app = self.app.clone();
        let state = self.state.clone();
        let clients_lock = Arc::clone(&self.active_clients);
        let cur_id = session_id.clone();
        let pid_holder = Arc::clone(&peer_id_holder);
        let pending_pair = Arc::clone(&self.pending_pair);
        let local_id = self.local_id.clone();
        let local_name = self.local_name.clone();
        let tls_fingerprint = self.tls_identity.fingerprint_sha256.clone();
        let client_tx = out_tx.clone();

        tauri::async_runtime::spawn(async move {
            let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
            let mut tracker = HeartbeatTracker::new();

            log::debug!("[Net] pair session {} started", cur_id);
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
                                log::warn!(
                                    "[Net] pair session {} write failed ({})",
                                    cur_id, pkt.t
                                );
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
                                                &authenticated,
                                                Some(&clients_lock),
                                                &client_tx,
                                                &local_id,
                                                &local_name,
                                                &tls_fingerprint,
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
                                                &authenticated,
                                                Some(&clients_lock),
                                                &client_tx,
                                                &local_id,
                                                &local_name,
                                                &tls_fingerprint,
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

            log::debug!("[Net] pair session {} ended", cur_id);
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
            let was_driving = state.inner.read().input == "driving";
            state.set_where_input("home");
            crate::kvm::NativeInputInjector::release_all_keys();
            if was_driving {
                crate::kvm::NativeInputInjector::recenter_local_cursor();
            }
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
            MeshServer::finish_host_pairing(&self.state, &self.app, &self.pending_pair, Some(&self.active_clients), &self.local_id, &self.local_name, &self.tls_identity.fingerprint_sha256).await;
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
            // Bound how long the other user has to press Allow.
            let (name, address) = self
                .state
                .inner
                .read()
                .pairing
                .as_ref()
                .map(|p| (p.mac_name.clone(), p.address.clone()))
                .unwrap_or_default();
            self.arm_pair_limit("waiting", &name, &address);
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

    /// Fast non-blocking relative motion dispatch: sum the deltas and send them on the
    /// next runtime turn, so a burst becomes one message with the total motion.
    pub fn send_motion(&self, dx: f64, dy: f64) {
        crate::applog::note_move(1);
        let schedule = {
            let mut motion = self.motion.lock();
            motion.add(dx, dy);
            if motion.scheduled {
                false
            } else {
                motion.scheduled = true;
                true
            }
        };
        if schedule {
            let this = self.clone();
            tauri::async_runtime::spawn(async move {
                tokio::task::yield_now().await;
                this.try_flush_motion();
            });
        }
    }

    /// Send the summed motion if the link has room, or after MOTION_MAX_WAIT; while the
    /// outbound queue is backed up keep summing and look again in MOTION_RETRY_MS.
    fn try_flush_motion(&self) {
        if self.queued_bytes() > MOTION_BACKLOG_LIMIT {
            let retry = {
                let mut motion = self.motion.lock();
                if !motion.pending() {
                    motion.scheduled = false;
                    return;
                }
                let now = std::time::Instant::now();
                let held = *motion.held_since.get_or_insert(now);
                if now.duration_since(held) < MOTION_MAX_WAIT {
                    true
                } else {
                    motion.held_since = None;
                    false
                }
            };
            if retry {
                let this = self.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(MOTION_RETRY_MS)).await;
                    this.try_flush_motion();
                });
                return;
            }
        } else {
            self.motion.lock().held_since = None;
        }
        self.flush_motion_inner();
    }

    /// Flushes any pending accumulated motion prior to sending mouse clicks, wheel, or keystrokes.
    pub fn flush_motion(&self) {
        self.flush_motion_inner();
    }

    fn flush_motion_inner(&self) {
        let (cdx, cdy) = {
            let mut motion = self.motion.lock();
            motion.scheduled = false;
            motion.held_since = None;
            if !motion.pending() {
                return;
            }
            motion.take()
        };
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
        let target_dests = pkt.d.clone();
        if pkt.o.is_none() {
            pkt.o = Some(self.local_id.clone());
            let mut s = self.seq.lock();
            *s += 1;
            pkt.s = Some(*s);
            if pkt.d.is_none() {
                let client_ids: Vec<String> = self.active_clients.read().keys().cloned().collect();
                pkt.d = Some(client_ids);
            }
            pkt.h = Some(0);
        }

        let is_sensitive = matches!(
            pkt.t.as_str(),
            "k.mm" | "remote.mm" | "k.md" | "remote.md" | "k.mu" | "remote.mu"
                | "k.wh" | "remote.wh" | "k.kd" | "remote.kd" | "k.ku" | "remote.ku"
                | "clip" | "clip.image" | "kvm.take" | "kvm.home" | "kvm.release"
                | "kvm.layout" | "window.handoff.offer" | "window.handoff.return"
                | "gamepad.event" | "chat.conv" | "notif.reply" | "chat.want"
                | "notif" | "notif.policy"
                | "rz.offer" | "rz.take" | "phone.media.ctl"
                | "files.batch" | "files.chunk" | "files.done" | "files.ack" | "files.cancel"
        );

        let mut sent = 0usize;
        let mut dropped = 0usize;
        let mut skipped = 0usize;
        {
            let guard = self.active_clients.read();
            // Two computers that dialled each other at the same moment hold a pair of
            // sessions; use only the one both ends agree on.
            let preferred = preferred_session_ids(&self.local_id, &guard);
            for client in guard.values() {
                let pid = client.peer_id.read().clone();
                let matches_dest = match &target_dests {
                    Some(dests) => {
                        let by_session = dests.iter().any(|d| d == &client.id);
                        // Naming the peer rather than the session matches both of its
                        // sessions, so only the agreed one carries the packet.
                        let by_peer = dests.iter().any(|d| {
                            pid.as_deref() == Some(d.as_str())
                                || d == &client.peer_addr.ip().to_string()
                        });
                        by_session || (by_peer && preferred.contains(&client.id))
                    }
                    None => preferred.contains(&client.id),
                };

                if matches_dest && (!is_sensitive || *client.authenticated.read()) {
                    match client.tx.try_send(pkt.clone()) {
                        Ok(_) => sent += 1,
                        Err(_) => dropped += 1,
                    }
                } else {
                    skipped += 1;
                }
            }
        }
        if (dropped > 0 || skipped > 0)
            && (pkt.t.starts_with("k.") || pkt.t.starts_with("remote."))
        {
            log::debug!(
                "[KVM] send {} -> sent={} dropped={} skipped={}",
                pkt.t, sent, dropped, skipped
            );
        }
    }

    /// Async send/broadcast to all active mesh peers.
    pub async fn send_packet(&self, mut pkt: WireMessage) {
        self.flush_motion();
        let target_dests = pkt.d.clone();
        if pkt.o.is_none() {
            pkt.o = Some(self.local_id.clone());
            let mut s = self.seq.lock();
            *s += 1;
            pkt.s = Some(*s);
            if pkt.d.is_none() {
                let client_ids: Vec<String> = self.active_clients.read().keys().cloned().collect();
                pkt.d = Some(client_ids);
            }
            pkt.h = Some(0);
        }

        let is_sensitive = matches!(
            pkt.t.as_str(),
            "k.mm" | "remote.mm" | "k.md" | "remote.md" | "k.mu" | "remote.mu"
                | "k.wh" | "remote.wh" | "k.kd" | "remote.kd" | "k.ku" | "remote.ku"
                | "clip" | "clip.image" | "kvm.take" | "kvm.home" | "kvm.release"
                | "kvm.layout" | "window.handoff.offer" | "window.handoff.return"
                | "gamepad.event" | "chat.conv" | "notif.reply" | "chat.want"
                | "notif" | "notif.policy"
                | "rz.offer" | "rz.take" | "phone.media.ctl"
                | "files.batch" | "files.chunk" | "files.done" | "files.ack" | "files.cancel"
        );

        let senders: Vec<mpsc::Sender<WireMessage>> = {
            let guard = self.active_clients.read();
            // One session per peer; see `preferred_session_ids`.
            let preferred = preferred_session_ids(&self.local_id, &guard);
            guard
                .values()
                .filter_map(|c| {
                    let pid = c.peer_id.read().clone();
                    let matches_dest = match &target_dests {
                        Some(dests) => {
                            let by_session = dests.iter().any(|d| d == &c.id);
                            let by_peer = dests.iter().any(|d| {
                                pid.as_deref() == Some(d.as_str())
                                    || d == &c.peer_addr.ip().to_string()
                            });
                            by_session || (by_peer && preferred.contains(&c.id))
                        }
                        None => preferred.contains(&c.id),
                    };

                    if matches_dest && (!is_sensitive || *c.authenticated.read()) {
                        Some(c.tx.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        for tx in senders {
            let _ = tx.send(pkt.clone()).await;
        }
    }

    /// Returns snapshot list of active connected clients.
    /// This install's stable id, the one other computers know it by.
    pub fn local_id(&self) -> &str {
        &self.local_id
    }

    /// The other computer's last `notif.policy`: true when it shows ours.
    pub fn set_notif_accept(&self, on: bool) {
        self.notif_accept.store(on, Ordering::Relaxed);
    }

    pub fn notif_accept(&self) -> bool {
        self.notif_accept.load(Ordering::Relaxed)
    }

    pub fn get_active_clients_info(&self) -> Vec<ClientInfo> {
        let guard = self.active_clients.read();
        guard
            .values()
            .map(|c| ClientInfo {
                id: c.id.clone(),
                peer_id: c.peer_id.read().clone(),
                peer_ip: c.peer_addr.ip().to_string(),
                authenticated: *c.authenticated.read(),
            })
            .collect()
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
        // An explicit Disconnect stays disconnected until the user connects again.
        self.state.inner.write().link.manual_off = true;
        let snap = self.state.snapshot();
        let _ = self.app.emit("state-change", &snap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_coalescer_sums_then_takes() {
        let mut m = MotionCoalescer::new();
        m.add(3.0, -1.0);
        m.add(2.0, 4.0);
        assert!(m.pending());
        assert_eq!(m.take(), (5.0, 3.0));
        assert!(!m.pending());
        assert_eq!(m.take(), (0.0, 0.0));
    }

    #[test]
    fn motion_coalescer_below_epsilon_is_not_pending() {
        let mut m = MotionCoalescer::new();
        m.add(0.0001, -0.0001);
        assert!(!m.pending());
    }

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
    fn test_discovery_probe_message_format() {
        let probe = WireMessage {
            t: "probe".to_string(),
            v: Some(1),
            id: Some("test-peer".to_string()),
            name: Some("Test Peer".to_string()),
            os: Some("macos".to_string()),
            port: Some(47800),
            role: Some("host".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&probe).expect("serialize probe");
        let parsed: WireMessage = serde_json::from_str(&json).expect("deserialize probe");
        assert_eq!(parsed.t, "probe");
        assert_eq!(parsed.id.as_deref(), Some("test-peer"));
        assert_eq!(parsed.port, Some(47800));
    }

    #[test]
    fn test_sas_code_generation_node_parity() {
        let code = sas_code("", "0123456789abcdef0123456789abcdef", "fedcba9876543210fedcba9876543210");
        assert_eq!(code, "286217");
        assert_eq!(code.len(), 6);
    }

    #[test]
    fn test_pair_code_is_the_same_on_both_computers() {
        let host_fp = "aa".repeat(32);
        let guest_nonce = "01".repeat(32);
        let host_nonce = "02".repeat(32);

        let code = pair_code(&host_fp, &guest_nonce, &host_nonce);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        // Both roles feed these three values: the host its own certificate, the guest the one
        // it saw on the wire. Nothing in between them, so the same certificate and the same code.
        assert_eq!(code, sas_code(&host_fp, &guest_nonce, &host_nonce));

        // Any other certificate — a middlebox's, or the guest's own (which the host used to
        // hash) — gives different digits, and the two windows would disagree.
        assert_ne!(code, pair_code(&"cc".repeat(32), &guest_nonce, &host_nonce));
        assert_ne!(code, pair_code(&"bb".repeat(32), &guest_nonce, &host_nonce));
        // Nor is it the host's nonce in the guest's place, or the other way round.
        assert_ne!(code, pair_code(&host_fp, &host_nonce, &guest_nonce));
    }

    #[test]
    fn test_commit_of_verification() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let commit = commit_of(nonce);
        assert!(!commit.is_empty());
        assert_eq!(commit_of(nonce), commit);
    }

    #[test]
    fn test_sas_code_with_token_and_deterministic_order() {
        let code1 = sas_code("secret123", "nonce_a_1234567890", "nonce_b_0987654321");
        assert_eq!(code1.len(), 6);
        assert!(code1.chars().all(|c| c.is_ascii_digit()));

        // Nonce ordering is strictly deterministic
        let code2 = sas_code("secret123", "nonce_a_1234567890", "nonce_b_0987654321");
        assert_eq!(code1, code2);
    }

    #[test]
    fn test_wire_message_layout_and_kvm_types() {
        let layout_pkt = WireMessage {
            t: "kvm.layout".to_string(),
            v: Some(1),
            side: Some("right".to_string()),
            pc_side: Some("left".to_string()),
            ..Default::default()
        };
        let s = serde_json::to_string(&layout_pkt).expect("serialize layout");
        assert!(s.contains("\"t\":\"kvm.layout\""));
        assert!(s.contains("\"side\":\"right\""));
        assert!(s.contains("\"pcSide\":\"left\""));

        let take_pkt = WireMessage {
            t: "kvm.take".to_string(),
            v: Some(1),
            ..Default::default()
        };
        let s_take = serde_json::to_string(&take_pkt).expect("serialize take");
        let parsed_take: WireMessage = serde_json::from_str(&s_take).expect("deserialize take");
        assert_eq!(parsed_take.t, "kvm.take");

        let home_pkt = WireMessage {
            t: "kvm.home".to_string(),
            v: Some(1),
            ..Default::default()
        };
        let s_home = serde_json::to_string(&home_pkt).expect("serialize home");
        let parsed_home: WireMessage = serde_json::from_str(&s_home).expect("deserialize home");
        assert_eq!(parsed_home.t, "kvm.home");
    }

    #[test]
    fn test_wire_message_window_handoff_lifecycle() {
        let offer = WireMessage {
            t: "window.handoff.offer".to_string(),
            v: Some(1),
            id: Some("win_123".to_string()),
            name: Some("Calculator".to_string()),
            w: Some(800),
            height: Some(600),
            ..Default::default()
        };
        let s_offer = serde_json::to_string(&offer).expect("serialize offer");
        let parsed_offer: WireMessage = serde_json::from_str(&s_offer).expect("deserialize offer");
        assert_eq!(parsed_offer.t, "window.handoff.offer");
        assert_eq!(parsed_offer.id.as_deref(), Some("win_123"));
        assert_eq!(parsed_offer.w, Some(800));
        assert_eq!(parsed_offer.height, Some(600));

        let ret = WireMessage {
            t: "window.handoff.return".to_string(),
            v: Some(1),
            id: Some("win_123".to_string()),
            ..Default::default()
        };
        let s_ret = serde_json::to_string(&ret).expect("serialize return");
        let parsed_ret: WireMessage = serde_json::from_str(&s_ret).expect("deserialize return");
        assert_eq!(parsed_ret.t, "window.handoff.return");
        assert_eq!(parsed_ret.id.as_deref(), Some("win_123"));
    }

    #[test]
    fn test_motion_coalescer_high_frequency_bursts() {
        let mut coalescer = MotionCoalescer::new();
        for _ in 0..100 {
            coalescer.add(0.25, -0.5);
        }
        assert!(coalescer.pending());
        let (dx, dy) = coalescer.take();
        assert!((dx - 25.0).abs() < 1e-6);
        assert!((dy - (-50.0)).abs() < 1e-6);
        assert!(!coalescer.pending());
    }

    #[test]
    fn test_multidevice_mesh_wire_packet_hops_and_relaying() {
        // Mock multi-device mesh: Device A -> Device B -> Device C
        let msg = WireMessage {
            t: "clipboard.share".to_string(),
            v: Some(1),
            o: Some("device_A".to_string()),
            s: Some(101),
            d: Some(vec!["device_B".to_string()]),
            h: Some(0), // Hop count starts at 0
            text: Some("hello multi-device mesh".to_string()),
            ..Default::default()
        };

        // Serialize across wire A -> B
        let wire_a_to_b = serde_json::to_string(&msg).expect("serialize A to B");
        let mut msg_at_b: WireMessage = serde_json::from_str(&wire_a_to_b).expect("deserialize at B");

        assert_eq!(msg_at_b.o.as_deref(), Some("device_A"));
        assert_eq!(msg_at_b.h, Some(0));

        // Device B relays to Device C: increment hop count
        let hops = msg_at_b.h.unwrap_or(0);
        const MAX_HOPS: u32 = 4;
        assert!(hops < MAX_HOPS);
        msg_at_b.h = Some(hops + 1);

        // Serialize across wire B -> C
        let wire_b_to_c = serde_json::to_string(&msg_at_b).expect("serialize B to C");
        let msg_at_c: WireMessage = serde_json::from_str(&wire_b_to_c).expect("deserialize at C");

        assert_eq!(msg_at_c.o.as_deref(), Some("device_A"));
        assert_eq!(msg_at_c.h, Some(1));
        assert_eq!(msg_at_c.text.as_deref(), Some("hello multi-device mesh"));
    }

    #[test]
    fn layout_sync_puts_each_screen_on_its_own_side() {
        use crate::commands::opposite_direction;
        // Each computer sends the side it sits on; the other flips it once, so the two
        // screens meet at the edge they share and never end up on the same side.
        for ours in ["left", "right", "top", "bottom"] {
            let theirs = opposite_direction(ours);
            assert_ne!(theirs, ours, "{ours} must not put both screens on one side");
            assert_eq!(opposite_direction(theirs), ours);
        }
        assert_eq!(opposite_direction("left"), "right");
        assert_eq!(opposite_direction("top"), "bottom");
    }

    #[test]
    fn layout_converges_when_both_sides_claim_the_same_edge() {
        // Already consistent: the other computer is on our right, so we are on its
        // left. Nothing changes however the ids compare.
        assert_eq!(layout_side_to_adopt("aaa", "zzz", "left", "right"), None);
        assert_eq!(layout_side_to_adopt("zzz", "aaa", "left", "right"), None);

        // Both computers claim "left" (a fresh launch). The smaller id keeps its
        // choice; the larger id turns round, so the pair stops swapping.
        assert_eq!(layout_side_to_adopt("aaa", "zzz", "left", "left"), None);
        assert_eq!(layout_side_to_adopt("zzz", "aaa", "left", "left"), Some("right"));
        // Same rule from the other side of the pair.
        assert_eq!(layout_side_to_adopt("aaa", "zzz", "right", "right"), None);
        assert_eq!(layout_side_to_adopt("zzz", "aaa", "right", "right"), Some("left"));

        // A real rearrangement (the other computer moved above us) is always followed.
        assert_eq!(layout_side_to_adopt("aaa", "zzz", "left", "top"), Some("bottom"));
        assert_eq!(layout_side_to_adopt("zzz", "aaa", "left", "top"), Some("bottom"));

        // An unknown peer id still lets the claim through.
        assert_eq!(layout_side_to_adopt("aaa", "", "right", "right"), Some("left"));
    }

    #[test]
    fn two_sessions_to_one_peer_use_one_of_them() {
        // Receivers are kept alive for as long as the sessions exist.
        let mut held: Vec<mpsc::Receiver<WireMessage>> = Vec::new();
        fn session(
            id: &str,
            peer: Option<&str>,
            outbound: bool,
            held: &mut Vec<mpsc::Receiver<WireMessage>>,
        ) -> ActiveClientSession {
            let (tx, rx) = mpsc::channel::<WireMessage>(4);
            held.push(rx);
            ActiveClientSession {
                id: id.to_string(),
                peer_id: Arc::new(RwLock::new(peer.map(|p| p.to_string()))),
                peer_addr: "192.168.4.9:47800".parse().unwrap(),
                tx,
                authenticated: Arc::new(RwLock::new(true)),
                outbound,
            }
        }

        // Both computers dialled at the same moment: one session each way.
        let mut clients = HashMap::new();
        clients.insert("inbound".to_string(), session("192.168.4.9:50123", Some("peer"), false, &mut held));
        clients.insert("outbound".to_string(), session("192.168.4.9:47800", Some("peer"), true, &mut held));

        // The computer with the smaller id dials, so it keeps its outbound session.
        let ours = preferred_session_ids("aaa", &clients);
        assert_eq!(ours.len(), 1);
        assert!(ours.contains("192.168.4.9:47800"));

        // The other computer keeps its inbound session, which is the same connection.
        let theirs = preferred_session_ids("zzz", &clients);
        assert_eq!(theirs.len(), 1);
        assert!(theirs.contains("192.168.4.9:50123"));

        // A single session is always used, whatever the ids.
        let mut one = HashMap::new();
        one.insert("only".to_string(), session("192.168.4.9:50123", Some("peer"), false, &mut held));
        assert!(preferred_session_ids("aaa", &one).contains("192.168.4.9:50123"));

        // Sessions that have not introduced themselves are left alone.
        let mut unknown = HashMap::new();
        unknown.insert("fresh".to_string(), session("192.168.4.8:50001", None, false, &mut held));
        assert!(preferred_session_ids("aaa", &unknown).contains("192.168.4.8:50001"));
    }

    #[test]
    fn test_active_session_auth_gate_and_pairing_token() {
        let auth_flag = Arc::new(RwLock::new(false));
        let (tx, _rx) = mpsc::channel::<WireMessage>(10);
        let session = ActiveClientSession {
            id: "192.168.4.15:47800".to_string(),
            peer_id: Arc::new(RwLock::new(Some("unpaired_guest".to_string()))),
            peer_addr: "192.168.4.15:47800".parse().unwrap(),
            tx,
            authenticated: Arc::clone(&auth_flag),
            outbound: false,
        };

        // 1. Session begins unauthenticated
        assert!(!*session.authenticated.read());

        // 2. Generate cryptographically random pairing token
        let token = random_hex(16);
        assert_eq!(token.len(), 32);

        // 3. Simulate successful handshake authentication
        *session.authenticated.write() = true;
        assert!(*session.authenticated.read());
    }

    #[test]
    fn test_resolve_file_dest_exact_rules_and_matching() {
        let clients = vec![
            ClientInfo::new("192.168.1.101:47800", Some("peer_alpha".into()), "192.168.1.101", true),
            ClientInfo::new("192.168.1.102:47800", Some("peer_beta".into()), "192.168.1.102", true),
            ClientInfo::new("192.168.1.103:47800", Some("peer_gamma".into()), "192.168.1.103", false),
        ];

        // 1. Match by client id -> yields peer's stable id
        let res_id = resolve_file_dest(Some("192.168.1.101:47800"), &clients);
        assert_eq!(res_id, Ok(vec!["peer_alpha".to_string()]));

        // 2. Match by peer_id -> yields peer's stable id
        let res_peer = resolve_file_dest(Some("peer_beta"), &clients);
        assert_eq!(res_peer, Ok(vec!["peer_beta".to_string()]));

        // 3. Match by IP -> yields peer's stable id
        let res_ip = resolve_file_dest(Some("192.168.1.101"), &clients);
        assert_eq!(res_ip, Ok(vec!["peer_alpha".to_string()]));

        // 4. Unknown target -> Err
        let res_unknown = resolve_file_dest(Some("non_existent_peer"), &clients);
        assert_eq!(res_unknown, Err("target peer 'non_existent_peer' is not connected".to_string()));

        // 5. Explicit target matching unauthenticated client -> Err
        let res_unauth = resolve_file_dest(Some("peer_gamma"), &clients);
        assert_eq!(
            res_unauth,
            Err("target peer 'peer_gamma' is not authenticated".to_string())
        );

        // 6. Matched client with no identified peer_id -> explicit Err
        let unidentified = vec![
            ClientInfo::new("192.168.1.104:47800", None, "192.168.1.104", true),
        ];
        assert_eq!(
            resolve_file_dest(Some("192.168.1.104"), &unidentified),
            Err("target peer '192.168.1.104' has not identified itself yet".to_string())
        );

        // 7. None + single authenticated client -> use it
        let single_auth = vec![
            ClientInfo::new("192.168.1.102:47800", Some("peer_beta".into()), "192.168.1.102", true),
        ];
        assert_eq!(resolve_file_dest(None, &single_auth), Ok(vec!["peer_beta".to_string()]));

        // 8. None + single unauthenticated client -> Err("no peers connected")
        let single_unauth = vec![
            ClientInfo::new("192.168.1.103:47800", Some("peer_gamma".into()), "192.168.1.103", false),
        ];
        assert_eq!(
            resolve_file_dest(None, &single_unauth),
            Err("no peers connected".to_string())
        );

        // 9. None + zero clients -> distinct Err("no peers connected")
        assert_eq!(
            resolve_file_dest(None, &[]),
            Err("no peers connected".to_string())
        );

        // 10. None + multiple authenticated clients -> Err
        assert_eq!(
            resolve_file_dest(None, &clients),
            Err("send_files requires a target peer (multiple peers connected)".to_string())
        );

        // 11. None + two sessions with the same computer (both dialled at once) ->
        //     still one destination, not "multiple peers connected".
        let two_sessions_one_peer = vec![
            ClientInfo::new("192.168.1.102:47800", Some("peer_beta".into()), "192.168.1.102", true),
            ClientInfo::new("192.168.1.102:50123", Some("peer_beta".into()), "192.168.1.102", true),
        ];
        assert_eq!(
            resolve_file_dest(None, &two_sessions_one_peer),
            Ok(vec!["peer_beta".to_string()])
        );

        // 12. Two different computers are still ambiguous.
        let two_peers = vec![
            ClientInfo::new("192.168.1.102:47800", Some("peer_beta".into()), "192.168.1.102", true),
            ClientInfo::new("192.168.1.104:47800", Some("peer_delta".into()), "192.168.1.104", true),
        ];
        assert!(resolve_file_dest(None, &two_peers).is_err());
    }

    #[test]
    fn test_file_transfer_packet_construction_d_routing() {
        let active = vec![
            ClientInfo::new("client_device_b", Some("peer_b".into()), "192.168.1.50", true),
        ];

        let dest = resolve_file_dest(Some("peer_b"), &active).expect("resolve peer_b");
        assert_eq!(dest, vec!["peer_b".to_string()]);

        // Construct files.batch
        let batch_pkt = WireMessage {
            t: "files.batch".to_string(),
            v: Some(1),
            batch_id: Some("batch_test_01".to_string()),
            target: Some("peer_b".to_string()),
            d: Some(dest.clone()),
            ..Default::default()
        };
        assert_eq!(batch_pkt.d, Some(vec!["peer_b".to_string()]));
        assert!(!batch_pkt.d.as_ref().unwrap().is_empty());

        // Construct files.chunk
        let chunk_pkt = WireMessage {
            t: "files.chunk".to_string(),
            v: Some(1),
            batch_id: Some("batch_test_01".to_string()),
            file_id: Some("file_01".to_string()),
            offset: Some(0),
            len: Some(1024),
            target: Some("peer_b".to_string()),
            d: Some(dest.clone()),
            ..Default::default()
        };
        assert_eq!(chunk_pkt.d, Some(vec!["peer_b".to_string()]));
        assert!(!chunk_pkt.d.as_ref().unwrap().is_empty());

        // Construct files.done
        let done_pkt = WireMessage {
            t: "files.done".to_string(),
            v: Some(1),
            batch_id: Some("batch_test_01".to_string()),
            file_id: Some("file_01".to_string()),
            sha256_total: Some("abcdef123456".to_string()),
            target: Some("peer_b".to_string()),
            d: Some(dest.clone()),
            ..Default::default()
        };
        assert_eq!(done_pkt.d, Some(vec!["peer_b".to_string()]));
        assert!(!done_pkt.d.as_ref().unwrap().is_empty());

        // Construct files.cancel
        let cancel_pkt = WireMessage {
            t: "files.cancel".to_string(),
            v: Some(1),
            batch_id: Some("batch_test_01".to_string()),
            d: Some(dest.clone()),
            ..Default::default()
        };
        assert_eq!(cancel_pkt.d, Some(vec!["peer_b".to_string()]));
        assert!(!cancel_pkt.d.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_three_peer_unicast_routing_and_observer_isolation() {
        // Setup 3 peers: Host sends to Peer B, while Peer C (authenticated observer) must receive nothing.
        let (tx_b, mut rx_b) = mpsc::channel::<WireMessage>(10);
        let (tx_c, mut rx_c) = mpsc::channel::<WireMessage>(10);

        let session_b = ActiveClientSession {
            id: "192.168.1.102:47800".to_string(),
            peer_id: Arc::new(RwLock::new(Some("peer_beta".to_string()))),
            peer_addr: "192.168.1.102:47800".parse().unwrap(),
            tx: tx_b,
            authenticated: Arc::new(RwLock::new(true)),
            outbound: false,
        };

        let session_c = ActiveClientSession {
            id: "192.168.1.103:47800".to_string(),
            peer_id: Arc::new(RwLock::new(Some("peer_gamma_observer".to_string()))),
            peer_addr: "192.168.1.103:47800".parse().unwrap(),
            tx: tx_c,
            authenticated: Arc::new(RwLock::new(true)),
            outbound: false,
        };

        let active_map = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut map = active_map.write();
            map.insert(session_b.id.clone(), session_b);
            map.insert(session_c.id.clone(), session_c);
        }

        let clients_info = {
            let guard = active_map.read();
            guard.values().map(|c| ClientInfo {
                id: c.id.clone(),
                peer_id: c.peer_id.read().clone(),
                peer_ip: c.peer_addr.ip().to_string(),
                authenticated: *c.authenticated.read(),
            }).collect::<Vec<_>>()
        };

        // Resolve destination for peer_beta -> stable peer_id
        let dest = resolve_file_dest(Some("peer_beta"), &clients_info).expect("resolve peer_beta");
        assert_eq!(dest, vec!["peer_beta".to_string()]);

        // Build file transfer packets addressed strictly to dest
        let packets = vec![
            WireMessage {
                t: "files.batch".to_string(),
                v: Some(1),
                batch_id: Some("batch_unicast_99".to_string()),
                d: Some(dest.clone()),
                ..Default::default()
            },
            WireMessage {
                t: "files.chunk".to_string(),
                v: Some(1),
                batch_id: Some("batch_unicast_99".to_string()),
                file_id: Some("file_0".to_string()),
                offset: Some(0),
                len: Some(512),
                d: Some(dest.clone()),
                ..Default::default()
            },
            WireMessage {
                t: "files.done".to_string(),
                v: Some(1),
                batch_id: Some("batch_unicast_99".to_string()),
                file_id: Some("file_0".to_string()),
                sha256_total: Some("hash123".to_string()),
                d: Some(dest.clone()),
                ..Default::default()
            },
            WireMessage {
                t: "files.cancel".to_string(),
                v: Some(1),
                batch_id: Some("batch_unicast_99".to_string()),
                d: Some(dest.clone()),
                ..Default::default()
            },
        ];

        // Simulate send_packet routing matcher
        for pkt in packets {
            let target_dests = pkt.d.clone();
            let is_sensitive = matches!(
                pkt.t.as_str(),
                "files.batch" | "files.chunk" | "files.done" | "files.cancel"
            );
            assert!(is_sensitive, "File packets must be marked is_sensitive");

            let guard = active_map.read();
            for client in guard.values() {
                let pid = client.peer_id.read().clone();
                let matches_dest = match &target_dests {
                    Some(dests) => dests.iter().any(|d| {
                        d == &client.id
                            || pid.as_deref() == Some(d.as_str())
                            || d == &client.peer_addr.ip().to_string()
                    }),
                    None => true,
                };

                if matches_dest && (!is_sensitive || *client.authenticated.read()) {
                    let _ = client.tx.try_send(pkt.clone());
                }
            }
        }

        // Intended peer B must have received all 4 packets
        assert_eq!(rx_b.try_recv().unwrap().t, "files.batch");
        assert_eq!(rx_b.try_recv().unwrap().t, "files.chunk");
        assert_eq!(rx_b.try_recv().unwrap().t, "files.done");
        assert_eq!(rx_b.try_recv().unwrap().t, "files.cancel");

        // Third peer observer C must have received NO packets
        assert!(
            rx_c.try_recv().is_err(),
            "Observer peer C must receive NO files.batch/chunk/done/cancel packets"
        );
    }

    #[test]
    fn test_destination_contract_sender_and_receiver_agree() {
        let (tx_b, mut rx_b) = mpsc::channel::<WireMessage>(10);
        let (tx_c, mut rx_c) = mpsc::channel::<WireMessage>(10);

        let session_b = ActiveClientSession {
            id: "192.168.1.102:47800".to_string(),
            peer_id: Arc::new(RwLock::new(Some("peer_beta".to_string()))),
            peer_addr: "192.168.1.102:47800".parse().unwrap(),
            tx: tx_b,
            authenticated: Arc::new(RwLock::new(true)),
            outbound: false,
        };

        let session_c = ActiveClientSession {
            id: "192.168.1.103:47800".to_string(),
            peer_id: Arc::new(RwLock::new(Some("peer_gamma_observer".to_string()))),
            peer_addr: "192.168.1.103:47800".parse().unwrap(),
            tx: tx_c,
            authenticated: Arc::new(RwLock::new(true)),
            outbound: false,
        };

        let active_map = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut map = active_map.write();
            map.insert(session_b.id.clone(), session_b);
            map.insert(session_c.id.clone(), session_c);
        }

        let clients_info = {
            let guard = active_map.read();
            guard.values().map(|c| ClientInfo {
                id: c.id.clone(),
                peer_id: c.peer_id.read().clone(),
                peer_ip: c.peer_addr.ip().to_string(),
                authenticated: *c.authenticated.read(),
            }).collect::<Vec<_>>()
        };

        // 1. Sender resolves destination for "peer_beta" -> must yield stable peer_id
        let dest = resolve_file_dest(Some("peer_beta"), &clients_info).expect("resolve peer_beta");
        assert_eq!(dest, vec!["peer_beta".to_string()]);

        // 2. Build WireMessage using sender's actual resolved dest output
        let pkt = WireMessage {
            t: "files.chunk".to_string(),
            v: Some(1),
            batch_id: Some("batch_agree_01".to_string()),
            file_id: Some("file_0".to_string()),
            offset: Some(0),
            len: Some(1024),
            d: Some(dest.clone()),
            ..Default::default()
        };

        // 3. Receiver evaluates is_addressed_to_local against sender's actual pkt.d
        assert!(
            is_addressed_to_local(&pkt.d, "peer_beta"),
            "Intended receiver (peer_beta) must accept packet"
        );
        assert!(
            !is_addressed_to_local(&pkt.d, "peer_gamma_observer"),
            "Observer (peer_gamma_observer) must reject packet"
        );

        // 4. Regression assertion: old bug where d contained socket address while receiver local_id was host name
        let old_bug_d = Some(vec!["192.168.1.102:47800".to_string()]);
        assert!(
            !is_addressed_to_local(&old_bug_d, "bensons-imac"),
            "Receiver must reject socket-address destination (documents the old bug)"
        );

        // 5. Broadcast / legacy compatibility: None dest accepted by all
        assert!(
            is_addressed_to_local(&None, "any_peer"),
            "Unaddressed / broadcast packets remain acceptable"
        );

        // 6. Assert sender routing matcher delivers to peer_beta and NOT to peer_gamma_observer
        let target_dests = pkt.d.clone();
        let guard = active_map.read();
        for client in guard.values() {
            let pid = client.peer_id.read().clone();
            let matches_dest = match &target_dests {
                Some(dests) => dests.iter().any(|d| {
                    d == &client.id
                        || pid.as_deref() == Some(d.as_str())
                        || d == &client.peer_addr.ip().to_string()
                }),
                None => true,
            };
            if matches_dest && *client.authenticated.read() {
                let _ = client.tx.try_send(pkt.clone());
            }
        }

        assert!(rx_b.try_recv().is_ok(), "peer_beta channel must receive packet");
        assert!(rx_c.try_recv().is_err(), "peer_gamma_observer channel must receive NO packets");
    }

    #[test]
    fn test_file_packet_guard_addresses_and_legacy_broadcast() {
        // Addressed packet: only the named receiver accepts it.
        let addressed = WireMessage {
            t: "files.chunk".to_string(),
            d: Some(vec!["peer_beta".to_string()]),
            ..Default::default()
        };
        assert!(file_packet_targets_local(&addressed, "peer_beta"));
        assert!(!file_packet_targets_local(&addressed, "peer_gamma_observer"));

        // Legacy unaddressed packet: still accepted (compatibility), so a sender
        // predating unicast addressing keeps working.
        let legacy = WireMessage {
            t: "files.batch".to_string(),
            ..Default::default()
        };
        assert!(legacy.d.is_none());
        assert!(file_packet_targets_local(&legacy, "peer_beta"));

        // Explicit empty destination list matches nobody.
        let empty = WireMessage {
            t: "files.done".to_string(),
            d: Some(Vec::new()),
            ..Default::default()
        };
        assert!(!file_packet_targets_local(&empty, "peer_beta"));
    }

    #[test]
    fn test_multi_peer_transfer_isolation_loopback() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            use sha2::Digest;
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            // 1. Bind loopback server
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind listener");
            let srv_addr = listener.local_addr().expect("local addr");

            // 2. Setup server channels to feed packet forwarder
            let (tx_a_in, mut rx_a_in) = mpsc::channel::<WireMessage>(20);
            let (tx_b_in, mut rx_b_in) = mpsc::channel::<WireMessage>(20);

            // Connect Peer Alpha
            let mut client_alpha = tokio::net::TcpStream::connect(srv_addr).await.expect("connect alpha");
            let (mut srv_conn_alpha, addr_alpha) = listener.accept().await.expect("accept alpha");

            // Connect Peer Beta
            let mut client_beta = tokio::net::TcpStream::connect(srv_addr).await.expect("connect beta");
            let (mut srv_conn_beta, addr_beta) = listener.accept().await.expect("accept beta");

            tokio::spawn(async move {
                while let Some(msg) = rx_a_in.recv().await {
                    let json = serde_json::to_vec(&msg).unwrap();
                    let len = (json.len() as u32).to_be_bytes();
                    if srv_conn_alpha.write_all(&len).await.is_err() || srv_conn_alpha.write_all(&json).await.is_err() {
                        break;
                    }
                }
            });

            tokio::spawn(async move {
                while let Some(msg) = rx_b_in.recv().await {
                    let json = serde_json::to_vec(&msg).unwrap();
                    let len = (json.len() as u32).to_be_bytes();
                    if srv_conn_beta.write_all(&len).await.is_err() || srv_conn_beta.write_all(&json).await.is_err() {
                        break;
                    }
                }
            });

            let session_alpha = ActiveClientSession {
                id: addr_alpha.to_string(),
                peer_id: Arc::new(RwLock::new(Some("peer_alpha".to_string()))),
                peer_addr: addr_alpha,
                tx: tx_a_in,
                authenticated: Arc::new(RwLock::new(true)),
                outbound: false,
            };

            let session_beta = ActiveClientSession {
                id: addr_beta.to_string(),
                peer_id: Arc::new(RwLock::new(Some("peer_beta".to_string()))),
                peer_addr: addr_beta,
                tx: tx_b_in,
                authenticated: Arc::new(RwLock::new(true)),
                outbound: false,
            };

            let active_clients = Arc::new(RwLock::new(HashMap::new()));
            {
                let mut guard = active_clients.write();
                guard.insert(session_alpha.id.clone(), session_alpha);
                guard.insert(session_beta.id.clone(), session_beta);
            }

            async fn read_framed(stream: &mut tokio::net::TcpStream) -> Option<WireMessage> {
                let mut len_buf = [0u8; 4];
                stream.read_exact(&mut len_buf).await.ok()?;
                let len = u32::from_be_bytes(len_buf) as usize;
                let mut buf = vec![0u8; len];
                stream.read_exact(&mut buf).await.ok()?;
                serde_json::from_slice(&buf).ok()
            }

            let dispatch_msg = |pkt: WireMessage, clients: Arc<RwLock<HashMap<String, ActiveClientSession>>>| {
                let guard = clients.read();
                let is_sensitive = matches!(
                    pkt.t.as_str(),
                    "files.batch" | "files.chunk" | "files.done" | "files.cancel"
                );
                for client in guard.values() {
                    let pid = client.peer_id.read().clone();
                    let matches_dest = match &pkt.d {
                        Some(dests) => dests.iter().any(|d| {
                            d == &client.id
                                || pid.as_deref() == Some(d.as_str())
                                || d == &client.peer_addr.ip().to_string()
                        }),
                        None => true,
                    };
                    if matches_dest && (!is_sensitive || *client.authenticated.read()) {
                        let _ = client.tx.try_send(pkt.clone());
                    }
                }
            };

            let payload_alpha: Vec<u8> = (0..131072).map(|i| (i * 7 % 255) as u8).collect();
            let hash_alpha = format!("{:x}", sha2::Sha256::digest(&payload_alpha));

            let payload_beta: Vec<u8> = (0..131072).map(|i| (i * 13 % 255) as u8).collect();
            let hash_beta = format!("{:x}", sha2::Sha256::digest(&payload_beta));

            assert_ne!(hash_alpha, hash_beta);

            // Dispatch batch headers
            dispatch_msg(WireMessage {
                t: "files.batch".to_string(),
                v: Some(1),
                batch_id: Some("batch_A".to_string()),
                d: Some(vec!["peer_alpha".to_string()]),
                ..Default::default()
            }, active_clients.clone());

            dispatch_msg(WireMessage {
                t: "files.batch".to_string(),
                v: Some(1),
                batch_id: Some("batch_B".to_string()),
                d: Some(vec!["peer_beta".to_string()]),
                ..Default::default()
            }, active_clients.clone());

            // Dispatch chunk 1 of Alpha (0..65536)
            let chunk_a1 = &payload_alpha[0..65536];
            dispatch_msg(WireMessage {
                t: "files.chunk".to_string(),
                v: Some(1),
                batch_id: Some("batch_A".to_string()),
                file_id: Some("file_alpha".to_string()),
                offset: Some(0),
                len: Some(65536),
                data: Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, chunk_a1)),
                sha256_chunk: Some(format!("{:x}", sha2::Sha256::digest(chunk_a1))),
                d: Some(vec!["peer_alpha".to_string()]),
                ..Default::default()
            }, active_clients.clone());

            // Dispatch chunk of Beta (0..131072)
            let chunk_b = &payload_beta[..];
            dispatch_msg(WireMessage {
                t: "files.chunk".to_string(),
                v: Some(1),
                batch_id: Some("batch_B".to_string()),
                file_id: Some("file_beta".to_string()),
                offset: Some(0),
                len: Some(131072),
                data: Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, chunk_b)),
                sha256_chunk: Some(format!("{:x}", sha2::Sha256::digest(chunk_b))),
                d: Some(vec!["peer_beta".to_string()]),
                ..Default::default()
            }, active_clients.clone());

            dispatch_msg(WireMessage {
                t: "files.done".to_string(),
                v: Some(1),
                batch_id: Some("batch_B".to_string()),
                file_id: Some("file_beta".to_string()),
                sha256_total: Some(hash_beta.clone()),
                d: Some(vec!["peer_beta".to_string()]),
                ..Default::default()
            }, active_clients.clone());

            // Resume chunk 2 of Alpha (65536..131072)
            let chunk_a2 = &payload_alpha[65536..131072];
            dispatch_msg(WireMessage {
                t: "files.chunk".to_string(),
                v: Some(1),
                batch_id: Some("batch_A".to_string()),
                file_id: Some("file_alpha".to_string()),
                offset: Some(65536),
                len: Some(65536),
                data: Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, chunk_a2)),
                sha256_chunk: Some(format!("{:x}", sha2::Sha256::digest(chunk_a2))),
                d: Some(vec!["peer_alpha".to_string()]),
                ..Default::default()
            }, active_clients.clone());

            dispatch_msg(WireMessage {
                t: "files.done".to_string(),
                v: Some(1),
                batch_id: Some("batch_A".to_string()),
                file_id: Some("file_alpha".to_string()),
                sha256_total: Some(hash_alpha.clone()),
                d: Some(vec!["peer_alpha".to_string()]),
                ..Default::default()
            }, active_clients.clone());

            // Read on Peer Alpha
            let pkt_a_batch = read_framed(&mut client_alpha).await.expect("pkt a batch");
            assert_eq!(pkt_a_batch.batch_id.as_deref(), Some("batch_A"));

            let pkt_a_c1 = read_framed(&mut client_alpha).await.expect("pkt a chunk 1");
            assert_eq!(pkt_a_c1.offset, Some(0));

            let pkt_a_c2 = read_framed(&mut client_alpha).await.expect("pkt a chunk 2 (resumed)");
            assert_eq!(pkt_a_c2.offset, Some(65536));

            let pkt_a_done = read_framed(&mut client_alpha).await.expect("pkt a done");
            assert_eq!(pkt_a_done.sha256_total.as_deref(), Some(hash_alpha.as_str()));

            // Read on Peer Beta
            let pkt_b_batch = read_framed(&mut client_beta).await.expect("pkt b batch");
            assert_eq!(pkt_b_batch.batch_id.as_deref(), Some("batch_B"));

            let pkt_b_chunk = read_framed(&mut client_beta).await.expect("pkt b chunk");
            assert_eq!(pkt_b_chunk.batch_id.as_deref(), Some("batch_B"));

            let pkt_b_done = read_framed(&mut client_beta).await.expect("pkt b done");
            assert_eq!(pkt_b_done.sha256_total.as_deref(), Some(hash_beta.as_str()));

            // Verify reassembled data and checksum on Alpha
            let data_a1 = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, pkt_a_c1.data.unwrap()).unwrap();
            let data_a2 = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, pkt_a_c2.data.unwrap()).unwrap();
            let mut assembled_a = data_a1;
            assembled_a.extend_from_slice(&data_a2);
            assert_eq!(assembled_a, payload_alpha);
            assert_eq!(format!("{:x}", sha2::Sha256::digest(&assembled_a)), hash_alpha);

            // Verify reassembled data and checksum on Beta
            let data_b = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, pkt_b_chunk.data.unwrap()).unwrap();
            assert_eq!(data_b, payload_beta);
            assert_eq!(format!("{:x}", sha2::Sha256::digest(&data_b)), hash_beta);
        });
    }
}
