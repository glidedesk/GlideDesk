#!/usr/bin/env bash
# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# Enable GlideDesk pre-login systemd daemon on Linux
set -euo pipefail

if [ "$EUID" -ne 0 ]; then
  echo "Error: Please run as root (sudo)." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVICE_FILE="${SCRIPT_DIR}/../systemd/glidedesk-login.service"

echo "==> Installing GlideDesk pre-login systemd unit..."
cp "${SERVICE_FILE}" /etc/systemd/system/glidedesk-login.service
chmod 644 /etc/systemd/system/glidedesk-login.service

echo "==> Reloading systemd daemon and enabling service..."
systemctl daemon-reload
systemctl enable glidedesk-login.service
systemctl restart glidedesk-login.service

echo "==> Checking service status..."
systemctl status glidedesk-login.service --no-pager || true

echo "==> GlideDesk pre-login service successfully enabled!"
