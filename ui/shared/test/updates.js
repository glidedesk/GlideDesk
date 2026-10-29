// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// shared/updates.js pickAsset: each computer finds its own download among a release's files,
// with the Mac's disk images named GlideDesk-<version>-<arch>.dmg (mac/build-dmg.sh) and as before.
'use strict';

const assert = require('assert');
const { pickAsset } = require('../updates.js');

const files = (...names) => names.map((name) => ({ name, browser_download_url: 'https://example.invalid/' + name, size: 1 }));
const pick = (assets, platform, arch, prefer) => (pickAsset(assets, platform, arch, prefer) || {}).name || null;
const OTHERS = ['GlideDesk-2.3.0-amd64.deb', 'GlideDesk-2.3.0-x86_64.AppImage', 'GlideDesk-2.3.0.apk', 'GlideDesk-2.3.0-full.apk',
  'GlideDesk-Setup-2.3.0.exe', 'GlideDesk-Setup-2.3.0-arm64.exe'];

// Versioned, both Macs: each its own.
const both = files(...OTHERS, 'GlideDesk-2.3.0-arm64.dmg', 'GlideDesk-2.3.0-x64.dmg');
assert.strictEqual(pick(both, 'mac', 'arm64'), 'GlideDesk-2.3.0-arm64.dmg');
assert.strictEqual(pick(both, 'mac', 'x64'), 'GlideDesk-2.3.0-x64.dmg');
// The order on the release doesn't matter.
assert.strictEqual(pick(both.slice().reverse(), 'mac', 'arm64'), 'GlideDesk-2.3.0-arm64.dmg');
assert.strictEqual(pick(both.slice().reverse(), 'mac', 'x64'), 'GlideDesk-2.3.0-x64.dmg');
// Only Apple Silicon's on the release yet: an Intel Mac isn't given it (the release page opens instead).
const armOnly = files(...OTHERS, 'GlideDesk-2.3.0-arm64.dmg');
assert.strictEqual(pick(armOnly, 'mac', 'arm64'), 'GlideDesk-2.3.0-arm64.dmg');
assert.strictEqual(pick(armOnly, 'mac', 'x64'), null);
// The names up to 2.2.0 still work.
const old = files(...OTHERS, 'TwinDesk.dmg', 'Twindesk_intel.dmg');
assert.strictEqual(pick(old, 'mac', 'arm64'), 'TwinDesk.dmg');
assert.strictEqual(pick(old, 'mac', 'x64'), 'Twindesk_intel.dmg');
// The other platforms are unaffected by the Mac's names.
assert.strictEqual(pick(both, 'win', 'x64'), 'GlideDesk-Setup-2.3.0.exe');
assert.strictEqual(pick(both, 'win', 'arm64'), 'GlideDesk-Setup-2.3.0-arm64.exe');
assert.strictEqual(pick(both, 'linux', 'x64'), 'GlideDesk-2.3.0-x86_64.AppImage');
assert.strictEqual(pick(both, 'linux', 'x64', 'deb'), 'GlideDesk-2.3.0-amd64.deb');

console.log('updates: ok');
