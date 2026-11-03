#!/bin/sh
# TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
# Builds bin/macshare-linuxhelper. Needs gcc and the X11 dev packages, and GLib for Bluetooth:
#   sudo apt install build-essential libx11-dev libxi-dev libxtst-dev libxrandr-dev libxfixes-dev libglib2.0-dev
# Without GLib it builds anyway, without Bluetooth (ble.c says so to every ble* command).
set -e
cd "$(dirname "$0")"
mkdir -p bin
BLE=""
if pkg-config --exists gio-2.0 2>/dev/null; then
  BLE="-DHAVE_BLE $(pkg-config --cflags --libs gio-2.0)"
else
  echo "gio-2.0 not found: building without Bluetooth (install libglib2.0-dev)" >&2
fi
# shellcheck disable=SC2086
${CC:-gcc} -O2 -Wall -Wextra -std=gnu11 src/*.c -o bin/macshare-linuxhelper -lX11 -lXi -lXtst -lXext -lXrandr -lXfixes -lm -pthread $BLE
# Multi-device's helper (docs/multi-device.md): the same sources with -DMULTI, several watched edges on
# X11 (the #ifdef MULTI parts of capture.c and main.c); only linux/mesh starts it.
# shellcheck disable=SC2086
${CC:-gcc} -O2 -Wall -Wextra -std=gnu11 -DMULTI src/*.c -o bin/macshare-linuxhelper-multi -lX11 -lXi -lXtst -lXext -lXrandr -lXfixes -lm -pthread $BLE
# Hosting Wi-Fi Direct beside the adapter's own network (linux/wifi.js; installed as root when it's first needed).
${CC:-gcc} -O2 -Wall -Wextra -std=gnu11 wifiap/wifiap.c -o bin/twindesk-wifiap
# Camera › Virtual camera's writer (linux/vcam.js): frames to the v4l2loopback device.
${CC:-gcc} -O2 -Wall -Wextra -std=gnu11 vcam/vcam.c -o bin/twindesk-vcam
# Remote › Real gamepad's writer (linux/gamepad.js): a uinput gamepad.
${CC:-gcc} -O2 -Wall -Wextra -std=gnu11 pad/pad.c -o bin/twindesk-pad
echo "built bin/macshare-linuxhelper bin/macshare-linuxhelper-multi bin/twindesk-wifiap bin/twindesk-vcam bin/twindesk-pad"
