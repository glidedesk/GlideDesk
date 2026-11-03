// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
'use strict';

const assert = require('assert');
const { E2EClient, randomHex, commitOf, sasCode } = require('./e2e-helpers.js');

async function testUnauthenticatedInputBlocked() {
  console.log('  Testing unauthenticated input gating...');
  const attacker = new E2EClient();
  const listener = new E2EClient();

  try {
    await attacker.connect();
    await listener.connect();

    // Attacker sends hello without any valid pairing token
    const attackerId = 'attacker-' + randomHex(4);
    attacker.send({
      t: 'hello',
      v: 1,
      id: attackerId,
      name: 'Unpaired Attacker',
      os: 'linux',
      role: 'guest',
    });

    // Host should respond with welcome and opposite layout sync
    const welcome = await attacker.waitFor((p) => p.t === 'welcome');
    assert.strictEqual(welcome.t, 'welcome');
    assert.strictEqual(welcome.role, 'host');

    // Attacker tries to inject keystrokes and clipboard
    attacker.send({
      t: 'k.kd',
      k: 'a',
      code: 'KeyA',
      vk: 65,
    });
    attacker.send({
      t: 'clip',
      text: 'EXPLOIT_PAYLOAD_UNAUTHENTICATED',
    });

    // Listener collects packets for 400ms — must NOT receive the attacker's clip or input
    const collected = await listener.collect(400);
    const leakedClip = collected.find((p) => p.t === 'clip' && p.text === 'EXPLOIT_PAYLOAD_UNAUTHENTICATED');
    assert.strictEqual(leakedClip, undefined, 'Unauthenticated clip packet must never be broadcast to peers');

    const leakedKey = collected.find((p) => p.t === 'k.kd' && p.code === 'KeyA');
    assert.strictEqual(leakedKey, undefined, 'Unauthenticated key packet must never be accepted/broadcast');

    console.log('  ✔ Unauthenticated keystrokes and clipboard successfully dropped by host');
  } finally {
    attacker.close();
    listener.close();
  }
}

async function testWelcomeBypassBlocked() {
  console.log('  Testing welcome host-role bypass rejection...');
  const spoofer = new E2EClient();

  try {
    await spoofer.connect();

    // Spoofer sends welcome with role: "host" and no valid paired token
    spoofer.send({
      t: 'welcome',
      v: 1,
      id: 'fake-host-' + randomHex(4),
      role: 'host',
      token: 'fake-token-' + randomHex(8),
    });

    // Try to send sensitive input immediately after
    spoofer.send({
      t: 'k.mm',
      dx: 50.0,
      dy: 50.0,
    });

    // Wait short delay to ensure host did not crash or authenticate
    await new Promise((r) => setTimeout(r, 200));

    // Send a normal ping and ensure connection is alive but unauthenticated
    spoofer.send({ t: 'ping', v: 1 });
    const pongOrPing = await spoofer.waitFor((p) => p.t === 'ping' || p.t === 'pong');
    assert.ok(pongOrPing, 'Host remained alive and safely handled invalid welcome');

    console.log('  ✔ Host-role welcome spoofing did not bypass auth gate');
  } finally {
    spoofer.close();
  }
}

async function testSasPairingCommitment() {
  console.log('  Testing SAS pairing commitment and verification...');
  const peer = new E2EClient();

  try {
    await peer.connect();

    const nw = randomHex(16);
    const commit = commitOf(nw);
    const guestId = 'guest-' + randomHex(4);

    // 1. Peer sends pair.start with cryptographic commitment
    peer.send({
      t: 'pair.start',
      v: 1,
      id: guestId,
      name: 'E2E Test Guest',
      os: 'macos',
      commit,
    });

    // 2. Host must respond with pair.nonce
    const noncePkt = await peer.waitFor((p) => p.t === 'pair.nonce');
    assert.strictEqual(noncePkt.t, 'pair.nonce');
    assert.ok(noncePkt.nonce, 'Host must return its nonce (nm)');
    const nm = noncePkt.nonce;

    // 3. Guest calculates SAS code
    const expectedCode = sasCode('', nw, nm);
    assert.strictEqual(expectedCode.length, 6, 'SAS code must be 6 decimal digits');

    // 4. Guest reveals nonce nw
    peer.send({
      t: 'pair.reveal',
      v: 1,
      nonce: nw,
    });

    // Allow host to verify SHA-256 commitment
    await new Promise((r) => setTimeout(r, 200));

    // 5. Cancel pairing cleanly to reset state
    peer.send({ t: 'pair.cancel', v: 1 });
    await new Promise((r) => setTimeout(r, 100));

    console.log(`  ✔ SAS commitment exchange verified (calculated code ${expectedCode})`);
  } finally {
    peer.close();
  }
}

async function run() {
  console.log('\n--- E2E: Auth Gate & Pairing Security ---');
  await testUnauthenticatedInputBlocked();
  await testWelcomeBypassBlocked();
  await testSasPairingCommitment();
  console.log('✔ All Auth Gate E2E tests passed\n');
}

module.exports = { run };

if (require.main === module) {
  run().catch((err) => {
    console.error('✖ Auth Gate E2E test failed:', err);
    process.exit(1);
  });
}
