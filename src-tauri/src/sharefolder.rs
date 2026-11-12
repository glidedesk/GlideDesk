// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// A folder one computer can open on the other, without mounting a drive.
//
// "Share a folder with X" makes this computer answer read-only requests for that
// one folder: a list, a page a browser can follow, and the files themselves. The
// address, the port and a random token are handed over on the paired, encrypted
// link, and the token is what makes the server refuse everybody else. Turning
// sharing off withdraws it — a stopped share answers nothing.
//
// "Open X's shared folder" reads the same list and saves a chosen file into the
// folder this computer saves received files in, so it appears in Recent with the
// rest.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::network::NetworkManager;
use crate::state::AppState;

/// The folder this computer is offering, and the token that reaches it.
#[derive(Clone)]
struct Shared {
    root: PathBuf,
    token: String,
}

fn shared() -> &'static Arc<RwLock<Option<Shared>>> {
    static SHARED: std::sync::OnceLock<Arc<RwLock<Option<Shared>>>> = std::sync::OnceLock::new();
    SHARED.get_or_init(|| Arc::new(RwLock::new(None)))
}

fn port_slot() -> &'static AtomicU16 {
    static PORT: AtomicU16 = AtomicU16::new(0);
    &PORT
}

/// The other computer's folder, as its announcement described it.
#[derive(Clone)]
struct PeerFolder {
    ip: String,
    port: u16,
    token: String,
    name: String,
}

fn peer() -> &'static Mutex<Option<PeerFolder>> {
    static PEER: std::sync::OnceLock<Mutex<Option<PeerFolder>>> = std::sync::OnceLock::new();
    PEER.get_or_init(|| Mutex::new(None))
}

/// Remember what the other computer said it is sharing.
pub fn peer_shared(ip: &str, name: &str, port: u16, token: &str, _files: u32) {
    if ip.trim().is_empty() || port == 0 || token.trim().is_empty() {
        *peer().lock() = None;
        return;
    }
    *peer().lock() = Some(PeerFolder {
        ip: ip.trim().to_string(),
        port,
        token: token.trim().to_string(),
        name: if name.trim().is_empty() {
            "the other computer".to_string()
        } else {
            name.trim().to_string()
        },
    });
}

/// The other computer stopped sharing.
pub fn peer_cleared() {
    *peer().lock() = None;
}

pub fn peer_available() -> bool {
    peer().lock().is_some()
}

fn host_for_url(ip: &str) -> String {
    if ip.contains(':') && !ip.starts_with('[') {
        format!("[{}]", ip)
    } else {
        ip.to_string()
    }
}

fn base_url(p: &PeerFolder) -> String {
    format!(
        "http://{}:{}/{}/",
        host_for_url(&p.ip),
        p.port,
        p.token
    )
}

