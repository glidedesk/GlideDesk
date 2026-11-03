// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// ../../shared/sleepwake.js for several devices: going to sleep ends every session (server.sleep
// returns their ids), waking invites each of them back as soon as discovery sees it on the network.
// The one-to-one app keeps the shared one.
'use strict';

const INVITE_FOR_MS = 60000; // after waking: look for them this long
const INVITE_EVERY_MS = 2000; // … inviting each one each time it's seen (UDP may drop one) until it's back

// server() / discovery(): ours (either may not exist yet); powerMonitor: Electron's.
function sleepWake({ powerMonitor, server, discovery }) {
  let guests = []; // the device ids that were connected when we went to sleep
  let timer = null;
  powerMonitor.on('suspend', () => {
    clearInterval(timer);
    const s = server();
    const ids = s ? s.sleep(true) : [];
    if (ids.length) guests = ids;
    console.log(`[sleep] going to sleep${ids.length ? `: ${ids.length} session(s) end (asleep)` : ''}`);
  });
  powerMonitor.on('resume', () => {
    const s = server();
    if (s) s.sleep(false);
    let waiting = guests;
    guests = [];
    console.log('[sleep] awake');
    if (!waiting.length) return;
    const until = Date.now() + INVITE_FOR_MS;
    clearInterval(timer);
    timer = setInterval(() => {
      waiting = waiting.filter((id) => !(s && s.clients.has(id))); // back already
      if (!waiting.length || Date.now() > until) return clearInterval(timer);
      for (const id of waiting) {
        const d = discovery() && discovery().get(id);
        if (d && d.address && !String(d.address).startsWith('ble:')) discovery().invite(d.address);
      }
    }, INVITE_EVERY_MS);
  });
}

module.exports = { sleepWake };
