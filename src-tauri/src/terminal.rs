// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// A shell on the other computer.
//
// The two computers already trust each other, so the terminal does not ask for a
// password: GlideDesk makes one SSH key for itself, offers its public half over the
// paired connection (`terminal.key`), and the other computer adds that line to the
// list of keys it lets in. Then Open Terminal is a plain
// `ssh -i <our key> <user>@<the other computer>` in the terminal app this computer has.
//
// Both directions use the same code, because both sides send their key and both sides
// answer with their own user name and whether their SSH server is running.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, Manager};

use crate::commands::push_state;
use crate::network::NetworkManager;
use crate::state::AppState;

/// The folder GlideDesk keeps its own SSH key in, beside `pairs.json`.
pub fn ssh_dir() -> PathBuf {
    let mut p = crate::pairstore::store_path();
    p.set_file_name("ssh");
    p
}

pub fn key_path() -> PathBuf {
    ssh_dir().join("glidedesk_ed25519")
}

fn allow_key_path() -> PathBuf {
    ssh_dir().join("peer_allow.pub")
}

/// The name the other computer knows this one by.
fn local_name(state: &AppState) -> String {
    let host = state.inner.read().about.pc.host.clone();
    if host.is_empty() {
        "GlideDesk".to_string()
    } else {
        host
    }
}

/// The login name this computer answers to over SSH.
pub fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

fn ssh_keygen_bin() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        if std::path::Path::new(r"C:\Windows\System32\OpenSSH\ssh-keygen.exe").exists() {
            return r"C:\Windows\System32\OpenSSH\ssh-keygen.exe";
        }
        return "ssh-keygen.exe";
    }
    #[allow(unreachable_code)]
    "ssh-keygen"
}

fn ssh_bin() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        if std::path::Path::new(r"C:\Windows\System32\OpenSSH\ssh.exe").exists() {
            return r"C:\Windows\System32\OpenSSH\ssh.exe";
        }
        return "ssh.exe";
    }
    #[allow(unreachable_code)]
    "ssh"
}

/// Make the key if it is not there yet, and return the public half. One key per
/// install: making a new one on every run would lock out the other computer.
pub fn ensure_key() -> Result<String, String> {
    let dir = ssh_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot make {}: {}", dir.display(), e))?;
    let key = key_path();
    if !key.exists() {
        let out = Command::new(ssh_keygen_bin())
            .args(["-t", "ed25519", "-N", "", "-C", "glidedesk", "-f"])
            .arg(&key)
            .output()
            .map_err(|e| format!("ssh-keygen could not start: {}", e))?;
        if !out.status.success() {
            return Err(format!(
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        log::info!("[Terminal] Made this computer's SSH key at {}", key.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600));
    }
    restrict_to_me(&key);
    let pub_path = key.with_extension("pub");
    let public = std::fs::read_to_string(&pub_path)
        .map_err(|e| format!("Cannot read {}: {}", pub_path.display(), e))?;
    Ok(public.trim().to_string())
}

/// Windows' ssh refuses a private key any other account can read, so take the
/// inheritance off and leave it to this user alone.
fn restrict_to_me(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let user = local_user();
        if user.is_empty() {
            return;
        }
        let _ = Command::new("icacls")
            .arg(path)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{}:F", user))
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
    }
}

/// A public key we are willing to add: one line, ssh-… , base64, optional comment.
pub fn valid_public_key(text: &str) -> bool {
    let line = text.trim();
    if line.is_empty() || line.len() > 2048 || line.contains('\n') || line.contains('\r') {
        return false;
    }
    let mut parts = line.split_whitespace();
    let kind = parts.next().unwrap_or("");
    let body = parts.next().unwrap_or("");
    let known = kind == "ssh-ed25519"
        || kind == "ssh-rsa"
        || kind.starts_with("ecdsa-sha2-")
        || kind.starts_with("sk-ssh-")
        || kind.starts_with("sk-ecdsa-");
    // The rest is only a comment, but it still goes into a file sshd parses, so
    // keep quotes, backslashes and shell punctuation out of it.
    let comment_ok = parts.next().map_or(true, |c| {
        !c.chars().any(|ch| {
            ch.is_control() || matches!(ch, '"' | '\'' | '\\' | '`' | '$')
        })
    });
    known
        && body.len() >= 32
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        && comment_ok
}

fn ssh_home_keys() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        return PathBuf::from(home).join(".ssh").join("authorized_keys");
    }
    #[allow(unreachable_code)]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".ssh").join("authorized_keys")
    }
}

