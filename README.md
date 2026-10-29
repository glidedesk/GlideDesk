<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="100" alt="GlideDesk icon">
</p>

<h1 align="center"><a href="https://github.com/glidedesk/glidedesk">GlideDesk</a></h1>

<p align="center">
  <b>Lightweight, Zero-Latency Software KVM (Tauri v2 + Rust)</b><br>
  Share one mouse, keyboard, and clipboard across Mac, Windows, Linux, and Android.<br>
  <b>15x Lower RAM. Zero-GC Latency. Instant Screen Transitions.</b>
</p>

<p align="center">
  <img alt="Release" src="https://img.shields.io/github/v/release/glidedesk/glidedesk?color=00c853">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.78%2B-dea584">
  <img alt="Tauri v2" src="https://img.shields.io/badge/Tauri-v2.0-24c8db">
  <img alt="macOS 13+" src="https://img.shields.io/badge/macOS-13%2B-black">
  <img alt="Windows 10/11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078d4">
  <img alt="Linux" src="https://img.shields.io/badge/Linux-X11%20%7C%20Wayland-f0a30a">
  <img alt="Android" src="https://img.shields.io/badge/Android-10%2B-3ddc84">
  <img alt="License: GPL-3.0" src="https://img.shields.io/badge/license-GPL--3.0-blue">
</p>

---

## ⚡ What is GlideDesk?

**GlideDesk** is a modern, high-performance software KVM switch built from the ground up on **Tauri v2 and asynchronous Rust**.

Move your mouse effortlessly past the edge of your monitor to seamlessly control your other Mac, Windows PC, or Linux workstation with zero physical cables, zero hardware switches, and zero lag.

---

### ⚔️ Comprehensive Feature & Architecture Comparison: GlideDesk vs. TwinDesk

GlideDesk was re-engineered from the ground up to solve the architectural bottlenecks, excessive RAM usage, and input collision bugs present in TwinDesk:

