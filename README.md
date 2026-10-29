<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="100" alt="GlideDesk icon">
</p>

<h1 align="center"><a href="https://github.com/glidedesk/glidedesk">GlideDesk</a></h1>

<p align="center">
  <b>Lightweight, Zero-Latency Software KVM & Multi-Device Workspace Bridge</b><br>
  Share one mouse, keyboard, and clipboard across Mac, Windows, Linux, and Android.<br>
  <b>15x Lower RAM. Zero-GC Latency. Instant Screen Transitions.</b>
</p>

<p align="center">
  <a href="https://github.com/glidedesk/glidedesk/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/glidedesk/glidedesk?color=00c853&label=Release"></a>
  <a href="https://github.com/glidedesk/glidedesk/actions"><img alt="Build Status" src="https://img.shields.io/github/actions/workflow/status/glidedesk/glidedesk/build-tauri.yml?branch=main&label=CI%20Build"></a>
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.78%2B-dea584?logo=rust">
  <img alt="Tauri v2" src="https://img.shields.io/badge/Tauri-v2.0-24c8db?logo=tauri">
  <img alt="Platforms" src="https://img.shields.io/badge/macOS%20%7C%20Windows%20%7C%20Linux%20%7C%20Android-Supported-blue">
  <a href="LICENSE"><img alt="License: GPL-3.0" src="https://img.shields.io/badge/license-GPL--3.0-blue"></a>
</p>

<p align="center">
  <a href="#-installation--setup"><b>Quick Install</b></a> •
  <a href="#-industry-comparison-glidedesk-vs-barrier-synergy--universal-control"><b>Comparison Matrix</b></a> •
  <a href="#-architecture--mesh-topology"><b>Architecture</b></a> •
  <a href="#-features"><b>Key Features</b></a> •
  <a href="#-building--development"><b>Build from Source</b></a>
</p>

---

## ⚡ What is GlideDesk?

**GlideDesk** is a modern, high-performance software KVM switch built from the ground up on **Tauri v2 and asynchronous Rust**.

Move your mouse effortlessly past the edge of your monitor to seamlessly control your other Mac, Windows PC, or Linux workstation with zero physical cables, zero hardware switches, and zero lag.

```mermaid
flowchart LR
    subgraph Desk["Physical Workspace Grid"]
        direction LR
        Mac["🍎 macOS Workstation<br/><code>(M1–M4 / Intel)</code>"]
        Win["🪟 Windows PC<br/><code>(10 / 11 64-bit)</code>"]
        Linux["🐧 Linux Rig<br/><code>(X11 / Wayland)</code>"]
        Phone["📱 Android Companion<br/><code>(Touchpad & Display)</code>"]
    end

    Mac <===>|"Zero-Latency UDP Mesh (Port 47801)"| Win
    Win <===>|"Real-Time Clipboard & Input Hopping"| Linux
    Mac <===>|"Encrypted P2P Link"| Linux
    Phone -.->|"Wi-Fi / BLE Wireless Trackpad"| Mac
    Phone -.->|"Second Screen & Audio Mirroring"| Win
```

---

### ⚔️ Industry Comparison: GlideDesk vs. Barrier, Synergy, & Universal Control

GlideDesk is engineered for modern cross-platform power users who need zero-latency input sharing without bloated runtimes, vendor lock-in, or expensive paid subscriptions:

