// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Audio › Play phone sound here (Bluetooth): this computer as the connected phone's Bluetooth
// speaker (A2DP), so its music, videos and every other sound play here; the phone sends it as to
// any speaker. Only while a phone is connected, paired with this computer over classic Bluetooth
// (in the system's Bluetooth settings, as for calls), with the switch on (off until turned on).
// The device is the one picked before, else the one with the phone's Bluetooth name (hub.state
// bt.name); with several and none matching, the tab asks which.
// backend: {list() → [{id, name}], open(id), close(), on(fn(state 'opened'|'closed'))}:
//   Windows: windows-helper's AudioPlaybackConnection (A2dp.cs); Linux: BlueZ (linux/a2dp.js).
// Events: 'change'.
'use strict';

const { EventEmitter } = require('events');
const P = require('./protocol.js');

const T = P.T;
const RETRY_MS = [3000, 10000, 30000, 60000];
const isPhone = (ctx) => !!ctx && ctx.os === P.OS.ANDROID;

class BtAudio extends EventEmitter {
  // server: the phone link ('client', 'closed', 't:<type>'); cfg() → settings.audio; save()
  constructor({ server, backend, cfg, save }) {
    super();
    Object.assign(this, { server, backend, cfg, save });
    this.phone = null; // {name (its Bluetooth name), bonded}
    this.devices = [];
    this.state = 'off'; // off | looking | connecting | playing | choose | needs-pair | failed | unavailable
    this.error = '';
    this.id = '';
    this.tries = 0;
    this.timer = null;
    server.on('client', (ctx) => isPhone(ctx) && this.apply());
    server.on('closed', (ctx) => {
      if (!isPhone(ctx)) return;
      this.phone = null;
      this.apply();
    });
    server.on(`t:${T.HUB_STATE}`, (m, ctx) => {
      if (!isPhone(ctx) || !m) return;
      const bt = m.bt && typeof m.bt === 'object' ? m.bt : {};
      const was = this.phone && this.phone.name;
      this.phone = { name: String(bt.name || '').slice(0, 100), bonded: bt.bonded === true ? true : bt.bonded === false ? false : null };
      if (was !== this.phone.name || ['needs-pair', 'choose'].includes(this.state)) this.apply();
    });
    backend.on((s) => {
      if (s === 'opened') this._set('playing');
      else if (this.state === 'playing') this._retry('the phone went'); // dropped: again in a moment
    });
  }

  // Not "on": that would hide EventEmitter's on(), and nothing could listen (it broke the PC's phone setup).
  get enabled() {
    return this.cfg().phoneBt === true;
  }

  get connected() {
    return isPhone(this.server.current);
  }

  // The switch, a phone connecting or going: on with a phone there, off otherwise.
  async apply() {
    clearTimeout(this.timer);
    if (!this.enabled || !this.connected) {
      this.tries = 0;
      if (this.state !== 'off') {
        this._set('off'); // first: its 'closed' isn't a drop
        await this.backend.close().catch(() => {});
      }
      return;
    }
    if (this.state === 'playing' || this.state === 'connecting') return;
    this._set('looking');
    try {
      this.devices = (await this.backend.list()) || [];
    } catch (e) {
      this.error = e.message;
      return this._set('unavailable');
    }
    if (!this.enabled || !this.connected) return this.apply();
    const saved = this.cfg().phoneBtDevice || '';
    const name = String((this.phone && this.phone.name) || (this.server.current && this.server.current.name) || '').trim().toLowerCase();
    // Never just the first one: that may be another phone paired before.
    const d = this.devices.find((x) => x.id === saved)
      || (name && this.devices.find((x) => String(x.name).trim().toLowerCase() === name));
    if (!d) return this._set(this.devices.length ? 'choose' : 'needs-pair');
    this._connect(d.id);
  }

  // The tab's choice among the paired devices.
  pick(id) {
    if (!this.devices.some((d) => d.id === id)) return;
    this.cfg().phoneBtDevice = id;
    this.save();
    this.tries = 0;
    this._set('looking');
    this.backend.close().catch(() => {}).then(() => this._connect(id));
  }

  async _connect(id) {
    this.id = id;
    if (id !== this.cfg().phoneBtDevice) {
      this.cfg().phoneBtDevice = id;
      this.save();
    }
    this._set('connecting');
    try {
      await this.backend.open(id);
      this.tries = 0;
      this.error = '';
      this._set('playing');
    } catch (e) {
      this._retry(e.message);
    }
  }

  _retry(why) {
    this.error = String(why || '').slice(0, 160);
    this._set('failed');
    const wait = RETRY_MS[this.tries++];
    clearTimeout(this.timer);
    if (wait) this.timer = setTimeout(() => { if (this.enabled && this.connected) { this.state = 'off'; this.apply(); } }, wait);
  }

  // Off now (the switch, quitting).
  stop() {
    clearTimeout(this.timer);
    this._set('off');
    this.backend.close().catch(() => {});
  }

  _set(state) {
    this.state = state;
    this.emit('change');
  }

  get view() {
    const dev = this.devices.find((d) => d.id === this.id);
    const phone = (this.phone && this.phone.name) || (this.server.current && this.server.current.name) || 'the phone';
    const TEXT = {
      off: '',
      looking: 'Looking for it…',
      connecting: `Connecting to ${(dev && dev.name) || phone}…`,
      playing: `${(dev && dev.name) || phone}’s sound plays here`,
      choose: 'Which of the paired devices is the phone?',
      'needs-pair': `Not paired with ${phone} over classic Bluetooth: pair them in Bluetooth settings first`,
      failed: `Didn’t connect: ${this.error || 'the phone didn’t answer'}. Trying again…`,
      unavailable: this.error || 'Bluetooth is off on this computer',
    };
    return {
      on: this.enabled,
      state: this.state,
      text: TEXT[this.state] || '',
      devices: this.state === 'choose' || this.state === 'playing' ? this.devices : [],
      device: this.id,
      bonded: this.phone ? this.phone.bonded : null,
    };
  }
}

module.exports = { BtAudio };