| Category | Capability / Attribute | TwinDesk (Electron) | GlideDesk (Tauri v2 + Rust) | Engineering Advantage |
| :--- | :--- | :--- | :--- | :--- |
| **Performance & Engine** | **Runtime Engine** | Bundled Chromium + Node.js (Electron) | **Native OS Webviews (WebKit/WebView2) + Tokio Async Rust** | 15–20x lower memory overhead |
| | **Idle RAM Footprint** | ~500 MB – 1.1 GB across 5+ processes | **~28 MB – 45 MB single process** | Frees RAM for developer IDEs, video editing, & games |
| | **Garbage Collection (GC)** | V8 Stop-The-World GC (100–300ms latency spikes) | **Zero GC** (Deterministic deallocation in Rust) | Zero cursor stutter or missed keystrokes at 144Hz+ |
| | **Cold Startup Time** | ~1.6 – 2.4 seconds | **<350 ms** | Instant launch |
| | **Headless Background Mode** | High (~400 MB background Chromium resident) | **~10 MB background daemon (`--daemon`)** | Invisible, silent 24/7 background service |
| **Input & Cursor Switching** | **OS Input Hooks** | Spawns separate helper processes (`mac-helper`) | **In-Process Kernel/Session Hooks (`CGEventTap`, `WH_MOUSE_LL`)** | Direct hardware events without IPC pipes |
| | **Mouse Motion Coalescing** | Raw streaming (can choke Wi-Fi networks) | **Delta Coalescing with Bounded Channels** | Smooth tracking under heavy network congestion |
| | **Edge Crossing Dimensions** | Left & Right only | **4-Way Crossing (Left, Right, Top, Bottom)** | Adapts to any multi-monitor physical desk layout |
| | **Virtual Desktop Bounds** | Often misaligns on mixed DPI monitors | **Dynamic Multi-Monitor Virtual Desktop Calculation** | Seamless crossing across different resolutions & scaling |
| **Touchpad & Conflict Resolution** | **Input State Machine** | Unlocked / No input arbitration | **Strict 3-State Machine (`home`, `driving`, `driven`)** | Prevents mouse & laptop touchpad fighting |
| | **Dual-Active Input Prevention** | ❌ **Bug:** Local touchpad & remote mouse fight | ✅ **Guaranteed:** Clean local takeover without desync | No cursor jumping when using both inputs |
| | **Touchpad Gesture Isolation** | ❌ **Bug:** 3-finger swipe down minimizes **both** PCs | ✅ **Isolated:** OS gestures stay strictly local | Laptop gestures won't accidentally minimize your remote Mac |
| | **Loopback Echo Suppression** | Prone to event bouncing | **Hardware-Level Event Tap Suppression** | Injected events never bounce back across network |
| **Window Management (Exclusive)** | **Single-Window Edge Handoff** | ❌ Not available (moves entire cursor only) | ✅ **Built-in (`Ctrl`/`Cmd` + Window Drag across edge)** | Move individual apps to another PC without physical cables |
| | **Offscreen Window Parking** | ❌ Not available | ✅ **Automated Offscreen Parking at `(-32000, -32000)`** | Keeps apps rendering at full speed without throttling |
| | **Floating Canvas Projection** | ❌ Not available | ✅ **Dedicated Target Canvas Window with Native Input** | Full mouse, click, and keyboard control of remote window |
| | **Crash Recovery Watchdog** | ❌ None (offscreen windows stay stranded) | ✅ **12s Disconnect Watchdog (Auto-restores all windows)** | Guaranteed zero lost windows if link drops or app exits |
| **Network & Mesh Topology** | **Network Architecture** | 1-to-1 direct peer link only | **N-Way Multi-Device Mesh Grid (Issue #64)** | Link 3+ computers and Android phones simultaneously |
| | **LAN Peer Discovery** | UDP broadcast on 47801 | **UDP broadcast on 47801 with auto-pruning & fallback** | Zero-config instant pairing |
| | **Port Conflict Resilience** | ❌ Aborts/crashes when port 47800/47801 is in use | ✅ **Graceful `EADDRINUSE` handling & UI notification** | Safely co-exists with other KVM tools without crashing |
| | **Connection Health** | Static ping interval | **Adaptive 1s active / 5s idle keepalive with backoff** | Low network overhead when idle |
| **Clipboard & Data Sync** | **Plain Text Sync** | Basic copy/paste | **Bidirectional Real-Time Sync (<10ms)** | Instant text sharing |
| | **Image Clipboard Sync** | Limited formats | **Full PNG / JPEG / Bitmap image clipboard transfer** | Seamless graphic design & screenshot pasting |
| | **Loopback Echo Suppression** | Often causes infinite clipboard loops | **Hash-checked deduplication registry** | Never re-pastes copied text in a feedback loop |
| **Android Mobile Companion** | **Wireless Second Screen** | Basic video streaming | **Hardware-Accelerated Display Mirroring** | Turns phone or tablet into extra desktop real estate |
| | **Trackpad & Gamepad Mode** | Limited touch zones | **Ultra-Low Latency Wi-Fi & Bluetooth BLE Gamepad** | Use phone as precision trackpad or PC controller |
| | **Phone Webcam for PC** | Requires external manual setup | **Direct Wireless HD Camera Streaming** | Use phone camera as Zoom/Teams webcam |
| | **Notification & SMS Mirroring**| Desktop popups | **Native System Notifications with Quick Reply** | Stay focused without picking up your phone |
| | **In-App Mobile Auto-Updater** | Manual APK sideloading | **Built-in Self-Updating Android Engine** | 1-tap updates directly inside the Android app |
| **Packaging & Distribution** | **Installer Download Size** | ~140 MB – 180 MB | **~7 MB – 16 MB** | ~10x faster downloads |
| | **Installed Disk Footprint** | ~380 MB | **~35 MB** | Saves SSD space |
| | **macOS Distribution** | Manual ZIP extraction | **Homebrew Cask (`brew install --cask`) & Universal DMG** | Native Mac package manager integration |
| | **Windows Distribution** | Standard installer | **NSIS Installer, Enterprise MSI & Zero-Install Portable** | IT administrator friendly |
| | **Linux Distribution** | Manual deb | **AppImage, Debian `.deb`, Fedora `.rpm`** | Works on any Linux distribution |

---

## 📥 Installation

### 🍎 macOS (Apple Silicon & Intel)

```sh
# Option 1: 1-line Terminal installer (bypasses Gatekeeper quarantine, headless-safe)
curl -fsSL https://raw.githubusercontent.com/glidedesk/glidedesk/main/mac/scripts/install.sh | bash

# Option 2: Homebrew Cask
brew install --cask glidedesk/glidedesk/glidedesk
```
*Manual download:*
- **Apple Silicon (M1/M2/M3/M4):** [`GlideDesk_1.1.1_aarch64.dmg`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_aarch64.dmg)
- **Intel Mac (Core i5/i7/i9):** [`GlideDesk_1.1.1_x64.dmg`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_x64.dmg)
- **Universal (All Macs):** [`GlideDesk_1.1.1_universal.dmg`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_universal.dmg)

### 🪟 Windows (10 / 11)

```powershell
# 1-line PowerShell silent installer / updater:
iex (irm https://raw.githubusercontent.com/glidedesk/glidedesk/main/windows/scripts/install.ps1)
```
- **Installer:** [`GlideDesk_1.1.1_x64-setup.exe`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_x64-setup.exe)
- **MSI Package:** [`GlideDesk_1.1.1_x64_en-US.msi`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_x64_en-US.msi)
- **Portable:** [`GlideDesk-portable-x64.exe`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk-portable-x64.exe)

### 🐧 Linux (Ubuntu, Debian, Fedora, Arch, openSUSE)

```sh
# 1-line Universal Linux installer / updater (auto-detects apt, rpm, or AppImage):
curl -fsSL https://raw.githubusercontent.com/glidedesk/glidedesk/main/linux/scripts/install.sh | bash
```
- **AppImage (Universal):** [`GlideDesk_1.1.1_amd64.AppImage`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_amd64.AppImage)
- **Debian / Ubuntu:** [`GlideDesk_1.1.1_amd64.deb`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_amd64.deb)
- **Fedora / RHEL / openSUSE:** [`GlideDesk_1.1.1_amd64.rpm`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_amd64.rpm)

### 📱 Android Companion

- **In-App Direct Update:** Tap **Update now** in the About tab or notification to update directly via Android's package installer.
- **Standard APK:** [`GlideDesk-1.1.1.apk`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk-1.1.1.apk)
- **Full APK (Notifications + Accessibility KVM):** [`GlideDesk-1.1.1-full.apk`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk-1.1.1-full.apk)

---

## ✨ Features

- **Single-Window Edge Handoff (TwinDesk Issue #18 Solved)**:
  - Drag any window's titlebar across your screen edge while holding `Ctrl` (Windows/Linux) or `Cmd` (macOS) to move just that application to your adjacent computer.
  - Source window is safely parked offscreen at `(-32000, -32000)` without render throttling or GPU slowdown.
  - Target machine displays a dedicated floating canvas with full bi-directional input forwarding (mouse, keyboard, scroll wheel).
- **Dual-Input & Touchpad Gesture Isolation (No Minimization Echoes)**:
  - Strict 3-state machine (`home`, `driving`, `driven`) resolves input collisions when an external mouse on one machine is used alongside a laptop touchpad on another.
  - Windows multi-finger touchpad gestures (like 3-finger swipe down / `Win+D`) stay local and are isolated from triggering destructive `Cmd+M` / `Cmd+H` minimize/hide actions on macOS.
- **Multi-Device Mesh Grid (TwinDesk Issue #64 Solved)**:
  - Connect 3+ Macs, Windows PCs, Linux workstations, and Android devices into an interconnected workspace grid.
- **Crash-Safe Self-Healing Watchdog**:
  - Continuous 12-second heartbeat watchdog monitors connection stability.
  - If a remote computer disconnects or loses network link while a window is parked, GlideDesk automatically unparks and restores all windows to your local monitor.
- **Seamless Monitor Edge-Crossing**:
  - Glide your cursor naturally across physical screens (Left, Right, Top, Bottom) with configurable border zones.
- **Zero-GC Determinism**:
  - High-performance Rust async engine with bounded channel streaming and motion delta coalescing for fluid 144Hz+ tracking.
- **Unified Real-Time Clipboard**:
  - Copy text or images on Mac, paste immediately on Windows or Linux, with built-in loopback echo suppression.
- **Instant LAN Discovery & Conflict Detection**:
  - Automatic peer discovery on UDP port `47801` without typing IP addresses, with intelligent detection and user notifications for busy ports.
- **Ultra-Light System Tray**:
  - Runs in the background using under 30 MB of RAM (versus 500 MB+ in Electron KVMs).

---

### 📱 Android Companion Features

GlideDesk includes a companion Android app (`android/`) for multi-device continuity:
- **Phone as Wireless Second Screen**: Extend your desktop onto an Android phone or tablet with smooth hardware-accelerated video streaming.
- **Touch Trackpad & Gamepad Mode**: Use your phone screen as an ultra-responsive wireless trackpad or gamepad for your PC over Wi-Fi or Bluetooth BLE.
- **Universal Clipboard Sync**: Copy text or pictures on your phone, paste immediately on your computer, and vice versa.
- **Notification & Call Mirroring**: View incoming phone alerts, texts, and caller IDs directly on your desktop screen.
- **Phone Camera as PC Webcam**: Turn your smartphone camera into a wireless HD webcam for desktop video calls.

---

## 🛠️ Building & Development

### Prerequisites
- [Rust toolchain](https://rustup.rs/) (stable)
- Node.js 18+
- On Linux: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev`
- On Windows: WebView2 (pre-installed on Windows 10/11)

### Quick Start
```bash
# Clone the repository
git clone https://github.com/glidedesk/glidedesk.git
cd glidedesk

# Install dependencies
npm install

# Run in development mode
npm run dev

# Run all test suites (Rust engine, JS protocol, Android)
npm test

# Fast tests (Rust backend + JS suites, skipping Android build)
npm run test:fast

# Build production package
npm run build
```

---

## 📜 License

GlideDesk is free and open-source software licensed under the [GNU General Public License v3.0](LICENSE).