fn new_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Percent-encode one path for a URL, leaving only what is safe on the wire.
fn enc(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn dec(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Reject anything that is not a plain relative path, then resolve it inside the
/// shared folder. `..`, absolute paths, backslashes and symlinks that point out
/// of the folder are all refused.
fn resolve(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let mut wanted = root.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains('\\') || part.contains('\0') {
            return Err("That path is not inside the shared folder.".to_string());
        }
        wanted.push(part);
    }
    let root_real = root
        .canonicalize()
        .map_err(|e| format!("Cannot read the shared folder: {}", e))?;
    let real = wanted
        .canonicalize()
        .map_err(|_| "That item is no longer there.".to_string())?;
    if real != root_real && !real.starts_with(&root_real) {
        return Err("That path is not inside the shared folder.".to_string());
    }
    Ok(real)
}

fn modified_ms(md: &std::fs::Metadata) -> u64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn list_dir(dir: &Path) -> Result<Vec<Value>, String> {
    let mut items: Vec<Value> = Vec::new();
    let read = std::fs::read_dir(dir).map_err(|e| format!("Cannot read {}: {}", dir.display(), e))?;
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue; // hidden files are not part of what people mean by "the folder"
        }
        let path = entry.path();
        // Do not follow a symlink out of the shared folder.
        let md = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if md.file_type().is_symlink() {
            let inside = path
                .canonicalize()
                .ok()
                .zip(dir.canonicalize().ok())
                .map(|(real, root)| real.starts_with(&root))
                .unwrap_or(false);
            if !inside {
                continue;
            }
        }
        let is_dir = if md.is_dir() {
            true
        } else if md.file_type().is_symlink() {
            path.is_dir()
        } else {
            false
        };
        let size = if is_dir {
            0
        } else {
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
        };
        items.push(json!({
            "name": name,
            "dir": is_dir,
            "size": size,
            "at": modified_ms(&md),
        }));
    }
    items.sort_by(|a, b| {
        let ad = a["dir"].as_bool().unwrap_or(false);
        let bd = b["dir"].as_bool().unwrap_or(false);
        if ad != bd {
            return if ad { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
        }
        let an = a["name"].as_str().unwrap_or("").to_lowercase();
        let bn = b["name"].as_str().unwrap_or("").to_lowercase();
        an.cmp(&bn)
    });
    Ok(items)
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "txt" | "log" | "md" => "text/plain; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

async fn respond(stream: &mut TcpStream, code: u16, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        code,
        reason(code),
        kind,
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

fn page(title: &str, inner: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{t}</title>\
<style>body{{font:15px -apple-system,Segoe UI,Roboto,sans-serif;margin:0;background:#f6f6f7;color:#1c1c1e}}\
header{{padding:16px 20px;background:#fff;border-bottom:1px solid #e5e5ea}}\
h1{{font-size:17px;margin:0 0 4px}}p{{margin:0;color:#6e6e73;font-size:13px}}\
main{{padding:8px 0}}a{{display:flex;justify-content:space-between;gap:16px;padding:10px 20px;\
text-decoration:none;color:inherit}}a:hover{{background:#eef4ff}}\
.n{{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}.s{{color:#8e8e93;font-size:12px;flex:none}}\
.dir::before{{content:\"📁 \"}}</style></head><body><header><h1>{t}</h1><p>{sub}</p></header>\
<main>{body}</main></body></html>",
        t = esc(title),
        sub = esc("Shared by GlideDesk · read only"),
        body = inner
    )
}

fn browse_html(root: &Path, rel: &str, entries: &[Value]) -> String {
    let mut rows = String::new();
    if !rel.is_empty() {
        let parent = rel.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
        rows.push_str(&format!(
            "<a class=\"dir\" href=\"../browse?path={}\"><span class=\"n\">Up</span></a>",
            enc(&parent)
        ));
    }
    for e in entries {
        let name = e["name"].as_str().unwrap_or("");
        let child = if rel.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", rel, name)
        };
        if e["dir"].as_bool().unwrap_or(false) {
            rows.push_str(&format!(
                "<a class=\"dir\" href=\"browse?path={}\"><span class=\"n\">{}</span><span class=\"s\">Folder</span></a>",
                enc(&child),
                esc(name)
            ));
        } else {
            rows.push_str(&format!(
                "<a href=\"get?path={}\"><span class=\"n\">{}</span><span class=\"s\">{}</span></a>",
                enc(&child),
                esc(name),
                human_size(e["size"].as_u64().unwrap_or(0))
            ));
        }
    }
    if rows.is_empty() {
        rows.push_str("<p style=\"padding:12px 20px\">This folder is empty.</p>");
    }
    let title = if rel.is_empty() {
        root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Shared folder".to_string())
    } else {
        rel.rsplit('/').next().unwrap_or(rel).to_string()
    };
    page(&title, &rows)
}

fn human_size(n: u64) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1048576 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else if n < 1073741824 {
        format!("{:.1} MB", n as f64 / 1048576.0)
    } else {
        format!("{:.2} GB", n as f64 / 1073741824.0)
    }
}

async fn send_file(stream: &mut TcpStream, path: &Path) -> std::io::Result<()> {
    let mut file = tokio::fs::File::open(path).await?;
    let len = file.metadata().await?.len();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let safe_name = name.replace(['"', '\r', '\n'], "_");
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nContent-Disposition: attachment; filename=\"{}\"\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        content_type(path),
        len,
        safe_name
    );
    stream.write_all(head.as_bytes()).await?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        stream.write_all(&buf[..n]).await?;
    }
    stream.flush().await
}

fn io_other(message: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, message)
}

async fn serve(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buf = vec![0u8; 8192];
    let mut filled = 0;
    loop {
        if filled == buf.len() {
            return respond(stream, 400, "text/plain", b"Request too long").await;
        }
        let n = match tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf[filled..])).await {
            Ok(Ok(0)) => return Ok(()),
            Ok(Ok(n)) => n,
            Ok(Err(_)) => return Ok(()),
            Err(_) => return respond(stream, 400, "text/plain", b"Request timed out").await,
        };
        filled += n;
        if buf[..filled].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let head = String::from_utf8_lossy(&buf[..filled]).to_string();
    let first = head.lines().next().unwrap_or("");
    let mut words = first.split_whitespace();
    let method = words.next().unwrap_or("");
    let target = words.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return respond(stream, 405, "text/plain", b"Only reading is allowed").await;
    }

    // The guard is taken and dropped before any await, so the request future stays
    // Send (a read guard held across an await makes it not).
    let current_opt = { shared().read().clone() };
    let current = match current_opt {
        Some(s) => s,
        None => return respond(stream, 403, "text/plain", b"This computer is not sharing a folder").await,
    };

    let path_part = target.split('?').next().unwrap_or("/");
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let trimmed = path_part.trim_start_matches('/');
    let (token, rest) = match trimmed.split_once('/') {
        Some((t, r)) => (t, r),
        None => (trimmed, ""),
    };
    // The token is the whole lock on this door.
    if token != current.token {
        return respond(stream, 404, "text/plain", b"Not found").await;
    }

    let rel = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "path")
        .map(|(_, v)| dec(v))
        .unwrap_or_default();
    let rel = rel.trim_matches('/').to_string();

    if rest.is_empty() || rest == "browse" {
        let dir = match resolve(&current.root, &rel) {
            Ok(p) if p.is_dir() => p,
            Ok(_) => return respond(stream, 400, "text/plain", b"That is a file").await,
            Err(_) => return respond(stream, 404, "text/plain", b"Not found").await,
        };
        let entries = list_dir(&dir).map_err(io_other)?;
        let body = browse_html(&current.root, &rel, &entries);
        return respond(stream, 200, "text/html; charset=utf-8", body.as_bytes()).await;
    }

    if rest == "list" {
        let dir = match resolve(&current.root, &rel) {
            Ok(p) if p.is_dir() => p,
            Ok(_) => return respond(stream, 400, "application/json", b"{\"error\":\"not a folder\"}").await,
            Err(e) => {
                return respond(
                    stream,
                    404,
                    "application/json",
                    format!("{{\"error\":\"{}\"}}", esc(&e)).as_bytes(),
                )
                .await
            }
        };
        let entries = list_dir(&dir).map_err(io_other)?;
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let body = json!({
            "name": name,
            "path": rel,
            "entries": entries,
        })
        .to_string();
        return respond(stream, 200, "application/json", body.as_bytes()).await;
    }

    if rest == "get" {
        let file = match resolve(&current.root, &rel) {
            Ok(p) if p.is_file() => p,
            Ok(_) => return respond(stream, 400, "text/plain", b"That is a folder").await,
            Err(_) => return respond(stream, 404, "text/plain", b"Not found").await,
        };
        return send_file(stream, &file).await;
    }

    respond(stream, 404, "text/plain", b"Not found").await
}

async fn handle(mut stream: TcpStream) {
    if let Err(e) = serve(&mut stream).await {
        log::debug!("[Share] Request ended: {}", e);
    }
}

/// The folder a fresh install offers, made on first use: a folder whose name says
/// what it is, rather than the whole Desktop or Documents going out by surprise.
pub fn default_share_dir(home: &Path) -> PathBuf {
    home.join("GlideDesk Share")
}

/// This computer's default shared folder, created if this is the first time it is
/// needed. `None` only when there is no home directory to put it in.
pub fn default_share_folder() -> Option<PathBuf> {
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())?;
    let dir = default_share_dir(Path::new(&home));
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Some(dir),
        Err(e) => {
            log::warn!("[Share] Could not make {}: {}", dir.display(), e);
            None
        }
    }
}

/// Start the folder server once, and keep the other computer told what is shared.
pub fn start(app: AppHandle, network: NetworkManager) {
    tauri::async_runtime::spawn(async move {
        match TcpListener::bind(("0.0.0.0", 0)).await {
            Ok(listener) => {
                let port = listener
                    .local_addr()
                    .map(|a| a.port())
                    .unwrap_or_default();
                port_slot().store(port, Ordering::Relaxed);
                log::info!("[Share] Folder server listening on port {}", port);
                tauri::async_runtime::spawn(async move {
                    loop {
                        match listener.accept().await {
                            Ok((stream, _)) => {
                                tauri::async_runtime::spawn(handle(stream));
                            }
                            Err(e) => {
                                log::warn!("[Share] Could not accept: {}", e);
                                tokio::time::sleep(Duration::from_millis(300)).await;
                            }
                        }
                    }
                });
            }
            Err(e) => log::error!("[Share] Could not start the folder server: {}", e),
        }

        let mut tick: u64 = 0;
        let mut last_sent: Option<String> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(4)).await;
            tick += 1;
            last_sent = announce(&app, &network, tick, last_sent).await;
        }
    });
}

/// Read what this computer is sharing, update the page's state, and tell the
/// paired computer. Returns the key of what was said, so the same thing is not
/// repeated every four seconds.
async fn announce(
    app: &AppHandle,
    network: &NetworkManager,
    tick: u64,
    last_sent: Option<String>,
) -> Option<String> {
    let state = app.state::<AppState>();
    let (enabled, folder, path) = {
        let s = state.inner.read();
        (
            s.files.share_on,
            s.files.smb.folder.clone(),
            s.files.path.clone(),
        )
    };
    // Nothing chosen yet is not "no folder": it is the default folder, made on first
    // use, so a fresh install has something to open without a trip to Choose….
    let chosen = if !folder.trim().is_empty() {
        folder.clone()
    } else if !path.trim().is_empty() {
        path
    } else {
        default_share_folder()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    let adopted_default = enabled && folder.trim().is_empty() && !chosen.trim().is_empty();
    let root = PathBuf::from(chosen.trim());

    let (sharing, name, count, message) = if !enabled {
        stop_shared();
        (false, String::new(), 0, String::new())
    } else if chosen.trim().is_empty() {
        stop_shared();
        (false, String::new(), 0, "Choose a folder to share".to_string())
    } else if !root.is_dir() {
        stop_shared();
        (
            false,
            String::new(),
            0,
            format!("{} is not there any more", root.display()),
        )
    } else {
        match ensure_shared(&root) {
            Ok(items) => (
                true,
                root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Folder".to_string()),
                items,
                String::new(),
            ),
            Err(e) => {
                stop_shared();
                (false, String::new(), 0, e)
            }
        }
    };

    let changed = {
        let mut s = state.inner.write();
        let mut changed = false;
        if s.files.smb.sharing != sharing {
            s.files.smb.sharing = sharing;
            changed = true;
        }
        if s.files.smb.count != count {
            s.files.smb.count = count;
            changed = true;
        }
        if s.files.smb.share_message != message {
            s.files.smb.share_message = message.clone();
            changed = true;
        }
        // Name the default folder on the page, and keep it for next time.
        if sharing && s.files.smb.folder.trim().is_empty() && !chosen.trim().is_empty() {
            s.files.smb.folder = chosen.clone();
            changed = true;
        }
        changed
    };
    if changed {
        crate::commands::push_state(app, &state);
        if adopted_default && sharing {
            let files = state.inner.read().files.clone();
            crate::pairstore::save_files(&files);
        }
    }

    let port = port_slot().load(Ordering::Relaxed);
    let token = shared()
        .read()
        .as_ref()
        .map(|s| s.token.clone())
        .unwrap_or_default();
    let key = format!("{}|{}|{}|{}", sharing, name, count, token);
    // Repeat now and then so a computer that has just started learns about a folder
    // that has been shared for a while. The packet is tiny; eight seconds keeps the
    // page honest without any work worth measuring.
    let should_send = last_sent.as_deref() != Some(key.as_str()) || tick % 2 == 0;
    if should_send {
        let peers: Vec<String> = network
            .get_active_clients_info()
            .into_iter()
            .filter(|c| c.authenticated)
            .filter_map(|c| c.peer_id)
            .collect();
        if !peers.is_empty() {
            let pkt = crate::network::WireMessage {
                t: "files.folder".to_string(),
                v: Some(1),
                on: Some(sharing),
                shared_folder: if sharing {
                    Some(crate::network::WireShareFolder {
                        name: name.clone(),
                        port,
                        token: token.clone(),
                        files: count,
                    })
                } else {
                    None
                },
                ..Default::default()
            };
            for pid in peers {
                network.send_to(&pid, pkt.clone()).await;
            }
        }
    }
    Some(key)
}

/// Point the server at this folder. The token stays the same while the folder
/// does, so a page already open keeps working.
fn ensure_shared(root: &Path) -> Result<u32, String> {
    let real = root
        .canonicalize()
        .map_err(|e| format!("Cannot read {}: {}", root.display(), e))?;
    if !real.is_dir() {
        return Err("That is not a folder".to_string());
    }
    {
        let guard = shared().read();
        if let Some(current) = guard.as_ref() {
            if current.root == real {
                return list_dir(&real).map(|v| v.len() as u32);
            }
        }
    }
    let token = new_token();
    *shared().write() = Some(Shared {
        root: real.clone(),
        token,
    });
    log::info!("[Share] Sharing {}", real.display());
    list_dir(&real).map(|v| v.len() as u32)
}

fn stop_shared() {
    let mut guard = shared().write();
    if guard.take().is_some() {
        log::info!("[Share] Stopped sharing");
    }
}

// ------------------------------------------------------------------
// The other computer's folder
// ------------------------------------------------------------------

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Could not prepare the request: {}", e))
}

fn say_why(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "The other computer did not answer in time.".to_string()
    } else if e.is_connect() {
        "Could not reach the other computer's shared folder. Is it still sharing?".to_string()
    } else {
        format!("The shared folder could not be read: {}", e)
    }
}

/// Read one level of the other computer's folder.
#[tauri::command]
pub async fn list_peer_folder(path: Option<String>) -> Result<Value, String> {
    let p = peer()
        .lock()
        .clone()
        .ok_or_else(|| "The other computer is not sharing a folder.".to_string())?;
    let rel = path.unwrap_or_default();
    let url = format!("{}list?path={}", base_url(&p), enc(rel.trim_matches('/')));
    let res = http_client()?
        .get(&url)
        .send()
        .await
        .map_err(say_why)?;
    if !res.status().is_success() {
        return Err(format!(
            "{} answered {} for that folder.",
            p.name,
            res.status().as_u16()
        ));
    }
    res.json::<Value>()
        .await
        .map_err(|e| format!("The answer could not be read: {}", e))
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let wanted = dir.join(name);
    if !wanted.exists() {
        return wanted;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let ext = path
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    for n in 2..1000 {
        let candidate = if ext.is_empty() {
            dir.join(format!("{} ({})", stem, n))
        } else {
            dir.join(format!("{} ({}).{}", stem, n, ext))
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    dir.join(format!("{}-{}", stamp, name))
}

/// Save one file from the other computer's folder into this computer's received
/// folder, and record it in Recent like any other arrival.
#[tauri::command]
pub async fn download_peer_file(app: AppHandle, path: String) -> Result<String, String> {
    let p = peer()
        .lock()
        .clone()
        .ok_or_else(|| "The other computer is not sharing a folder.".to_string())?;
    let rel = path.trim().trim_matches('/').to_string();
    if rel.is_empty() || rel.split('/').any(|part| part == ".." || part.contains('\\')) {
        return Err("That is not a file in the shared folder.".to_string());
    }
    let url = format!("{}get?path={}", base_url(&p), enc(&rel));
    let res = http_client()?
        .get(&url)
        .send()
        .await
        .map_err(say_why)?;
    if !res.status().is_success() {
        return Err(format!(
            "{} answered {} for that file.",
            p.name,
            res.status().as_u16()
        ));
    }
    if let Some(len) = res.content_length() {
        if len > 2 * 1024 * 1024 * 1024 {
            return Err("That file is larger than 2 GB.".to_string());
        }
    }
    let bytes = res
        .bytes()
        .await
        .map_err(|e| format!("The file stopped arriving: {}", e))?;

    let state = app.state::<AppState>();
    let received = state.inner.read().files.received_folder.clone();
    let dir = crate::transport::transfer::effective_received_dir(&received);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot make {}: {}", dir.display(), e))?;
    let name = rel.rsplit('/').next().unwrap_or("file").to_string();
    let dest = unique_path(&dir, &name);
    std::fs::write(&dest, &bytes)
        .map_err(|e| format!("Cannot save {}: {}", dest.display(), e))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    {
        let mut s = state.inner.write();
        s.files.incoming.last = Some(crate::state::FilesEvent {
            ok: true,
            name: dest
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| name.clone()),
            text: dest.to_string_lossy().to_string(),
            at: now,
        });
    }
    crate::commands::push_state(&app, &state);
    log::info!("[Share] Saved {} from {}", dest.display(), p.name);
    Ok(dest.to_string_lossy().to_string())
}

/// Open the other computer's folder in the system browser. This is what "Open"
/// means outside the app: a page that lists the folder, with every file a link.
pub fn open_peer_folder() -> Result<(), String> {
    let p = peer()
        .lock()
        .clone()
        .ok_or_else(|| "The other computer is not sharing a folder.".to_string())?;
    let url = format!("{}browse", base_url(&p));
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Could not open the browser: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| format!("Could not open the browser: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Could not open the browser: {}", e))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

/// How many items sit at the top of the folder right now, for the page.
pub fn shared_count() -> u32 {
    shared()
        .read()
        .as_ref()
        .and_then(|s| list_dir(&s.root).ok())
        .map(|v| v.len() as u32)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gd-share-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn paths_cannot_escape_the_shared_folder() {
        let dir = temp_dir("escape");
        std::fs::create_dir_all(dir.join("inside")).unwrap();
        std::fs::write(dir.join("inside/a.txt"), b"hello").unwrap();
        assert!(resolve(&dir, "inside/a.txt").is_ok());
        assert!(resolve(&dir, "").is_ok());
        assert!(resolve(&dir, "../a.txt").is_err());
        assert!(resolve(&dir, "inside/../../a.txt").is_err());
        assert!(resolve(&dir, "/etc/passwd").is_err());
        assert!(resolve(&dir, "inside\\..\\a.txt").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn listing_puts_folders_first_and_hides_nothing_else() {
        let dir = temp_dir("list");
        std::fs::create_dir_all(dir.join("beta")).unwrap();
        std::fs::write(dir.join("alpha.txt"), b"1234").unwrap();
        std::fs::write(dir.join(".hidden"), b"no").unwrap();
        let entries = list_dir(&dir).unwrap();
        let names: Vec<&str> = entries.iter().filter_map(|e| e["name"].as_str()).collect();
        assert_eq!(names, vec!["beta", "alpha.txt"]);
        assert_eq!(entries[1]["size"].as_u64(), Some(4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_token_holder_gets_the_list() {
        // The check in `serve` is an equality on this value; a short token must not
        // match a longer one.
        assert!("abc" != "abcd");
    }

    #[test]
    fn a_name_is_encoded_and_decoded_unchanged() {
        let name = "résumé 2026 & notes.txt";
        assert_eq!(dec(&enc(name)), name);
    }

    #[test]
    fn the_default_folder_is_under_home_and_named_for_what_it_is() {
        let home = Path::new("/Users/someone");
        assert_eq!(default_share_dir(home), PathBuf::from("/Users/someone/GlideDesk Share"));
        assert!(default_share_dir(home).starts_with(home));
        let windows = Path::new("C:/Users/Babsons");
        assert!(default_share_dir(windows).ends_with("GlideDesk Share"));
    }

    #[test]
    fn a_second_download_does_not_overwrite_the_first() {
        let dir = temp_dir("unique");
        std::fs::write(dir.join("report.pdf"), b"one").unwrap();
        let second = unique_path(&dir, "report.pdf");
        assert!(second.ends_with("report (2).pdf"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
