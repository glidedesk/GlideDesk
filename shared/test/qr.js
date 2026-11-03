// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// shared/qr.js: the pairing QR codes. Versions grow with the text, and the fixed patterns (finders,
// timing) are where a reader looks for them. (Decoded with OpenCV while it was written: v1, 4, 7, 10.)
'use strict';

const assert = require('assert');
const Q = require('../qr.js');

const pair = 'twindesk://pair?id=a8fcde0b-e03b-4085-a469-76b3100db0cc&n=Inuka&a=192.168.1.20,10.0.0.5&p=47800&k=0123456789abcdef';
const adb = 'WIFI:T:ADB;S:twindesk-AbCdEf12;P:kL9xQ2mN7p;;';
const sizes = [['hello', 1], [adb, 4], [pair, 7]].map(([t, v]) => {
  const q = Q.encode(t);
  assert.strictEqual(q.version, v, t);
  assert.strictEqual(q.size, v * 4 + 17);
  return q;
});
for (const q of sizes) {
  // A finder in three corners: a dark ring, a light ring, a dark 3×3 centre.
  for (const [x, y] of [[0, 0], [q.size - 7, 0], [0, q.size - 7]]) {
    assert.ok(q.get(x, y) && q.get(x + 6, y + 6) && q.get(x + 3, y + 3), 'finder corners and centre');
    assert.ok(!q.get(x + 1, y + 1) && !q.get(x + 5, y + 5), 'finder light ring');
  }
  // Timing: alternating along row and column 6.
  for (let i = 8; i < q.size - 8; i++) assert.strictEqual(q.get(i, 6), i % 2 === 0);
}
assert.throws(() => Q.encode('x'.repeat(2000)), /Too long/);
console.log('qr: ok');
