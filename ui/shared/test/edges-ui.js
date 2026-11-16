// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tests for shared/edges-ui.js: the arrangement grid on the Multi-device page.
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

ok('the grid puts this computer in the middle and a device on each side', () => {
  const edges = { left: 'desktop', right: 'laptop', top: null, bottom: null };
  const cells = E.cells(edges, { desktop: 'Desktop', laptop: 'Laptop' });
  assert.strictEqual(cells.length, 9, 'a 3×3 grid');
  assert.deepStrictEqual(cells.slice(0, 3).map((c) => c.kind), ['gap', 'side', 'gap']);
  assert.deepStrictEqual(cells.slice(3, 6).map((c) => c.kind), ['side', 'here', 'side']);
  assert.deepStrictEqual(cells.slice(6).map((c) => c.kind), ['gap', 'side', 'gap']);
  assert.strictEqual(cells[4].kind, 'here', 'the centre is this computer');
  assert.strictEqual(cells[3].side, 'left', 'left of the centre is the left edge');
  assert.strictEqual(cells[5].side, 'right');
  assert.strictEqual(cells[1].side, 'top');
  assert.strictEqual(cells[7].side, 'bottom');
  assert.strictEqual(cells[3].name, 'Desktop', 'the cell shows who is there');
  assert.strictEqual(cells[7].device, null, 'an empty side is empty');
  assert.strictEqual(cells[7].name, '');
});

ok('a device with no name on the desk still fills its cell', () => {
  const cells = E.cells({ top: 'zzz-unknown' });
  assert.strictEqual(cells[1].name, 'zzz-unknown', 'the id stands in rather than a blank cell');
  assert.strictEqual(cells[1].label, 'Top');
});

ok('a click puts the device on the side, or takes it off its own side', () => {
  const edges = { left: 'desktop', right: 'laptop', top: null, bottom: null };
  // A free side: the device goes there.
  assert.deepStrictEqual(E.attempt(edges, 'phone', 'bottom'), { call: { side: 'bottom', deviceId: 'phone' } });
  // Its own side: the same click clears it, so nothing is ever stuck on an edge.
  assert.deepStrictEqual(E.attempt(edges, 'laptop', 'right'), { call: { side: 'right', deviceId: null } });
  // Someone else's side: nothing is sent, and the page says who is in the way.
  assert.deepStrictEqual(E.attempt(edges, 'phone', 'left', { desktop: 'Desktop' }),
    { held: 'desktop', name: 'Desktop' });
  // Nonsense asks for nothing.
  assert.strictEqual(E.attempt(edges, 'phone', 'diagonal'), null);
  assert.strictEqual(E.attempt(edges, '', 'left'), null);
  assert.strictEqual(E.attempt(null, 'phone', 'left').call.deviceId, 'phone', 'no edges yet, nothing held');
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
  assert.strictEqual(E.describe({ left: 'vasanth' }, { vasanth: 'VASANTH' }), 'left: VASANTH',
    'the note names the device, not the id');
});

console.log('edges-ui: ' + checks + ' checks passed');