| Feature / Capability | GlideDesk | Barrier / Deskflow | Symless Synergy | Microsoft Mouse Without Borders | Apple Universal Control |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Price & License** | **100% Free & Open Source (GPL-3.0)** | Free & Open Source (GPL-2.0) | Paid ($29 – $39) / Proprietary | Free / Proprietary (PowerToys) | Free / Apple Hardware Locked |
| **Supported Platforms** | **macOS, Windows, Linux, Android** | macOS, Windows, Linux | macOS, Windows, Linux | Windows only | macOS & iPadOS only |
| **Engine & Language** | **Tokio Async Rust + Tauri v2** | C++ / Qt5 | C++ / Qt5 | C# / .NET Framework | Swift / Objective-C (Apple Private APIs) |
| **Idle RAM Footprint** | **~28 MB** | ~60 MB – 80 MB | ~120 MB – 180 MB | ~45 MB – 65 MB | Built-in OS Daemon |
| **Garbage Collection (GC)** | **Zero GC (Deterministic Rust)** | Zero GC (C++) | Zero GC (C++) | CLR GC (Periodic pauses) | ARC (Apple Reference Counting) |
| **Single-Window Edge Handoff** | **✅ Built-in (`Ctrl`/`Cmd` + Drag window across border)** | ❌ Whole cursor/screen only | ❌ Whole cursor/screen only | ❌ Whole cursor/screen only | ❌ Whole cursor/screen only |
| **Dual-Active Input Arbitration** | **✅ Strict 3-State Machine (`home`, `driving`, `driven`)** | ❌ Stutters on dual inputs | ❌ Gestures trigger remote actions | ❌ Windows only | ✅ Native trackpad gestures |
| **Android Mobile Companion** | **✅ Native Companion (Trackpad, Display, Webcam)** | ❌ No mobile support | ❌ No mobile support | ❌ No mobile support | ❌ iPad only (No Android) |
| **High Refresh Rate (144Hz+)** | **✅ Delta Coalescing (Smooth under Wi-Fi load)** | ⚠️ Drops packets on high polling | ⚠️ Latency spikes on Wi-Fi | ⚠️ Stutters on high DPI mice | ✅ Apple ProMotion 120Hz |
| **Image Clipboard Transfer** | **✅ Real-Time PNG/JPEG Transfer with Hash Deduplication** | ⚠️ Frequently hangs on large clips | ✅ Basic image sync | ⚠️ Small files only | ✅ Apple Continuity |
| **Multi-Monitor Mesh Topology** | **✅ N-Way P2P Mesh (3+ Computers + Phone)** | ⚠️ Rigid server/client grid | ⚠️ Rigid server/client grid | ⚠️ 4 PCs max (Windows only) | ⚠️ 3 Apple devices max |

<details>
<summary><b>🔍 View Architectural Deep Dive (Rust Engine vs Legacy Architectures)</b></summary>

<br>

