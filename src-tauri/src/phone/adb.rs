#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Native ADB Client: device enumeration, wireless pairing, port forwarding, and shell execution.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
    pub usb: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbMdnsService {
    pub name: String,
    pub service_type: String, // "pairing" | "connect"
    pub address: String,
    pub port: u16,
}

#[derive(Clone)]
pub struct AdbClient {
    bin_path: Option<PathBuf>,
    server_jar: Option<PathBuf>,
}

impl AdbClient {
    pub fn new() -> Self {
        Self {
            bin_path: Self::find_adb(),
            server_jar: Self::find_scrcpy_server(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.bin_path.is_some()
    }

    pub fn server_jar_path(&self) -> Option<&PathBuf> {
        self.server_jar.as_ref()
    }

    pub fn find_adb() -> Option<PathBuf> {
        // 1. Check ADB environment variable
        if let Ok(p) = std::env::var("ADB") {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }

        // 2. Check Android SDK paths
        for env_var in &["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
            if let Ok(val) = std::env::var(env_var) {
                let pb = PathBuf::from(val).join("platform-tools").join(if cfg!(windows) { "adb.exe" } else { "adb" });
                if pb.is_file() {
                    return Some(pb);
                }
            }
        }

        // 3. Check standard user library paths
        let home_opt = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok().map(PathBuf::from);
        if let Some(home) = home_opt {
            let pb = home.join("Library/Android/sdk/platform-tools").join(if cfg!(windows) { "adb.exe" } else { "adb" });
            if pb.is_file() {
                return Some(pb);
            }
        }

        // 4. Check Homebrew & system binary directories
        for candidate in &[
            "/opt/homebrew/bin/adb",
            "/usr/local/bin/adb",
            "/usr/bin/adb",
        ] {
            let pb = PathBuf::from(candidate);
            if pb.is_file() {
                return Some(pb);
            }
        }

        // 5. Query PATH via `which adb`
        if let Ok(out) = Command::new(if cfg!(windows) { "where" } else { "which" })
            .arg("adb")
            .output()
        {
            if out.status.success() {
                if let Ok(s) = std::str::from_utf8(&out.stdout) {
                    if let Some(first_line) = s.lines().next() {
                        let pb = PathBuf::from(first_line.trim());
                        if pb.is_file() {
                            return Some(pb);
                        }
                    }
                }
            }
        }

        None
    }

    pub fn find_scrcpy_server() -> Option<PathBuf> {
        let mut candidates = Vec::new();

        // 1. Current executable directory vendor / resources
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("scrcpy-server"));
                candidates.push(dir.join("resources/scrcpy-server"));
                candidates.push(dir.join("../Resources/scrcpy-server"));
                candidates.push(dir.join("vendor/scrcpy-server"));
            }
        }

        // 2. Relative to working directory
        candidates.push(PathBuf::from("src-tauri/vendor/scrcpy-server"));
        candidates.push(PathBuf::from("vendor/scrcpy-server"));
        candidates.push(PathBuf::from("/Volumes/External/bensonmac/Documents/BensonProject/GlideDesk/src-tauri/vendor/scrcpy-server"));
        candidates.push(PathBuf::from("/Volumes/External/bensonmac/Documents/BensonProject/TwinDesk/mac/vendor/scrcpy-server"));

