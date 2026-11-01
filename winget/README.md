# WinGet Manifests for GlideDesk

This directory contains the [Windows Package Manager (WinGet)](https://learn.microsoft.com/en-us/windows/package-manager/) package manifests for GlideDesk (`GlideDesk.GlideDesk`).

---

## 🚀 How to Install via WinGet

Once published to the [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) community repository, users can install GlideDesk on any Windows 10/11 system with a single command:

```powershell
winget install GlideDesk.GlideDesk
```

Or by moniker:
```powershell
winget install glidedesk
```

---

## 🧪 Testing Locally

To test installing GlideDesk using the local manifest before or during PR review:

### Option A: Install from the Singleton Manifest
```powershell
winget install --manifest winget/GlideDesk.GlideDesk.singleton.yaml
```

### Option B: Validate Manifests with winget-cli
```powershell
winget validate winget/manifests/g/GlideDesk/GlideDesk/1.1.2/
```

---

## 📦 Submitting / Updating Releases on WinGet

### Method 1: Automated GitHub Action (Recommended)
Add a repository secret `WINGET_TOKEN` (a GitHub Personal Access Token with `public_repo` permissions) in your repo settings.
The `.github/workflows/winget.yml` workflow will automatically generate and open a PR against [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) whenever a new release is published.

### Method 2: Using `wingetcreate` CLI
On Windows:
```powershell
winget install wingetcreate
wingetcreate new https://github.com/glidedesk/GlideDesk/releases/download/v1.1.2/GlideDesk_1.1.2_x64-setup.exe
```
Follow the prompts and submit directly to `microsoft/winget-pkgs`.