#[cfg(target_os = "windows")]
fn is_admin() -> bool {
    // Whether this *account* is an administrator, not whether this process is
    // elevated: Windows' sshd reads administrators' keys from a machine-wide file
    // even when the app runs without elevation.
    Command::new("whoami")
        .args(["/groups"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("S-1-5-32-544"))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn admin_authorized_keys() -> PathBuf {
    PathBuf::from(r"C:\ProgramData\ssh\administrators_authorized_keys")
}

/// After the elevated step has put the other computer's key where Windows reads it,
/// this file says so. A normal process cannot even read the machine-wide key file, so
/// without the note the app could never tell that the step had worked.
#[cfg(target_os = "windows")]
fn admin_key_marker() -> PathBuf {
    ssh_dir().join("admin_key_installed")
}

/// Remember a key the other computer offered, so the one-time administrator step can
/// also let it in where Windows keeps keys for administrators.
fn remember_peer_key(pubkey: &str) {
    let path = allow_key_path();
    let wanted = pubkey.trim().to_string();
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.trim() != wanted {
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(&ssh_dir()));
        if let Ok(mut f) = std::fs::File::create(&path) {
            let _ = writeln!(f, "{}", wanted);
        }
    }
}

fn append_line(path: &std::path::Path, line: &str) -> std::io::Result<bool> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line.trim()) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line.trim());
    text.push('\n');
    std::fs::write(path, text)?;
    Ok(true)
}

/// Add the other computer's key to the list this computer lets in. Safe to call on
/// every packet: the same line is only ever written once.
pub fn install_authorized_key(pubkey: &str) -> Result<(), String> {
    if !valid_public_key(pubkey) {
        return Err("The other computer sent an unusable key".to_string());
    }
    remember_peer_key(pubkey);
    #[cfg(target_os = "windows")]
    {
        // Windows' sshd reads an administrator's keys from a machine-wide file, not
        // the user's. Add it there too when the file can be written; otherwise the
        // one-time Set up step does it.
        if is_admin() {
            let marked = std::fs::read_to_string(admin_key_marker()).unwrap_or_default();
            if marked.trim() != pubkey.trim() {
                let admin_file = admin_authorized_keys();
                match append_line(&admin_file, pubkey) {
                    Ok(_) => {
                        let _ = std::fs::write(admin_key_marker(), pubkey.trim());
                        log::info!("[Terminal] Added the key to {}", admin_file.display());
                    }
                    Err(e) => log::debug!(
                        "[Terminal] {} is written by the one-time Set up ({})",
                        admin_file.display(),
                        e
                    ),
                }
            }
        }
    }
    let path = ssh_home_keys();
    let added = append_line(&path, pubkey).map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(dir) = path.parent() {
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    if added {
        log::info!("[Terminal] Let the other computer in: added its key to {}", path.display());
    }
    Ok(())
}

/// Whether the key we installed is one this computer's sshd will really read. On
/// Windows an administrator's keys are read from a machine-wide file instead of the
/// user's, and only the elevated setup step can write it.
pub fn peer_key_accepted(pubkey: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        if is_admin() {
            // The elevated step leaves a note naming the key it wrote; a normal
            // process cannot read the machine-wide file itself.
            let marked = std::fs::read_to_string(admin_key_marker()).unwrap_or_default();
            if marked.trim() == pubkey.trim() {
                return true;
            }
            let text = std::fs::read_to_string(admin_authorized_keys()).unwrap_or_default();
            return text.lines().any(|l| l.trim() == pubkey.trim());
        }
    }
    let user_file = std::fs::read_to_string(ssh_home_keys()).unwrap_or_default();
    user_file.lines().any(|l| l.trim() == pubkey.trim())
}

