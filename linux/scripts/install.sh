#!/usr/bin/env bash
# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# Universal Linux installer & updater (Debian, Ubuntu, Fedora, Arch, openSUSE, AppImage).
# Run via curl:
#   curl -fsSL https://raw.githubusercontent.com/glidedesk/GlideDesk/main/linux/scripts/install.sh | bash
set -euo pipefail

REPO="glidedesk/GlideDesk"
ARCH="$(uname -m)"

echo "==> Detecting architecture: ${ARCH}"
if [ "${ARCH}" != "x86_64" ] && [ "${ARCH}" != "amd64" ]; then
    echo "Warning: Only x86_64 / amd64 binaries are currently pre-built for Linux."
fi

echo "==> Fetching latest GlideDesk release..."
RELEASE_JSON="$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null || true)"
VERSION="$(echo "${RELEASE_JSON}" | grep -o '"tag_name": *"[^"]*"' | head -n 1 | cut -d'"' -f4 | sed 's/^v//' || true)"
if [ -z "${VERSION}" ]; then
    VERSION="1.1.1"
fi
echo "==> Latest version: ${VERSION}"

# Check running process
if pgrep -ix "glidedesk" >/dev/null 2>&1; then
    echo "==> Stopping running GlideDesk instances..."
    pkill -ix "glidedesk" 2>/dev/null || true
    sleep 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

# Determine best package format for current distro
DEB_URL=""
if command -v dpkg >/dev/null 2>&1; then
    DEB_URL="$(echo "${RELEASE_JSON}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*_amd64\.deb" | head -n 1 || true)"
    if [ -z "${DEB_URL}" ]; then
        DEB_URL="https://github.com/${REPO}/releases/latest/download/GlideDesk_${VERSION}_amd64.deb"
    fi
fi

APPIMAGE_URL="$(echo "${RELEASE_JSON}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*_amd64\.AppImage" | head -n 1 || true)"
if [ -z "${APPIMAGE_URL}" ]; then
    APPIMAGE_URL="https://github.com/${REPO}/releases/latest/download/GlideDesk_${VERSION}_amd64.AppImage"
fi

# 1. Debian / Ubuntu package
if [ -n "${DEB_URL:-}" ] && command -v dpkg >/dev/null 2>&1; then
    echo "==> Detected Debian/Ubuntu environment. Downloading .deb package..."
    curl -fSL "${DEB_URL}" -o "${TMP_DIR}/glidedesk.deb"
    if [ "$(id -u)" -eq 0 ]; then
        dpkg -i "${TMP_DIR}/glidedesk.deb" || apt-get install -f -y
    else
        sudo dpkg -i "${TMP_DIR}/glidedesk.deb" || sudo apt-get install -f -y
    fi
    echo "==> GlideDesk installed successfully via apt/dpkg!"
    exit 0
fi

# 2. RPM package (Fedora, RHEL, openSUSE)
if [ -n "${RPM_URL:-}" ] && command -v rpm >/dev/null 2>&1; then
    echo "==> Detected RPM-based environment. Downloading .rpm package..."
    curl -fSL "${RPM_URL}" -o "${TMP_DIR}/glidedesk.rpm"
    if [ "$(id -u)" -eq 0 ]; then
        rpm -Uvh --replacepkgs "${TMP_DIR}/glidedesk.rpm"
    else
        sudo rpm -Uvh --replacepkgs "${TMP_DIR}/glidedesk.rpm"
    fi
    echo "==> GlideDesk installed successfully via rpm!"
    exit 0
fi

# 3. Universal AppImage (Arch Linux, Manjaro, Alpine, Solus, or standalone)
if [ -z "${APPIMAGE_URL}" ]; then
    echo "Error: Could not resolve Linux AppImage asset." >&2
    exit 1
fi

echo "==> Installing Universal AppImage to ~/.local/bin/glidedesk..."
mkdir -p "${HOME}/.local/bin" "${HOME}/.local/share/applications"
TARGET="${HOME}/.local/bin/glidedesk"

curl -fSL "${APPIMAGE_URL}" -o "${TARGET}"
chmod +x "${TARGET}"

# Create Desktop Entry
cat <<EOF > "${HOME}/.local/share/applications/glidedesk.desktop"
[Desktop Entry]
Name=GlideDesk
Comment=Lightweight, Zero-Latency Software KVM
Exec=${TARGET}
Icon=glidedesk
Terminal=false
Type=Application
Categories=Utility;Network;
EOF

echo "==> GlideDesk ${VERSION} installed successfully to ${TARGET}!"
echo "==> Launch with: glidedesk"
