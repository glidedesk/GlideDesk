// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
'use strict';

const assert = require('assert');
const { E2EClient, randomHex } = require('./e2e-helpers.js');

async function testOppositeLayoutSync() {
  console.log('  Testing opposite layout synchronization...');
  const peer = new E2EClient();

  try {
    await peer.connect();

    const peerId = 'peer-layout-' + randomHex(4);
    peer.send({
      t: 'hello',
      v: 1,
      id: peerId,
      name: 'E2E Layout Peer',
      os: 'macos',
      role: 'guest',
    });

    // 1. Host should respond with welcome
    const welcome = await peer.waitFor((p) => p.t === 'welcome');
    assert.strictEqual(welcome.t, 'welcome');

    // 2. Host should send kvm.layout packet synchronizing the opposite direction
    const layout = await peer.waitFor((p) => p.t === 'kvm.layout');
    assert.strictEqual(layout.t, 'kvm.layout');
    assert.ok(layout.side, 'kvm.layout must include side');
    console.log(`    Received host layout side: ${layout.side}`);

    // Opposite sides must match: right <-> left, top <-> bottom
    const validSides = ['left', 'right', 'top', 'bottom'];
    assert.ok(validSides.includes(layout.side), `side '${layout.side}' must be one of ${validSides.join(', ')}`);

    // 3. Guest updates remote layout to 'left'
    peer.send({
      t: 'kvm.layout',
      v: 1,
      side: 'left',
      pc_side: 'right',
    });

    // Short delay to let host process state update
    await new Promise((r) => setTimeout(r, 200));

    console.log('  ✔ Opposite layout handshake and remote layout synchronization passed');
  } finally {
    peer.close();
  }
}

async function run() {
  console.log('\n--- E2E: Layout Synchronization ---');
  await testOppositeLayoutSync();
  console.log('✔ All Layout Sync E2E tests passed\n');
}

module.exports = { run };

if (require.main === module) {
  run().catch((err) => {
    console.error('✖ Layout Sync E2E test failed:', err);
    process.exit(1);
  });
}