/// "running", "stopped", "missing" or "unknown", plus a line a person can act on.
pub fn sshd_service() -> (String, String) {
    #[cfg(target_os = "macos")]
    {
        let enabled = Command::new("launchctl")
            .args(["print", "system/com.openssh.sshd"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if enabled {
            return ("running".to_string(), "Remote Login is on. The other computer can open a terminal here.".to_string());
        }
        return (
            "stopped".to_string(),
            "Remote Login is off. Set up turns it on (asks for your password once).".to_string(),
        );
    }
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("sc.exe").args(["query", "sshd"]).output();
        let text = out
            .as_ref()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if text.contains("RUNNING") {
            return ("running".to_string(), "OpenSSH Server is running. The other computer can open a terminal here.".to_string());
        }
        if text.contains("STOPPED") {
            return ("stopped".to_string(), "OpenSSH Server is installed but stopped. Set up starts it (asks for administrator rights once).".to_string());
        }
        if text.contains("1060") || text.is_empty() {
            return ("missing".to_string(), "OpenSSH Server is not installed. Add it in Settings › System › Optional features › OpenSSH Server.".to_string());
        }
        return ("unknown".to_string(), "Could not read the OpenSSH Server service state.".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        for name in ["sshd", "ssh"] {
            if let Ok(out) = Command::new("systemctl").args(["is-active", name]).output() {
                if String::from_utf8_lossy(&out.stdout).trim() == "active" {
                    return ("running".to_string(), "sshd is running. The other computer can open a terminal here.".to_string());
                }
            }
        }
        return ("stopped".to_string(), "sshd is not running. Set up starts it (asks for administrator rights once).".to_string());
    }
    #[allow(unreachable_code)]
    ("unknown".to_string(), String::new())
}

/// Start this computer's SSH server, asking for administrator rights once if the
/// system needs them. Blocking: call it off the UI thread.
pub fn enable_sshd() -> Result<String, String> {
    let (service, _) = sshd_service();
    if service == "running" {
        return Ok("The SSH server is already running.".to_string());
    }
    if service == "missing" {
        return Err("OpenSSH Server is not installed on this computer.".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        let script = "do shell script \"systemsetup -setremotelogin on\" with administrator privileges";
        let out = Command::new("osascript").args(["-e", script]).output();
        match out {
            Ok(o) if o.status.success() => {
                log::info!("[Terminal] Remote Login turned on");
            }
            Ok(o) => {
                let why = String::from_utf8_lossy(&o.stderr).trim().to_string();
                log::warn!("[Terminal] Remote Login was not turned on: {}", why);
                return Err(if why.is_empty() {
                    "Remote Login was not turned on.".to_string()
                } else {
                    why
                });
            }
            Err(e) => return Err(format!("Could not ask for administrator rights: {}", e)),
        }
    }

    #[cfg(target_os = "windows")]
    {
        // One elevated step: start the service and, when this account is an
        // administrator, put the other computer's key where Windows reads it.
        let admin_file = admin_authorized_keys();
        let peer = std::fs::read_to_string(allow_key_path())
            .unwrap_or_default()
            .trim()
            .to_string();
        let peer_escaped = peer.replace('\'', "''");
        let marker = admin_key_marker();
        let script = format!(
            "$f = '{}'; if (Test-Path $f -PathType Leaf) {{ $t = Get-Content $f -Raw; if (-not $t.Contains('{p}')) {{ Add-Content $f '{p}' }} }} elseif ('{p}' -ne '') {{ Set-Content $f '{p}' }}; if (Test-Path $f) {{ icacls $f /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null }}; if ('{p}' -ne '') {{ Set-Content '{}' '{p}' }}; Set-Service sshd -StartupType Automatic; Start-Service sshd",
            admin_file.display(),
            marker.display(),
            p = peer_escaped
        );
        // write the script to a file so quoting cannot break it
        let script_path = ssh_dir().join("enable_sshd.ps1");
        let _ = std::fs::create_dir_all(ssh_dir());
        std::fs::write(&script_path, &script)
            .map_err(|e| format!("Cannot write setup script: {}", e))?;
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','{}'",
                    script_path.display()
                ),
            ])
            .output()
            .map_err(|e| format!("Could not ask for administrator rights: {}", e))?;
        if !out.status.success() {
            return Err("The administrator step was not completed.".to_string());
        }
        log::info!("[Terminal] OpenSSH Server start requested");
    }

    #[cfg(target_os = "linux")]
    {
        let out = Command::new("pkexec")
            .args(["systemctl", "enable", "--now", "sshd"])
            .output()
            .map_err(|e| format!("Could not ask for administrator rights: {}", e))?;
        if !out.status.success() {
            return Err("sshd was not started.".to_string());
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(900));
    let (service, message) = sshd_service();
    if service == "running" {
        Ok("The SSH server is running. The other computer can open a terminal here.".to_string())
    } else {
        Err(message)
    }
}

fn quote(s: &str) -> String {
    if s.contains(' ') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// Open a terminal window on this computer running a shell on `user@host`.
pub fn open_shell(host: &str, user: &str) -> Result<(), String> {
    let key = key_path();
    if !key.exists() {
        ensure_key()?;
    }
    let target = format!("{}@{}", user, host);
    let ssh_args: Vec<String> = vec![
        "-i".into(),
        key.to_string_lossy().to_string(),
        "-o".into(),
        "IdentitiesOnly=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "-t".into(),
        target.clone(),
    ];

    #[cfg(target_os = "macos")]
    {
        // A .command file opened with Terminal is the least fragile way to hand a
        // window a command: no AppleScript quoting, and the window stays open.
        let script = ssh_dir().join("open_shell.command");
        let body = format!(
            "#!/bin/sh\nexec {} {}\n",
            quote(ssh_bin()),
            ssh_args
                .iter()
                .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
                .collect::<Vec<_>>()
                .join(" ")
        );
        std::fs::write(&script, body).map_err(|e| format!("Cannot write command file: {}", e))?;
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700));
        }
        Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .arg(&script)
            .spawn()
            .map_err(|e| format!("Could not open Terminal: {}", e))?;
        log::info!("[Terminal] Opened a shell on {}", target);
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let mut args: Vec<String> = vec!["/C".into(), "start".into(), "".into(), ssh_bin().into()];
        args.extend(ssh_args.iter().cloned());
        Command::new("cmd")
            .args(&args)
            .spawn()
            .map_err(|e| format!("Could not open a terminal: {}", e))?;
        log::info!("[Terminal] Opened a shell on {}", target);
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let mut args: Vec<String> = vec!["-e".into(), ssh_bin().into()];
        args.extend(ssh_args.iter().cloned());
        Command::new("x-terminal-emulator")
            .args(&args)
            .spawn()
            .or_else(|_| Command::new("gnome-terminal").args(["--"]).args(&args).spawn())
            .map_err(|e| format!("Could not open a terminal: {}", e))?;
        log::info!("[Terminal] Opened a shell on {}", target);
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Terminals are not supported on this system.".to_string())
}

