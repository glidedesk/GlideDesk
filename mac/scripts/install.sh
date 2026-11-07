#!/usr/bin/env bash
# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# 1-Line macOS Terminal Installer & Updater (Always installs the latest release)
# Usage: curl -fsSL https://raw.githubusercontent.com/glidedesk/GlideDesk/main/mac/scripts/install.sh | bash

set -euo pipefail

BOLD="\033[1m"
GREEN="\033[32m"
BLUE="\033[34m"
YELLOW="\033[33m"
RED="\033[31m"
RESET="\033[0m"

echo -e "${BLUE}${BOLD}=== GlideDesk macOS Auto-Installer ===${RESET}"

# 1. Check OS
if [[ "$(uname)" != "Darwin" ]]; then
    echo -e "${RED}Error: This script is intended for macOS only.${RESET}" >&2
    exit 1
fi

TMP_DIR=$(mktemp -d /tmp/glidedesk-install.XXXXXX)
cleanup() {
    hdiutil detach "${TMP_DIR}/mount" -quiet 2>/dev/null || true
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

echo -e "${BLUE}Resolving latest GlideDesk release...${RESET}"

# Attempt 1: Follow GitHub release redirect (rate-limit immune)
TAG=$(curl -sIL "https://github.com/glidedesk/GlideDesk/releases/latest" 2>/dev/null | grep -i "^location:" | head -n1 | tr -d '\r' | sed -E 's/.*tag\/(v[^[:space:]]+).*/\1/' || echo "")

# Attempt 2: Fallback to GitHub REST API if redirect header was empty
if [[ -z "$TAG" ]]; then
    LATEST_JSON=$(curl -fsSL "https://api.github.com/repos/glidedesk/GlideDesk/releases/latest" 2>/dev/null || echo "")
    TAG=$(echo "$LATEST_JSON" | grep -m1 '"tag_name":' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/' || echo "")
fi

# Attempt 3: Hardcoded safe fallback
if [[ -z "$TAG" ]]; then
    TAG="v3.2.0"
fi

VERSION="${TAG#v}"
echo -e "${GREEN}Latest release:${RESET} ${BOLD}${TAG}${RESET}"

DMG_URL="https://github.com/glidedesk/GlideDesk/releases/download/${TAG}/GlideDesk_${VERSION}_universal.dmg"
DMG_PATH="${TMP_DIR}/GlideDesk.dmg"

echo -e "${BLUE}Downloading Universal DMG (${VERSION})...${RESET}"
curl -fSL --progress-bar "$DMG_URL" -o "$DMG_PATH"

echo -e "${BLUE}Mounting DMG package...${RESET}"
MOUNT_DIR="${TMP_DIR}/mount"
mkdir -p "$MOUNT_DIR"
hdiutil attach "$DMG_PATH" -nobrowse -quiet -mountpoint "$MOUNT_DIR"

APP_SOURCE="${MOUNT_DIR}/GlideDesk.app"
APP_TARGET="/Applications/GlideDesk.app"

if [[ ! -d "$APP_SOURCE" ]]; then
    echo -e "${RED}Error: GlideDesk.app not found inside DMG.${RESET}" >&2
    exit 1
fi

# Close any running instances before overwriting
if pgrep -f "GlideDesk.app" >/dev/null 2>&1; then
    echo -e "${YELLOW}Closing running GlideDesk instance...${RESET}"
    pkill -f "GlideDesk.app" || true
    sleep 1
fi

echo -e "${BLUE}Installing to /Applications...${RESET}"
rm -rf "$APP_TARGET"
cp -R "$APP_SOURCE" "$APP_TARGET"

echo -e "${BLUE}Clearing macOS quarantine attributes...${RESET}"
xattr -cr "$APP_TARGET" 2>/dev/null || true

echo -e "${BLUE}Unmounting DMG...${RESET}"
hdiutil detach "$MOUNT_DIR" -quiet 2>/dev/null || true

echo ""
echo -e "${GREEN}${BOLD}✓ GlideDesk ${TAG} has been installed successfully!${RESET}"
echo -e "You can launch it from Applications or run:"
echo -e "  ${BOLD}open /Applications/GlideDesk.app${RESET}"
echo ""

# Launch if running in an interactive terminal and not CI
if [[ -t 1 ]] && [[ "${CI:-}" != "true" ]]; then
    open "$APP_TARGET"
fi
