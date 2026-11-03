// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// An EventEmitter whose class has a getter named on, once, off or emit hides that method, and the
// first listener added to it throws: NowPlaying's `get on()` stopped the PC's phone setup there (no
// Remote, no Bluetooth sound). No class in the apps does it.
'use strict';

const assert = require('assert');
const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..');
const files = [];
for (const dir of ['shared', 'windows', 'mac', 'linux', 'windows/mesh', 'mac/mesh', 'linux/mesh']) {
  const d = path.join(ROOT, dir);
  if (!fs.existsSync(d)) continue;
  for (const f of fs.readdirSync(d)) if (f.endsWith('.js')) files.push(path.join(dir, f));
}

let emitters = 0;
for (const f of files) {
  const src = fs.readFileSync(path.join(ROOT, f), 'utf8');
  if (!/extends EventEmitter/.test(src)) continue;
  emitters++;
  const m = /^\s*(?:get|set)\s+(on|once|off|emit|addListener|removeListener)\s*\(/m.exec(src);
  assert.ok(!m, `${f}: a getter or setter named ${m && m[1]} hides EventEmitter's method`);
}

// And the two that did, listened to.
const { EventEmitter } = require('events');
const server = Object.assign(new EventEmitter(), { current: null, sendToClient: () => {} });
const { NowPlaying } = require('../nowplaying.js');
const { BtAudio } = require('../btaudio.js');
assert.strictEqual(typeof new NowPlaying({ server, cfg: () => ({}) }).on('change', () => {}).emit, 'function');
assert.strictEqual(typeof new BtAudio({ server, backend: { on: () => {} }, cfg: () => ({}), save: () => {} }).on('change', () => {}).emit, 'function');

console.log(`emitters: passed (${emitters} EventEmitter classes)`);