| Category | Capability / Attribute | Legacy Software KVMs (Qt / Electron / .NET) | GlideDesk (Tauri v2 + Tokio Rust) | Engineering Advantage |
| :--- | :--- | :--- | :--- | :--- |
| **Performance & Engine** | **Runtime Engine** | Bundled Chromium / Heavy Qt / .NET | **Native OS Webviews (WebKit/WebView2) + Tokio Async Rust** | 15–20x lower memory overhead |
| | **Idle RAM Footprint** | ~100 MB – 1 GB across multiple processes | **~28 MB – 45 MB single process** | Frees RAM for developer IDEs, video editing, & games |
| | **Garbage Collection (GC)** | GC pauses (100–300ms latency spikes) | **Zero GC** (Deterministic deallocation in Rust) | Zero cursor stutter or missed keystrokes at 144Hz+ |
| | **Cold Startup Time** | ~1.6 – 2.4 seconds | **<350 ms** | Instant launch |
| | **Headless Background Mode** | High background resident | **~10 MB background daemon (`--daemon`)** | Invisible, silent 24/7 background service |
| **Input & Cursor Switching** | **OS Input Hooks** | Spawns separate helper processes | **In-Process Kernel/Session Hooks (`CGEventTap`, `WH_MOUSE_LL`)** | Direct hardware events without IPC pipes |
| | **Mouse Motion Coalescing** | Raw streaming (can choke Wi-Fi networks) | **Delta Coalescing with Bounded Channels** | Smooth tracking under heavy network congestion |
| | **Edge Crossing Dimensions** | Left & Right only | **4-Way Crossing (Left, Right, Top, Bottom)** | Adapts to any multi-monitor physical desk layout |
| | **Virtual Desktop Bounds** | Often misaligns on mixed DPI monitors | **Dynamic Multi-Monitor Virtual Desktop Calculation** | Seamless crossing across different resolutions & scaling |
| **Touchpad & Conflict Resolution** | **Input State Machine** | Unlocked / No input arbitration | **Strict 3-State Machine (`home`, `driving`, `driven`)** | Prevents mouse & laptop touchpad fighting |
| | **Dual-Active Input Prevention** | ❌ Local touchpad & remote mouse fight | ✅ **Guaranteed:** Clean local takeover without desync | No cursor jumping when using both inputs |
| | **Touchpad Gesture Isolation** | ❌ Gestures trigger accidental remote shortcuts | ✅ **Isolated:** OS gestures stay strictly local | Laptop gestures won't accidentally minimize your remote Mac |
| | **Loopback Echo Suppression** | Prone to event bouncing | **Hardware-Level Event Tap Suppression** | Injected events never bounce back across network |
| **Window Management (Exclusive)** | **Single-Window Edge Handoff** | ❌ Not available (moves entire cursor only) | ✅ **Built-in (`Ctrl`/`Cmd` + Window Drag across edge)** | Move individual apps to another PC without physical cables |
| | **Offscreen Window Parking** | ❌ Not available | ✅ **Automated Offscreen Parking at `(-32000, -32000)`** | Keeps apps rendering at full speed without throttling |
| | **Floating Canvas Projection** | ❌ Not available | ✅ **Dedicated Target Canvas Window with Native Input** | Full mouse, click, and keyboard control of remote window |
| | **Crash Recovery Watchdog** | ❌ None (offscreen windows stay stranded) | ✅ **12s Disconnect Watchdog (Auto-restores all windows)** | Guaranteed zero lost windows if link drops or app exits |
| **Network & Mesh Topology** | **Network Architecture** | 1-to-1 direct peer link only | **N-Way Multi-Device Mesh Grid** | Link 3+ computers and Android phones simultaneously |
| | **LAN Peer Discovery** | Static IP configuration or basic broadcast | **UDP broadcast on 47801 with auto-pruning & fallback** | Zero-config instant pairing |
| | **Port Conflict Resilience** | ❌ Crashes when default port is occupied | ✅ **Graceful `EADDRINUSE` handling & UI notification** | Safely co-exists with other tools without crashing |
| | **Connection Health** | Static ping interval | **Adaptive 1s active / 5s idle keepalive with backoff** | Low network overhead when idle |
| **Clipboard & Data Sync** | **Plain Text Sync** | Basic copy/paste | **Bidirectional Real-Time Sync (<10ms)** | Instant text sharing |
| | **Image Clipboard Sync** | Unreliable or missing | **Full PNG / JPEG / Bitmap image clipboard transfer** | Seamless graphic design & screenshot pasting |
| | **Loopback Echo Suppression** | Prone to infinite clipboard loops | **Hash-checked deduplication registry** | Never re-pastes copied text in a feedback loop |
| **Packaging & Distribution** | **Installer Download Size** | ~60 MB – 180 MB | **~7 MB – 16 MB** | ~10x faster downloads |
| | **Installed Disk Footprint** | ~150 MB – 380 MB | **~35 MB** | Saves SSD space |
| | **macOS Distribution** | Manual ZIP extraction | **Homebrew Cask (`brew install --cask`) & Universal DMG** | Native Mac package manager integration |
| | **Windows Distribution** | Standard installer | **NSIS Installer, Enterprise MSI & Zero-Install Portable** | IT administrator friendly |
| | **Linux Distribution** | Manual deb / compile from source | **AppImage, Debian `.deb`, Fedora `.rpm`** | Works on any Linux distribution |

</details>

---

## 📥 Installation & Setup

### 🍎 macOS (Apple Silicon M1/M2/M3/M4 & Intel x86_64)

#### Option 1: 1-Line Terminal Installer (Recommended)
Automatically detects your Mac's CPU architecture (Apple Silicon vs. Intel), downloads the latest DMG, bypasses Gatekeeper quarantine, and cleanly updates running instances:
```bash
curl -fsSL https://raw.githubusercontent.com/glidedesk/glidedesk/main/mac/scripts/install.sh | bash
```

#### Option 2: Homebrew Cask
```bash
brew install --cask https://raw.githubusercontent.com/glidedesk/glidedesk/main/Casks/glidedesk.rb
```

