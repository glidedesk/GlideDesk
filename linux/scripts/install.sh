#!/usr/bin/env bash
# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# 1-Line Linux Installer & Updater (Always installs the latest release)
# Usage: curl -fsSL https://raw.githubusercontent.com/glidedesk/GlideDesk/main/linux/scripts/install.sh | bash

set -euo pipefail

BOLD="\033[1m"
GREEN="\033[32m"
BLUE="\033[34m"
YELLOW="\033[33m"
RED="\033[31m"
RESET="\033[0m"

echo -e "${BLUE}${BOLD}=== GlideDesk Linux Auto-Installer ===${RESET}"

INSTALL_DIR="${HOME}/.local/bin"
DESKTOP_DIR="${HOME}/.local/share/applications"
mkdir -p "$INSTALL_DIR" "$DESKTOP_DIR"

echo -e "${BLUE}Resolving latest GlideDesk release...${RESET}"

# Attempt 1: Follow GitHub release redirect
TAG=$(curl -sIL "https://github.com/glidedesk/GlideDesk/releases/latest" 2>/dev/null | grep -i "^location:" | head -n1 | tr -d '\r' | sed -E 's/.*tag\/(v[^[:space:]]+).*/\1/' || echo "")

# Attempt 2: Fallback to GitHub REST API
if [[ -z "$TAG" ]]; then
    LATEST_JSON=$(curl -fsSL "https://api.github.com/repos/glidedesk/GlideDesk/releases/latest" 2>/dev/null || echo "")
    TAG=$(echo "$LATEST_JSON" | grep -m1 '"tag_name":' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/' || echo "")
fi

# Fallback
if [[ -z "$TAG" ]]; then
    TAG="v3.2.0"
fi

VERSION="${TAG#v}"
echo -e "${GREEN}Latest release:${RESET} ${BOLD}${TAG}${RESET}"

TARGET_BIN="${INSTALL_DIR}/GlideDesk.AppImage"
APPIMAGE_URL="https://github.com/glidedesk/GlideDesk/releases/download/${TAG}/GlideDesk_${VERSION}_amd64.AppImage"

echo -e "${BLUE}Downloading GlideDesk ${TAG} AppImage...${RESET}"
curl -fSL --progress-bar "$APPIMAGE_URL" -o "$TARGET_BIN"
chmod +x "$TARGET_BIN"

# Create or update standard desktop entry
cat <<EOF > "${DESKTOP_DIR}/glidedesk.desktop"
[Desktop Entry]
Name=GlideDesk
Comment=Lightweight, Zero-Latency Multi-Device KVM
Exec=${TARGET_BIN}
Terminal=false
Type=Application
Categories=Utility;Network;
StartupWMClass=glidedesk
EOF

echo ""
echo -e "${GREEN}${BOLD}✓ GlideDesk ${TAG} installed to ${TARGET_BIN}${RESET}"
echo -e "You can launch it from your desktop application launcher or run:"
echo -e "  ${BOLD}${TARGET_BIN}${RESET}"
echo ""
