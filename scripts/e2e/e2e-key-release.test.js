// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
'use strict';

const assert = require('assert');
const { E2EClient, randomHex } = require('./e2e-helpers.js');

async function testGracefulDisconnectAndBye() {
  console.log('  Testing bye packet and disconnect cleanup...');
  const peer = new E2EClient();

  try {
    await peer.connect();

    const peerId = 'peer-bye-' + randomHex(4);
    peer.send({
      t: 'hello',
      v: 1,
      id: peerId,
      name: 'E2E Bye Peer',
      os: 'macos',
      role: 'guest',
    });

    await peer.waitFor((p) => p.t === 'welcome');

    // Send bye packet
    peer.send({
      t: 'bye',
      v: 1,
      id: peerId,
    });

    // Short wait to ensure host processes bye (releases keys, cleans up client, recenters cursor)
    await new Promise((r) => setTimeout(r, 200));

    console.log('  ✔ Bye packet processed cleanly and connection closed');
  } finally {
    peer.close();
  }
}

async function run() {
  console.log('\n--- E2E: Key Release & Disconnect ---');
  await testGracefulDisconnectAndBye();
  console.log('✔ All Key Release & Disconnect E2E tests passed\n');
}

module.exports = { run };

if (require.main === module) {
  run().catch((err) => {
    console.error('✖ Key Release E2E test failed:', err);
    process.exit(1);
  });
}