/// Keep `ssh glidedesk` working in any terminal, as the tab promises: a marked block
/// in the user's SSH config pointing at the paired computer and GlideDesk's key.
/// Only our own block is ever replaced, so the rest of the file is left alone.
pub fn write_ssh_alias(host: &str, user: &str) {
    const BEGIN: &str = "# GlideDesk: the paired computer, added by the Terminal tab";
    const END: &str = "# GlideDesk: end";
    if host.trim().is_empty() || user.trim().is_empty() {
        return;
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if home.is_empty() {
        return;
    }
    let path = PathBuf::from(home).join(".ssh").join("config");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let block = format!(
        "{}\nHost glidedesk\n    HostName {}\n    User {}\n    IdentityFile \"{}\"\n    IdentitiesOnly yes\n    StrictHostKeyChecking accept-new\n{}\n",
        BEGIN,
        host.trim(),
        user.trim(),
        key_path().display(),
        END
    );
    let next = match (existing.find(BEGIN), existing.find(END)) {
        (Some(a), Some(b)) if b > a => {
            let mut out = String::new();
            out.push_str(&existing[..a]);
            out.push_str(&block);
            out.push_str(&existing[b + END.len()..].trim_start_matches('\n'));
            out
        }
        _ => {
            let mut out = existing.clone();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&block);
            out
        }
    };
    if next == existing {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    if std::fs::write(&path, next).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        log::info!("[Terminal] Updated the `ssh glidedesk` alias for {}", host);
    }
}

