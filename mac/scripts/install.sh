#!/usr/bin/env bash
# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# Headless-safe macOS installer & updater.
# Can be run over SSH, Terminal, or via curl:
#   curl -fsSL https://raw.githubusercontent.com/glidedesk/glidedesk/main/mac/scripts/install.sh | bash
set -euo pipefail

REPO="glidedesk/glidedesk"
ARCH="$(uname -m)"

echo "==> Detecting architecture: ${ARCH}"
if [ "${ARCH}" = "arm64" ]; then
  ASSET_MATCH="GlideDesk_.*_aarch64\.dmg"
else
  ASSET_MATCH="GlideDesk_.*_universal\.dmg"
fi

echo "==> Fetching latest GlideDesk release from GitHub..."
RELEASE_JSON="$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null || true)"
DOWNLOAD_URL="$(echo "${RELEASE_JSON}" | grep -o 'https://[^"]*' | grep -E "${ASSET_MATCH}" | head -n 1 || true)"

if [ -z "${DOWNLOAD_URL}" ]; then
  echo "Resolving download redirect for latest release..."
  if [ "${ARCH}" = "arm64" ]; then
    DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/GlideDesk_1.1.1_aarch64.dmg"
  else
    DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/GlideDesk_1.1.1_universal.dmg"
  fi
fi

echo "==> Downloading: ${DOWNLOAD_URL}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
DMG_PATH="${TMP_DIR}/GlideDesk.dmg"
MOUNT_POINT="${TMP_DIR}/mount"

curl -fSL --progress-bar "${DOWNLOAD_URL}" -o "${DMG_PATH}"

# Check if GlideDesk is currently running
WAS_RUNNING=0
if pgrep -ix "glidedesk" >/dev/null 2>&1; then
  WAS_RUNNING=1
  echo "==> GlideDesk is currently running. Preparing seamless hot-swap..."
fi

echo "==> Mounting disk image..."
mkdir -p "${MOUNT_POINT}"
hdiutil attach "${DMG_PATH}" -mountpoint "${MOUNT_POINT}" -nobrowse -quiet

if [ ! -d "${MOUNT_POINT}/GlideDesk.app" ]; then
  echo "Error: GlideDesk.app not found inside DMG." >&2
  hdiutil detach "${MOUNT_POINT}" -quiet || true
  exit 1
fi

# Stop running instance right before replacement to minimize downtime
if [ "${WAS_RUNNING}" -eq 1 ]; then
  echo "==> Stopping running instance..."
  pkill -ix "glidedesk" 2>/dev/null || true
  sleep 1
fi

echo "==> Installing to /Applications/GlideDesk.app..."
rm -rf "/Applications/GlideDesk.app"
cp -R "${MOUNT_POINT}/GlideDesk.app" "/Applications/GlideDesk.app"

echo "==> Unmounting disk image..."
hdiutil detach "${MOUNT_POINT}" -quiet || true

# Strip quarantine attribute to completely eliminate Gatekeeper "Move to Bin" warning
echo "==> Removing macOS quarantine flags (Gatekeeper approval)..."
xattr -cr "/Applications/GlideDesk.app" 2>/dev/null || true

# Reopen immediately if it was running or if user requested
if [ "${WAS_RUNNING}" -eq 1 ] || [ "${1:-}" = "--launch" ]; then
  echo "==> Relaunching GlideDesk (remote mouse/keyboard control will reconnect)..."
  open -a "/Applications/GlideDesk.app"
fi

echo "==> GlideDesk successfully installed!"