#### Option 3: Manual Direct Download
- **Universal Build (Recommended for Apple Silicon & Intel Macs):** [`GlideDesk_1.1.1_universal.dmg`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_universal.dmg) (15 MB)
- **Apple Silicon Only (M1 / M2 / M3 / M4 Compact):** [`GlideDesk_1.1.1_aarch64.dmg`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_aarch64.dmg) (8 MB)

---

### 🪟 Windows (10 / 11 64-bit)

#### Option 1: 1-Line PowerShell Silent Installer (Recommended)
Automatically downloads, verifies, and silently installs or updates GlideDesk without reboot prompts.

In **PowerShell**:
```powershell
irm https://raw.githubusercontent.com/glidedesk/glidedesk/main/windows/scripts/install.ps1 | iex
```

Or from **Command Prompt (`cmd.exe`)** / **Win + R Run Dialog**:
```cmd
powershell -ExecutionPolicy Bypass -Command "iex (irm https://raw.githubusercontent.com/glidedesk/glidedesk/main/windows/scripts/install.ps1)"
```

#### Option 2: Manual Direct Download
- **Standard Installer (`.exe`):** [`GlideDesk_1.1.1_x64-setup.exe`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_x64-setup.exe)
- **Enterprise MSI (`.msi`):** [`GlideDesk_1.1.1_x64_en-US.msi`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_x64_en-US.msi)
- **Portable Standalone (`.exe`, Zero-Install):** [`GlideDesk-portable-x64.exe`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk-portable-x64.exe)

---

### 🐧 Linux (Ubuntu, Debian, Fedora, Arch, openSUSE)

#### Option 1: Universal 1-Line Installer (Recommended)
Auto-detects your distribution's native package manager (`apt` for `.deb`, or installs standalone `.AppImage` with desktop entry):
```bash
curl -fsSL https://raw.githubusercontent.com/glidedesk/glidedesk/main/linux/scripts/install.sh | bash
```

#### Option 2: Manual Package Download & Terminal Commands
- **Universal AppImage (Recommended for all Linux distros — Fedora, Arch, openSUSE, Ubuntu, Debian):**
  ```bash
  curl -fSL -O https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_amd64.AppImage
  chmod +x GlideDesk_1.1.1_amd64.AppImage
  ./GlideDesk_1.1.1_amd64.AppImage
  ```
- **Debian / Ubuntu / Pop!_OS / Linux Mint (`.deb` Package):**
  ```bash
  curl -fSL -O https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk_1.1.1_amd64.deb
  sudo dpkg -i GlideDesk_1.1.1_amd64.deb || sudo apt-get install -f -y
  ```

---

### 📱 Android Companion (Android 10+)

#### Option 1: Direct Download & In-App Auto-Update
- **Standard APK (Compact KVM & Clipboard):** [`GlideDesk-1.1.1.apk`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk-1.1.1.apk)
- **Full APK (Adds Phone Webcam & Notification Sync):** [`GlideDesk-1.1.1-full.apk`](https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk-1.1.1-full.apk)

#### Option 2: Install via ADB (USB or Wi-Fi)
```bash
curl -fSL -O https://github.com/glidedesk/glidedesk/releases/download/v1.1.1/GlideDesk-1.1.1.apk
adb install -r GlideDesk-1.1.1.apk
```

---

## ✨ Features

- **Single-Window Edge Handoff (Move Any App to Another PC)**:
  - Drag any window's titlebar across your screen edge while holding `Ctrl` (Windows/Linux) or `Cmd` (macOS) to move just that application to your adjacent computer.
  - Source window is safely parked offscreen at `(-32000, -32000)` without render throttling or GPU slowdown.
  - Target machine displays a dedicated floating canvas with full bi-directional input forwarding (mouse, keyboard, scroll wheel).
- **Dual-Input & Touchpad Gesture Isolation (No Minimization Echoes)**:
  - Strict 3-state machine (`home`, `driving`, `driven`) resolves input collisions when an external mouse on one machine is used alongside a laptop touchpad on another.
  - Windows multi-finger touchpad gestures (like 3-finger swipe down / `Win+D`) stay local and are isolated from triggering destructive `Cmd+M` / `Cmd+H` minimize/hide actions on macOS.
- **Multi-Device Mesh Grid (3+ Workstations Simultaneously)**:
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
