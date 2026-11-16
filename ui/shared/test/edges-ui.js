// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tests for shared/edges-ui.js: the edge picker on the Multi-device page.
// Run with: node ui/shared/test/edges-ui.js
'use strict';

const assert = require('assert');
const E = require('../edges-ui.js');

let checks = 0;
function ok(name, fn) {
  fn();
  checks++;
  console.log('  ✔ ' + name);
}

const states = []; // reserved for future page-level checks
void states;

// The same rules as shared/kvm-edges.js and src-tauri/src/mesh/edges.rs.
ok('a missing or broken edge map reads as no sides', () => {
  const none = { left: null, right: null, top: null, bottom: null };
  assert.deepStrictEqual(E.normalize(undefined), none);
  assert.deepStrictEqual(E.normalize('left'), none);
  assert.deepStrictEqual(E.normalize({ left: 5, middle: 'x' }), none);
  assert.deepStrictEqual(E.normalize({ top: 'B', left: 'B' }), { ...none, left: 'B' },
    'one device keeps its first side in left, right, top, bottom order');
  assert.deepStrictEqual(E.normalize({ top: 'x'.repeat(65) }), none, 'over-long ids are not devices');
  assert.deepStrictEqual(E.normalize({ top: '  ' }), none, 'an empty id is not a device');
});

ok('a device is on exactly one side', () => {
  const edges = { left: null, right: 'laptop', top: 'desktop', bottom: null };
  assert.strictEqual(E.sideOf(edges, 'laptop'), 'right');
  assert.strictEqual(E.sideOf(edges, 'desktop'), 'top');
  assert.strictEqual(E.sideOf(edges, 'LAPTOP'), 'right', 'ids compare case-insensitively');
  assert.strictEqual(E.sideOf(edges, 'phone'), null);
  assert.strictEqual(E.sideOf(edges, ''), null);
  assert.strictEqual(E.deviceOn(edges, 'right'), 'laptop');
  assert.strictEqual(E.deviceOn(edges, 'bottom'), null);
});

ok('the picker offers every side, marks the current one, and refuses a taken one', () => {
  const edges = { left: 'desktop', right: null, top: null, bottom: null };
  const opts = E.options(edges, 'laptop', { desktop: 'Desktop' });
  assert.strictEqual(opts.length, 5, 'no side plus the four sides');
  assert.deepStrictEqual(opts.map((o) => o.value), ['', 'left', 'right', 'top', 'bottom']);
  const current = opts.filter((o) => o.selected);
  assert.strictEqual(current.length, 1);
  assert.strictEqual(current[0].value, '', 'the laptop has no side yet');
  const left = opts.find((o) => o.value === 'left');
  assert.strictEqual(left.disabled, true, 'left already leads to the desktop');
  assert.strictEqual(left.holder, 'Desktop', 'and the picker says who is there');
  assert.strictEqual(opts.find((o) => o.value === 'right').disabled, false);
});

ok('the current side is selected, and a side leading to this same device is not a conflict', () => {
  const edges = { left: null, right: 'laptop', top: null, bottom: null };
  const opts = E.options(edges, 'laptop');
  assert.strictEqual(opts.find((o) => o.value === 'right').selected, true);
  assert.strictEqual(opts.find((o) => o.value === 'right').disabled, false,
    'the side it is already on is its own');
});

ok('a choice becomes the command the app takes', () => {
  const edges = { left: null, right: 'laptop', top: null, bottom: null };
  assert.deepStrictEqual(E.change(edges, 'laptop', 'top'), { side: 'top', deviceId: 'laptop' });
  // Clearing names the side it is on now: the command clears a side.
  assert.deepStrictEqual(E.change(edges, 'laptop', ''), { side: 'right', deviceId: null });
  assert.strictEqual(E.change(edges, 'desktop', ''), null, 'nothing to clear: no call at all');
  assert.strictEqual(E.change(edges, 'laptop', 'diagonal'), null);
  assert.strictEqual(E.change(edges, '', 'top'), null);
});

ok('the note under the list reads as a sentence', () => {
  assert.strictEqual(E.describe({}), 'No sides assigned yet');
  assert.strictEqual(E.describe(null), 'No sides assigned yet');
  assert.strictEqual(E.describe({ left: 'desktop', bottom: 'phone' }), 'left: desktop, bottom: phone');
});

console.log('edges-ui: ' + checks + ' checks passed');
