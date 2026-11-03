// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Tests for shared/pairlimit.js: pairing time limits and their reasons.
// Run with: node shared/test/pairlimit.js
'use strict';

const assert = require('assert');
const P = require('../protocol.js');
const L = require('../pairlimit.js');

// Reasons name the computer and, for connecting, the address (or Bluetooth).
assert.ok(L.limitMessage('connecting', 'Desk PC', '192.168.1.9').includes('Desk PC at 192.168.1.9'));
assert.ok(L.limitMessage('connecting', 'Desk PC', 'ble:ab').includes('over Bluetooth'));
assert.ok(L.limitMessage('waiting', 'Desk PC', '1.2.3.4').startsWith('Desk PC didn’t answer'));

// Version mismatch: which side to update, from the host's "speaks protocol vN".
assert.strictEqual(L.versionMessage({ message: `speaks protocol v${P.PROTOCOL_VERSION + 1}` }, 'Mac'), 'This TwinDesk is older than the one on Mac. Update it here.');
if (P.PROTOCOL_VERSION > 1) assert.strictEqual(L.versionMessage({ message: `speaks protocol v${P.PROTOCOL_VERSION - 1}` }, 'Mac'), 'Mac runs an older TwinDesk. Update it there.');
assert.strictEqual(L.versionMessage({ message: 'no version here' }, 'Mac'), null);
assert.strictEqual(L.versionMessage(null, 'Mac'), null);

// PairLimit: fires for the phase it was armed for; a phase change or a new pairing re-arms it.
(async () => {
  const real = global.setTimeout;
  const timers = [];
  global.setTimeout = (fn, ms) => { timers.push({ fn, ms }); return timers.length; };
  global.clearTimeout = () => {};
  const fired = [];
  const lim = new L.PairLimit((m) => fired.push(m));
  const ps = { phase: 'connecting' };
  lim.update(ps, 'PC', '10.0.0.2');
  assert.strictEqual(timers[0].ms, L.LIMIT_MS.connecting);
  lim.update(ps, 'PC', '10.0.0.2'); // same phase: not re-armed
  assert.strictEqual(timers.length, 1);
  ps.phase = 'starting';
  lim.update(ps, 'PC', '10.0.0.2');
  assert.strictEqual(timers[1].ms, L.LIMIT_MS.starting);
  timers[0].fn(); // the old connecting timer: stale, nothing happens
  assert.deepStrictEqual(fired, []);
  timers[1].fn();
  assert.strictEqual(fired.length, 1);
  ps.phase = 'code'; // reading the code: no limit
  lim.update(ps, 'PC', '10.0.0.2');
  assert.strictEqual(timers.length, 2);
  lim.update({ phase: 'waiting' }, 'PC', 'ble:x'); // through Bluetooth: three times as long
  assert.strictEqual(timers[2].ms, L.LIMIT_MS.waiting * 3);
  global.setTimeout = real;
  console.log('pairlimit: ok');
})();
