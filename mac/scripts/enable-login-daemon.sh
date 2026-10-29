#!/usr/bin/env bash
# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# Enable GlideDesk pre-login LaunchDaemon on macOS
set -euo pipefail

if [ "$EUID" -ne 0 ]; then
  echo "Error: Please run as root (sudo)." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLIST_FILE="${SCRIPT_DIR}/../system/com.glidedesk.helper.plist"
TARGET="/Library/LaunchDaemons/com.glidedesk.helper.plist"

echo "==> Installing LaunchDaemon to ${TARGET}..."
cp "${PLIST_FILE}" "${TARGET}"
chown root:wheel "${TARGET}"
chmod 644 "${TARGET}"

echo "==> Loading LaunchDaemon into system launchd..."
launchctl unload "${TARGET}" 2>/dev/null || true
launchctl load -w "${TARGET}"

echo "==> GlideDesk pre-login LaunchDaemon successfully loaded!"