        candidates.into_iter().find(|p| p.is_file())
    }

    fn run_adb(&self, args: &[&str]) -> Result<String, String> {
        let bin = self.bin_path.as_ref().ok_or_else(|| "adb binary not found. Please install Android Platform Tools.".to_string())?;

        let output = Command::new(bin)
            .args(args)
            .output()
            .map_err(|e| format!("Failed to execute adb {:?}: {}", args, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(format!("adb {:?} failed (exit code {:?}): {}{}", args, output.status.code(), stdout, stderr));
        }

        Ok(stdout)
    }

    pub fn devices(&self) -> Result<Vec<AdbDevice>, String> {
        let out = self.run_adb(&["devices", "-l"])?;
        Ok(Self::parse_devices(&out))
    }

    pub fn parse_devices(output: &str) -> Vec<AdbDevice> {
        let mut devices = Vec::new();
        for line in output.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let serial = parts[0].to_string();
                let state = parts[1].to_string();
                let mut model = None;

                for part in &parts[2..] {
                    if let Some(m) = part.strip_prefix("model:") {
                        model = Some(m.to_string());
                    }
                }

                let usb = !serial.contains(':');
                devices.push(AdbDevice {
                    serial,
                    state,
                    model,
                    usb,
                });
            }
        }
        devices
    }

    pub fn mdns_services(&self) -> Result<Vec<AdbMdnsService>, String> {
        let mut services = Vec::new();
        if let Ok(out) = self.run_adb(&["mdns", "services"]) {
            services.extend(Self::parse_mdns_services(&out));
        }

        // On macOS: Query Bonjour dns-sd for wireless debugging
        #[cfg(target_os = "macos")]
        {
            if let Ok(bonjour_services) = Self::query_bonjour_mdns() {
                for s in bonjour_services {
                    if !services.iter().any(|existing| existing.address == s.address && existing.port == s.port) {
                        services.push(s);
                    }
                }
            }
        }

        Ok(services)
    }

    pub fn parse_mdns_services(output: &str) -> Vec<AdbMdnsService> {
        let mut services = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.trim().split_whitespace().collect();
            if parts.len() >= 3 {
                let name = parts[0].to_string();
                let service_raw = parts[1];
                let host_port = parts[2];

                let service_type = if service_raw.contains("pairing") {
                    "pairing"
                } else if service_raw.contains("connect") {
                    "connect"
                } else {
                    continue;
                }.to_string();

                if let Some((addr, port_str)) = host_port.split_once(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        services.push(AdbMdnsService {
                            name,
                            service_type,
                            address: addr.to_string(),
                            port,
                        });
                    }
                }
            }
        }
        services
    }

    #[cfg(target_os = "macos")]
    fn query_bonjour_mdns() -> Result<Vec<AdbMdnsService>, String> {
        // Query local Bonjour services for _adb-tls-connect._tcp
        let out = Command::new("/usr/bin/dns-sd")
            .args(["-B", "_adb-tls-connect._tcp", "local."])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        if let Ok(mut child) = out {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let _ = child.kill();
        }
        Ok(vec![])
    }

    pub fn pair(&self, address: &str, port: u16, code: &str) -> Result<String, String> {
        let target = format!("{}:{}", address, port);
        self.run_adb(&["pair", &target, code])
    }

    pub fn connect(&self, address: &str, port: u16) -> Result<String, String> {
        let target = format!("{}:{}", address, port);
        self.run_adb(&["connect", &target])
    }

    pub fn disconnect(&self, serial: &str) -> Result<(), String> {
        let _ = self.run_adb(&["disconnect", serial]);
        Ok(())
    }

    pub fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<(), String> {
        let local_str = local.to_str().ok_or_else(|| "Invalid local path".to_string())?;
        self.run_adb(&["-s", serial, "push", local_str, remote])?;
        Ok(())
    }

    pub fn forward(&self, serial: &str, local_abstract: &str) -> Result<u16, String> {
        let abstract_arg = format!("localabstract:{}", local_abstract);
        let out = self.run_adb(&["-s", serial, "forward", "tcp:0", &abstract_arg])?;
        let port_str = out.trim();
        port_str.parse::<u16>().map_err(|e| format!("Failed to parse adb forward port '{}': {}", port_str, e))
    }

    pub fn unforward(&self, serial: &str, port: u16) -> Result<(), String> {
        let tcp_arg = format!("tcp:{}", port);
        let _ = self.run_adb(&["-s", serial, "forward", "--remove", &tcp_arg]);
        Ok(())
    }

    pub fn shell(&self, serial: &str, cmd: &str) -> Result<String, String> {
        self.run_adb(&["-s", serial, "shell", cmd])
    }

    pub fn spawn_shell(&self, serial: &str, cmd: &str) -> Result<Child, String> {
        let bin = self.bin_path.as_ref().ok_or_else(|| "adb binary not found".to_string())?;
        Command::new(bin)
            .args(["-s", serial, "shell", cmd])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn adb shell: {}", e))
    }
}