/// Read this computer's SSH server state into the tab once, so the row is not blank
/// before the other computer has said anything.
fn refresh_local_service(app: &AppHandle, state: &AppState) {
    let (service, message) = sshd_service();
    {
        let mut s = state.inner.write();
        s.terminal.server.service = service;
        if s.terminal.server.message.is_empty() {
            s.terminal.server.message = message;
        }
    }
    push_state(app, state);
}

fn set_status(app: &AppHandle, state: &AppState, status: &str, user: &str, message: &str) -> bool {
    let mut changed = false;
    {
        let mut s = state.inner.write();
        if s.terminal.status != status {
            s.terminal.status = status.to_string();
            changed = true;
        }
        if s.terminal.user != user {
            s.terminal.user = user.to_string();
            changed = true;
        }
        if s.terminal.message != message {
            s.terminal.message = message.to_string();
            changed = true;
        }
    }
    if changed {
        push_state(app, state);
    }
    changed
}

/// Keep the tab honest: who is connected, whether our key is offered, and whether
/// the other computer said a shell could be opened there. Runs on its own so the
/// answer does not depend on which page is open.
pub fn start(app: AppHandle, network: NetworkManager) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        {
            let state = app.state::<AppState>();
            refresh_local_service(&app, &state);
        }
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;

            let state = app.state::<AppState>();
            let (enabled, allow, current_status) = {
                let s = state.inner.read();
                (
                    s.terminal.enabled,
                    s.terminal.allow_mac,
                    s.terminal.status.clone(),
                )
            };

            if !enabled {
                set_status(&app, &state, "off", "", "");
                continue;
            }

            let clients = network.get_active_clients_info();
            let peer = clients.iter().find(|c| {
                c.authenticated && c.peer_id.as_deref().map(|p| !p.is_empty()).unwrap_or(false)
            });

            let peer = match peer {
                Some(p) => p.clone(),
                None => {
                    set_status(&app, &state, "no_mac", "", "");
                    continue;
                }
            };

            // Offer our key so the other computer can let us in. It answers with its
            // own user name; until then the tab says it is waiting.
            if allow {
                if let (Ok(public), Some(pid)) = (ensure_key(), peer.peer_id.clone()) {
                    let pkt = crate::network::WireMessage {
                        t: "terminal.key".to_string(),
                        v: Some(1),
                        id: Some(network.local_id().to_string()),
                        name: Some(local_name(&state)),
                        user: Some(local_user()),
                        ssh: Some(public),
                        on: Some(true),
                        ..Default::default()
                    };
                    network.send_to(&pid, pkt).await;
                }
            }

            // Keep a ready or a refused answer: the other computer changes it at once
            // when it can, and overwriting a reason with "waiting" would hide it.
            if current_status == "ready" || current_status == "unavailable" {
                // `ssh glidedesk` is offered on the page, so keep it pointing at the
                // computer that answered.
                if current_status == "ready" {
                    let user = state.inner.read().terminal.user.clone();
                    if !user.trim().is_empty() {
                        write_ssh_alias(&peer.peer_ip, user.trim());
                    }
                }
                continue;
            }
            set_status(&app, &state, "waiting", "", "");
        }
    });
}

