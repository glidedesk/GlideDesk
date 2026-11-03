// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Wireless debugging's Pair device with QR code, from here (Connection, shared/ui-adbqr.js): the QR
// carries a service name and password of ours (WIFI:T:ADB;S:<name>;P:<password>;;); the phone, once
// it scanned it, announces a pairing service by that name over mDNS, and `adb pair` with the
// password pairs, as Android Studio does. The 6-digit code stays as it was.
//   const q = startAdbQr({adb, onPaired(), onEnd(why)}); q.text (for the QR), q.stop()
'use strict';

const crypto = require('crypto');

const POLL_MS = 1500;
const LIMIT_MS = 3 * 60000;
const ALNUM = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789';
const word = (n) => [...crypto.randomBytes(n)].map((b) => ALNUM[b % ALNUM.length]).join('');

function startAdbQr({ adb, onPaired, onEnd }) {
  const name = 'twindesk-' + word(8);
  const pass = word(10);
  let stopped = false;
  let busy = false;
  const started = Date.now();
  const end = (why) => {
    if (stopped) return;
    stopped = true;
    clearInterval(timer);
    if (onEnd) onEnd(why);
  };
  const timer = setInterval(async () => {
    if (stopped || busy) return;
    if (Date.now() - started > LIMIT_MS) return end('timeout');
    busy = true;
    try {
      const s = (await adb.mdns()).find((x) => x.type === 'pairing' && x.name === name);
      if (s) {
        const r = await adb.pair(s.address, s.port, pass);
        if (r.ok) { end('paired'); if (onPaired) onPaired(s); } else end('failed: ' + (r.text.split('\n').pop() || 'no answer'));
      }
    } catch (e) { /* the next look */ }
    busy = false;
  }, POLL_MS);
  return { text: `WIFI:T:ADB;S:${name};P:${pass};;`, stop: () => end('stopped') };
}

module.exports = { startAdbQr };
