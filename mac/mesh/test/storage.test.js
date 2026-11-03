// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// The mesh's own files on the Mac (mac/mesh/store.js; docs/multi-device.md, rule 4), in a temp folder:
// the one-to-one app's hosts.json and paired.json are read to start from and never written; a device
// forgotten in the mesh stays forgotten; the mesh's settings live in multi.json, never in settings.json.
// Run with: node mac/mesh/test/storage.test.js
'use strict';

const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { runTests } = require('../../../windows/mesh/test/lib/run.js');
const base = require('../../store.js');
const store = require('../store.js');

let dir = null;
function reset(init = {}) {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), 'twindesk-mesh-store-'));
  store.init(dir);
  for (const [k, v] of Object.entries(init)) fs.writeFileSync(path.join(dir, k), JSON.stringify(v));
}
const raw = (n) => (fs.existsSync(path.join(dir, n)) ? fs.readFileSync(path.join(dir, n), 'utf8') : undefined);
const host = (id, token) => ({ id, name: id, os: 'win', address: '10.0.0.2', port: 8443, fp: 'ab', token, lastSeen: 1 });
const guest = (id, tokenHash) => ({ id, name: id, os: 'win', tokenHash, pairedAt: 1, lastSeen: 1 });

const tests = [];
const test = (name, fn) => tests.push([name, fn]);

test('the one-to-one hosts join hosts-mesh.json once; hosts.json is never written', () => {
  reset({ 'hosts.json': [host('PC', 't1')] });
  const before = raw('hosts.json');
  assert.deepStrictEqual(store.read('hosts.json', []).map((e) => e.id), ['PC']);
  store.write('hosts.json', []); // forgotten in the mesh
  assert.strictEqual(raw('hosts.json'), before, 'the one-to-one list is as it was');
  assert.deepStrictEqual(store.read('hosts.json', []), [], 'stays forgotten');
  fs.writeFileSync(path.join(dir, 'hosts.json'), JSON.stringify([host('PC', 't2')])); // paired again one to one
  assert.deepStrictEqual(store.read('hosts.json', []).map((e) => e.token), ['t2'], 'a new pairing joins again');
});

test('the one-to-one guests join paired-mesh.json; paired.json is never written', () => {
  reset({ 'paired.json': [guest('PHONE', 'h1'), guest('PC', 'h2')] });
  const before = raw('paired.json');
  assert.deepStrictEqual(store.read('paired.json', []).map((e) => e.id), ['PHONE', 'PC']);
  store.write('paired.json', [guest('PHONE', 'h1')]);
  assert.strictEqual(raw('paired.json'), before);
  assert.deepStrictEqual(store.read('paired.json', []).map((e) => e.id), ['PHONE'], 'PC stays forgotten');
});

test('settings: layout and mesh go to multi.json; settings.json keeps the rest', () => {
  reset({ 'settings.json': { kvm: { pcSide: 'left', speed: 2 } }, 'multi.json': { on: true } });
  const s = store.loadSettings();
  assert.strictEqual(s.kvm.pcSide, 'left');
  assert.strictEqual(s.mesh.autoJoin, true, 'Join every device automatically is on by default');
  s.layout = { rev: 2, by: 'A', cells: { A: { x: 0, y: 0 } } };
  s.mesh.autoJoin = false; // switched off: kept
  store.saveSettings(s);
  const one = JSON.parse(raw('settings.json'));
  assert.strictEqual(one.layout, undefined, 'no layout in settings.json');
  assert.strictEqual(one.mesh, undefined, 'no mesh in settings.json');
  assert.strictEqual(one.kvm.pcSide, 'left');
  assert.strictEqual(one.kvm.speed, 2);
  const multi = JSON.parse(raw('multi.json'));
  assert.strictEqual(multi.on, true, 'the switch is kept');
  assert.strictEqual(multi.layout.rev, 2);
  assert.strictEqual(multi.mesh.autoJoin, false);
  const again = store.loadSettings();
  assert.strictEqual(again.layout.rev, 2);
  assert.strictEqual(again.mesh.autoJoin, false, 'the choice is kept over the default');
  assert.strictEqual(base.loadSettings().layout, undefined, 'the one-to-one app sees no layout');
});

test('settings: an off Join every device automatically saved before 2.2.0 (no choice) is on', () => {
  reset({ 'multi.json': { on: true, mesh: { autoJoin: false } } });
  assert.strictEqual(store.loadSettings().mesh.autoJoin, true, 'the old default, not a choice');
  reset({ 'multi.json': { on: true, mesh: { autoJoin: false, chosen: true } } });
  assert.strictEqual(store.loadSettings().mesh.autoJoin, false, 'switched off here');
});

runTests(tests);