/// Ask the other computer for a shell, over the paired connection.
pub fn request_peer(state: &AppState, network: &NetworkManager, peer_id: &str) -> bool {
    let public = match ensure_key() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[Terminal] {}", e);
            return false;
        }
    };
    let pkt = crate::network::WireMessage {
        t: "terminal.key".to_string(),
        v: Some(1),
        id: Some(network.local_id().to_string()),
        name: Some(local_name(state)),
        user: Some(local_user()),
        ssh: Some(public),
        on: Some(true),
        ..Default::default()
    };
    let pid = peer_id.to_string();
    let net = network.clone();
    tauri::async_runtime::spawn(async move {
        net.send_to(&pid, pkt).await;
    });
    true
}

#[tauri::command]
pub fn open_terminal(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    network: tauri::State<'_, NetworkManager>,
) -> Result<String, String> {
    let (enabled, _status, user, message) = {
        let s = state.inner.read();
        (
            s.terminal.enabled,
            s.terminal.status.clone(),
            s.terminal.user.clone(),
            s.terminal.message.clone(),
        )
    };
    if !enabled {
        return Err("Terminal is off on this computer.".to_string());
    }

    let clients = network.get_active_clients_info();
    let peer = clients
        .iter()
        .find(|c| c.authenticated && c.peer_id.as_deref().map(|p| !p.is_empty()).unwrap_or(false))
        .cloned()
        .ok_or_else(|| "No computer is connected.".to_string())?;

    if user.trim().is_empty() {
        // The other computer has not answered yet; ask again rather than opening a
        // window that would only ask for a password.
        if let Some(pid) = peer.peer_id.clone() {
            request_peer(&state, &network, &pid);
        }
        return Err(if message.is_empty() {
            "Waiting for the other computer to let this one in.".to_string()
        } else {
            message
        });
    }

    open_shell(&peer.peer_ip, user.trim())?;
    let msg = format!("Opened a terminal on {} as {}.", peer.peer_id.unwrap_or_default(), user.trim());
    {
        let mut s = state.inner.write();
        s.terminal.message = msg.clone();
    }
    push_state(&app, &state);
    Ok(msg)
}

#[tauri::command]
pub fn setup_sshd(app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    {
        let mut s = state.inner.write();
        if s.terminal.server.busy {
            return Ok("Setting up the SSH server…".to_string());
        }
        s.terminal.server.busy = true;
        s.terminal.server.message = "Asking for administrator rights…".to_string();
    }
    push_state(&app, &state);

    tauri::async_runtime::spawn(async move {
        let job = tauri::async_runtime::spawn_blocking(enable_sshd).await;
        let (service, _) = sshd_service();
        let message: String;
        let enabled = match job {
            Ok(Ok(said)) => {
                message = said;
                service == "running"
            }
            Ok(Err(e)) => {
                message = e;
                false
            }
            Err(e) => {
                message = e.to_string();
                false
            }
        };
        let state = app.state::<AppState>();
        {
            let mut s = state.inner.write();
            s.terminal.server.busy = false;
            s.terminal.server.service = service;
            s.terminal.server.message = message;
            s.terminal.server.info.enabled = enabled;
        }
        push_state(&app, &state);
    });

    Ok("Setting up the SSH server…".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_key_lines_are_accepted() {
        assert!(valid_public_key(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB6v0hZ0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV glidedesk"
        ));
        assert!(valid_public_key("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQDfakeexamplekeymaterialonly"));
        assert!(!valid_public_key(""));
        assert!(!valid_public_key("ssh-ed25519 short"));
        assert!(!valid_public_key(
            "ssh-ed25519 AAAA\nBad AAAAC3NzaC1lZDI1NTE5AAAAIB6v0hZ0hV0hV0hV0hV0h"
        ));
        assert!(!valid_public_key(
            "command=\"rm -rf /\",no-pty ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB6v0hZ0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV"
        ));
        assert!(!valid_public_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB6v0hZ0hV0h $()"));
    }

    #[test]
    fn the_same_key_is_only_added_once() {
        let dir = std::env::temp_dir().join(format!("gd-term-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("authorized_keys");
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB6v0hZ0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV0hV glidedesk";
        assert!(append_line(&file, key).unwrap());
        assert!(!append_line(&file, key).unwrap());
        assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
