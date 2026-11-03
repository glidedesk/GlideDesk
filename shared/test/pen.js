// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// A phone stylus's `pen` on in.mm/md/mu (shared/protocol.js P.penOf): what the helpers get.
// Run with: node shared/test/pen.js
'use strict';

const assert = require('assert');
const P = require('../protocol.js');

// None, or not an object: a mouse.
assert.strictEqual(P.penOf(undefined), null);
assert.strictEqual(P.penOf('pen'), null);

// Touching: pressure, tilt and twist as sent.
assert.deepStrictEqual(P.penOf({ p: 0.5, tx: 30, ty: -12, tw: 90, c: true }),
  { p: 0.5, tx: 30, ty: -12, tw: 90, c: true, er: false, bar: false, out: false });

// Out of range values are clamped; twist wraps.
assert.deepStrictEqual(P.penOf({ p: 3, tx: 200, ty: -200, tw: -90, c: true, er: true }),
  { p: 1, tx: 90, ty: -90, tw: 270, c: true, er: true, bar: false, out: false });

// Hovering has no pressure; junk is 0.
assert.deepStrictEqual(P.penOf({ p: 0.7, tx: 'x', tw: NaN, bar: true }),
  { p: 0, tx: 0, ty: 0, tw: 0, c: false, er: false, bar: true, out: false });

// Out of range: neither touching nor the side button.
assert.deepStrictEqual(P.penOf({ p: 0.7, c: true, bar: true, out: true }),
  { p: 0, tx: 0, ty: 0, tw: 0, c: false, er: false, bar: false, out: true });

console.log('pen ok');
