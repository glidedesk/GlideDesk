// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// The mesh's own files (windows/mesh/store.js, windows/mesh/settings.js; docs/multi-device.md, rule 4):
// the one-to-one app's pairing files are read to start from and never written or removed; a device
// forgotten in the mesh stays forgotten; the mesh's settings live in multi.json, never in settings.json.
// Run with: node windows/mesh/test/storage.test.js  (NODE_PATH as in CI)
'use strict';

const assert = require('assert');
const { runTests } = require('./lib/run.js');

// ../store.js (the user data folder, through Electron) as files in memory.
const files = new Map();
const writes = [];
const storePath = require.resolve('../../store.js');
require.cache[storePath] = {
  id: storePath, filename: storePath, loaded: true,
  exports: {
    read: (n, fb) => (files.has(n) ? JSON.parse(files.get(n)) : fb),
    write: (n, o) => { writes.push(n); files.set(n, JSON.stringify(o)); },
    remove: (n) => { writes.push('-' + n); files.delete(n); },
  },
};
const base = require.cache[storePath].exports;
const meshStore = require('../store.js');
const M = require('../settings.js');

const reset = (init = {}) => {
  files.clear();
  writes.length = 0;
  for (const [k, v] of Object.entries(init)) files.set(k, JSON.stringify(v));
};
const read = (n) => (files.has(n) ? JSON.parse(files.get(n)) : undefined);
const host = (macId, token) => ({ macId, macName: macId, token, fp: 'ab', address: '10.0.0.2', port: 8443 });
const guest = (id, tokenHash) => ({ id, name: id, os: 'win', tokenHash, pairedAt: 1, lastSeen: 1 });

const tests = [];
const test = (name, fn) => tests.push([name, fn]);

test('the one-to-one host joins pairings.json once; pairing.json is never written or removed', () => {
  reset({ 'pairing.json': host('MAC', 't1') });
  assert.deepStrictEqual(meshStore.read('pairings.json', null).map((e) => e.macId), ['MAC']);
  meshStore.remove('pairing.json');
  assert.ok(files.has('pairing.json'), 'remove of the one-to-one file does nothing');
  meshStore.write('pairings.json', []); // forgotten in the mesh
  assert.deepStrictEqual(meshStore.read('pairings.json', null), [], 'stays forgotten');
  files.set('pairing.json', JSON.stringify(host('MAC', 't2'))); // paired again in the one-to-one app
  assert.deepStrictEqual(meshStore.read('pairings.json', null).map((e) => e.token), ['t2'], 'a new pairing joins again');
  assert.ok(!writes.includes('pairing.json') && !writes.includes('-pairing.json'), 'pairing.json untouched');
});

test('no pairing anywhere: the caller gets its fallback', () => {
  reset();
  assert.strictEqual(meshStore.read('pairings.json', null), null);
  assert.strictEqual(meshStore.read('paired.json', 'none'), 'none');
});

test('the one-to-one guests join paired-mesh.json; writes of paired.json go there', () => {
  reset({ 'paired.json': [guest('PHONE', 'h1'), guest('PC', 'h2')] });
  assert.deepStrictEqual(meshStore.read('paired.json', []).map((e) => e.id), ['PHONE', 'PC']);
  meshStore.write('paired.json', [guest('PHONE', 'h1')]); // PC forgotten in the mesh
  assert.deepStrictEqual(read('paired.json').map((e) => e.id), ['PHONE', 'PC'], 'the one-to-one list is as it was');
  assert.deepStrictEqual(meshStore.read('paired.json', []).map((e) => e.id), ['PHONE'], 'PC stays forgotten');
  meshStore.remove('paired.json');
  assert.ok(files.has('paired.json'));
  assert.ok(!writes.includes('paired.json'), 'paired.json never written');
});

test('settings: the mesh keys go to multi.json, settings.json keeps the one-to-one keys and pcSide', () => {
  reset({ 'settings.json': { kvm: { pcSide: 'left', speed: 1.5 } }, 'multi.json': { on: true } });
  const s = M.load(base);
  assert.strictEqual(s.kvm.pcSide, 'left');
  assert.strictEqual(s.mesh.autoJoin, true, 'Join every device automatically is on by default');
  s.kvm.edges = { left: null, right: 'B', top: null, bottom: null };
  s.layout = { rev: 1, by: 'A', cells: { A: { x: 0, y: 0 }, B: { x: 1, y: 0 } } };
  assert.strictEqual(M.apply(s, 'mesh', { autoJoin: false }), true);
  assert.deepStrictEqual(s.kvm.edges.right, 'B', 'apply keeps the edges');
  assert.ok(s.layout && s.layout.rev === 1, 'apply keeps the layout');
  M.save(base, s);
  const one = read('settings.json');
  assert.strictEqual(one.kvm.edges, undefined, 'no edges in settings.json');
  assert.strictEqual(one.layout, undefined);
  assert.strictEqual(one.mesh, undefined);
  assert.strictEqual(one.kvm.pcSide, 'left', 'pcSide stays for the one-to-one app');
  assert.strictEqual(one.kvm.speed, 1.5);
  const multi = read('multi.json');
  assert.strictEqual(multi.on, true, 'the switch is kept');
  assert.strictEqual(multi.kvm.edges.right, 'B');
  assert.strictEqual(multi.mesh.autoJoin, false);
  const again = M.load(base);
  assert.strictEqual(again.kvm.edges.right, 'B');
  assert.strictEqual(again.layout.rev, 1);
  assert.strictEqual(again.mesh.autoJoin, false, 'the choice is kept over the default');
});

test('settings: an off Join every device automatically saved before 2.2.0 (no choice) is on', () => {
  reset({ 'settings.json': {}, 'multi.json': { on: true, mesh: { autoJoin: false } } });
  assert.strictEqual(M.load(base).mesh.autoJoin, true, 'the old default, not a choice');
  reset({ 'settings.json': {}, 'multi.json': { on: true, mesh: { autoJoin: false, chosen: true } } });
  assert.strictEqual(M.load(base).mesh.autoJoin, false, 'switched off here');
});

runTests(tests);
