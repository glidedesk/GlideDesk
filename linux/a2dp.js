// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The phone's sound here as its Bluetooth speaker on Linux (shared/btaudio.js's backend): BlueZ
// connects the paired phone's A2DP audio source (Device1.ConnectProfile), and the sound server
// (PipeWire's Bluetooth support, or PulseAudio's module-bluetooth-policy) plays it on the
// default output, as with any phone that connects to a Linux computer as to a speaker.
'use strict';

const { execFile } = require('child_process');

const A2DP_SOURCE = '0000110a-0000-1000-8000-00805f9b34fb'; // the phone's side: it sends the sound

const run = (cmd, args, timeout = 8000) => new Promise((resolve, reject) => {
  execFile(cmd, args, { timeout }, (err, out, errOut) => (err ? reject(new Error(String(errOut || err.message).trim().split('\n').pop())) : resolve(String(out || ''))));
});

// BlueZ's object path of a device (any adapter): /org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF.
async function pathOf(mac) {
  const tail = 'dev_' + mac.toUpperCase().replace(/:/g, '_');
  const out = await run('busctl', ['--system', 'tree', '--list', 'org.bluez']);
  const p = out.split('\n').map((l) => l.trim()).find((l) => l.endsWith('/' + tail));
  if (!p) throw new Error('BlueZ doesn’t know that device');
  return p;
}

class LinuxA2dp {
  constructor() {
    this.fns = [];
    this.open_ = '';
  }

  on(fn) {
    this.fns.push(fn);
  }

  // Paired devices that send sound (their UUIDs list an Audio Source).
  async list() {
    let out = await run('bluetoothctl', ['devices', 'Paired']).catch(() => '');
    if (!/Device /.test(out)) out = await run('bluetoothctl', ['paired-devices']); // BlueZ before 5.65
    const devices = [];
    for (const l of out.split('\n')) {
      const m = /^Device\s+([0-9A-F:]{17})\s+(.*)$/i.exec(l.trim());
      if (!m) continue;
      const info = await run('bluetoothctl', ['info', m[1]]).catch(() => '');
      if (info.toLowerCase().includes(A2DP_SOURCE)) devices.push({ id: m[1].toUpperCase(), name: m[2].trim() });
    }
    return devices;
  }

  async open(id) {
    if (!/^[0-9A-F:]{17}$/i.test(id)) throw new Error('not a Bluetooth address');
    const path = await pathOf(id);
    await run('gdbus', ['call', '--system', '--dest', 'org.bluez', '--object-path', path, '--method', 'org.bluez.Device1.ConnectProfile', A2DP_SOURCE], 30000);
    this.open_ = id;
    for (const fn of this.fns) fn('opened');
  }

  async close() {
    const id = this.open_;
    this.open_ = '';
    if (!id) return;
    try {
      await run('gdbus', ['call', '--system', '--dest', 'org.bluez', '--object-path', await pathOf(id), '--method', 'org.bluez.Device1.DisconnectProfile', A2DP_SOURCE]);
    } finally {
      for (const fn of this.fns) fn('closed');
    }
  }
}

module.exports = { LinuxA2dp };
