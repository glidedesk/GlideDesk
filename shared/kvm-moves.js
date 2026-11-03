// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Pointer moves summed into one message: on the driving side before they go to the other computer,
// on the driven side before they go to the helper. A move goes on the next turn of the event loop
// (all that came in meanwhile, summed), or after delay() ms (through Bluetooth every message is a
// round trip on the radio). While the way out is backed up (backlog() bytes above limit(): the
// other side or the link fell behind), moves keep summing instead of queueing, so the pointer goes
// to where it is now rather than replaying old moves: until there's room, or at most maxWait() ms
// (then one goes anyway: behind a big message the pointer still moves). Anything else (a click, a
// key) flush()es first, so it lands where the moves before it took the pointer.
'use strict';

const RETRY_MS = 2; // backed up: look again this often

class KvmMoves {
  // opts: send({dx, dy}); delay() -> ms (0: next turn); backlog() -> bytes waiting; limit() -> bytes;
  //   maxWait() -> ms backed up before one goes anyway (Infinity: wait for room). All but send optional.
  constructor(opts) {
    this.o = opts;
    this.pending = null;
    this.timer = null;
    this.since = 0; // first held back at (backed up)
  }

  add(dx, dy) {
    KvmMoves.count++;
    if (!this.pending) {
      this.pending = { dx: 0, dy: 0 };
      this._schedule(this.o.delay ? this.o.delay() : 0);
    }
    this.pending.dx += dx;
    this.pending.dy += dy;
  }

  // Send what's summed now.
  flush() {
    this._clear();
    const m = this.pending;
    this.pending = null;
    this.since = 0;
    if (m) this.o.send(m);
  }

  // Forget what's summed (driving ended).
  drop() {
    this._clear();
    this.pending = null;
    this.since = 0;
  }

  _schedule(ms) {
    if (this.timer) return;
    const go = () => {
      this.timer = null;
      this._try();
    };
    this.timer = ms > 0 ? { t: setTimeout(go, ms) } : { i: setImmediate(go) };
  }

  _clear() {
    if (!this.timer) return;
    if (this.timer.t) clearTimeout(this.timer.t);
    else clearImmediate(this.timer.i);
    this.timer = null;
  }

  _try() {
    if (!this.pending) return;
    const backlog = this.o.backlog ? this.o.backlog() : 0;
    const limit = this.o.limit ? this.o.limit() : Infinity;
    if (backlog > limit) {
      const now = Date.now();
      if (!this.since) this.since = now;
      const maxWait = this.o.maxWait ? this.o.maxWait() : Infinity;
      if (now - this.since < maxWait) return this._schedule(Math.max(RETRY_MS, this.o.delay ? this.o.delay() : 0));
    }
    this.flush();
  }
}

KvmMoves.count = 0; // moves added, all of them (shared/applog.js: moves a second in the log)

module.exports = { KvmMoves };
