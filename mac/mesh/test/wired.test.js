// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// The USB cable check (wired.js _poll) with no phone connected: it asks hasPhone (from main.js) before
// it looks at the cable, so Wired must keep it; without it every poll threw and no phone on a cable
// was ever found in Multi-device mode.
// Run with: node mac/mesh/test/wired.test.js  (NODE_PATH as in CI)
'use strict';

const assert = require('assert');
const { EventEmitter } = require('events');
const { Wired } = require('../wired.js');

function make(opts) {
  const asked = [];
  const adb = { available: true, devices: async () => { asked.push('devices'); return []; } };
  const peer = Object.assign(new EventEmitter(), { all: () => [], ctxOf: () => null });
  const w = new Wired({ adb, raw: { current: null }, peer, getId: () => 'pc', getName: () => 'PC', ...opts });
  return { w, asked };
}

async function main() {
  // Nothing paired: no error, and adb is not asked (no adb server started for nothing).
  {
    const { w, asked } = make({ hasPhone: () => false });
    try {
      await w._poll();
      assert.deepStrictEqual(asked, [], 'adb not asked without a paired phone');
    } finally {
      w.stop();
    }
  }
  // Without hasPhone (an older caller): the cable is always looked at.
  {
    const { w } = make({});
    try {
      assert.strictEqual(w.hasPhone(), true);
    } finally {
      w.stop();
    }
  }
  console.log('wired: passed');
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
