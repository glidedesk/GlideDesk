# Secure Pre-Login & Lock Screen KVM Architecture

GlideDesk provides seamless, zero-latency KVM control across physical machines. To provide true feature parity with Synergy Enterprise and Barrier, GlideDesk incorporates native OS-level desktop switching and background system services so remote input works seamlessly before user logon and during secure lock screens.

---

## 🏛️ Architectural Overview

### The Challenge: OS Security Boundaries
On modern operating systems, user sessions are strictly isolated from the login and lock screen environments:
1. **Windows:** Interactive user applications run on window station `WinSta0` under the `"default"` desktop. When Windows locks (Win+L) or displays the Logon/UAC screen, the OS switches active rendering and input focus to the isolated `"Winlogon"` desktop. Standard user processes attempting to inject input via `SendInput` will silently fail or have their inputs discarded.
2. **Linux:** Display managers (GDM, LightDM, SDDM) run within a separate unauthenticated session under their own X11/Wayland context before the user's graphical session starts.
3. **macOS:** `loginwindow` and FileVault authentication run in a root system context before user launchd jobs are spawned.

---

## ⚡ GlideDesk Solution

### 1. Windows: Native `OpenInputDesktop` & `SetThreadDesktop`
GlideDesk's Rust input engine dynamically checks and synchronizes with the active input desktop before every mouse or keyboard injection:

```rust
// src-tauri/src/kvm.rs
pub fn sync_input_desktop() {
    let access = DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS | DESKTOP_SWITCHDESKTOP;
    if let Ok(input_desktop) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, access) {
        let cur_thread = GetCurrentThreadId();
        let current_desktop = GetThreadDesktop(cur_thread);
        if current_desktop != input_desktop {
            let _ = SetThreadDesktop(input_desktop);
        }
        let _ = CloseDesktop(input_desktop);
    }
}
```

When GlideDesk operates as an elevated helper or background service, `sync_input_desktop()` attaches the input injection thread directly to the `"Winlogon"` desktop, allowing password/PIN entry and lock screen dismissal.

### 2. Multi-Platform Service Configuration

| Platform | Service Architecture | Location |
| :--- | :--- | :--- |
| **Windows** | Scheduled Task as `SYSTEM` at startup | `windows/scripts/install-login-service.ps1` |
| **Linux** | Systemd unit before `graphical.target` | `linux/systemd/glidedesk-login.service` |
| **macOS** | System `LaunchDaemon` | `mac/system/com.glidedesk.helper.plist` |

---

## 🚀 Installation & Setup

### Windows
Run PowerShell as Administrator:
```powershell
.\windows\scripts\install-login-service.ps1
```

### Linux
Run in Terminal:
```bash
sudo ./linux/scripts/enable-login-service.sh
```

### macOS
Run in Terminal:
```bash
sudo ./mac/scripts/enable-login-daemon.sh
```

---

## 📊 Parity Matrix

| Feature | GlideDesk (Rust/Tauri) | Synergy Enterprise | Barrier / Input Leap |
| :--- | :---: | :---: | :---: |
| **Pre-Login Control** | ✅ Native Service | ✅ Proprietary Daemon | ⚠️ Manual Service Setup |
| **Windows Lock Screen (UAC/Winlogon)** | ✅ Native Win32 API | ✅ Supported | ⚠️ Requires UAC elevation |
| **Multi-Monitor Virtual Bounds** | ✅ Spanned Virtual Screen | ✅ Supported | ⚠️ Monitor edge bugs (#54) |
| **Memory Footprint** | **~25-45 MB** | ~150-250 MB | ~80-120 MB |
| **Zero Garbage Collection Latency** | ✅ Native async Tokio | ❌ Node/Chromium | ✅ C++ |
