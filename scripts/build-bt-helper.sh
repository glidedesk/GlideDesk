#!/usr/bin/env bash
# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
#
# Builds the small Bluetooth helper (bt/glidedesk-bt.swift) and places it where
# the app looks for it and where the macOS bundle carries it.
#
# The helper is what lets the app use Bluetooth from a command-line process:
# CoreBluetooth lives in the system frameworks, not behind a CLI, so this compiles
# one with the app's own permission text. Both architectures are built and joined
# into a universal binary, because the release build is universal.
#
# The built helper is kept in the repository (src-tauri/resources/glidedesk-bt),
# so every build — including the ones on machines without a Swift toolchain —
# ships the same binary. Run this script after changing bt/glidedesk-bt.swift and
# commit the result.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="$ROOT/src-tauri/bt/glidedesk-bt.swift"
OUT_DIR="${1:-$ROOT/src-tauri/resources}"
OUT="$OUT_DIR/glidedesk-bt"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "The Bluetooth helper is only built on macOS; nothing to do on $(uname -s)."
  exit 0
fi

if ! command -v swiftc >/dev/null 2>&1; then
  echo "swiftc is missing; install the Xcode command line tools to build the Bluetooth helper." >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# macOS 13 is the oldest the helper is built for: the command line tools carry
# the Swift runtime for both architectures from there, so one universal binary
# serves Intel and Apple silicon alike. Older Macs simply do not offer this, and
# the app says so instead of showing a switch that cannot work.
#
# The usage text is compiled in, so the system can ask the person once, in words
# that say what Bluetooth is used for.
PLIST="$TMP/Info.plist"
cat > "$PLIST" <<'PLIST_END'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>com.glidedesk.bt</string>
  <key>CFBundleName</key><string>GlideDesk Bluetooth</string>
  <key>CFBundleExecutable</key><string>glidedesk-bt</string>
  <key>NSBluetoothAlwaysUsageDescription</key>
  <string>GlideDesk uses Bluetooth to reach a nearby computer when the two are not on the same network.</string>
  <key>NSBluetoothPeripheralUsageDescription</key>
  <string>GlideDesk uses Bluetooth to offer a direct network to a nearby computer.</string>
</dict>
</plist>
PLIST_END

build() {
  local target="$1" name="$2"
  swiftc -O -swift-version 5 -target "$target" \
    -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker "$PLIST" \
    -o "$TMP/$name" "$SOURCE"
}

build "arm64-apple-macos13.0" "bt-arm64"
build "x86_64-apple-macos13.0" "bt-x86_64"
lipo -create "$TMP/bt-arm64" "$TMP/bt-x86_64" -output "$OUT"
chmod +x "$OUT"

echo "Bluetooth helper built: $OUT"
"$OUT" caps || echo "(the helper answered nothing; the app will say why when it is used)"
