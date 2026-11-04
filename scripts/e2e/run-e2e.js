#!/usr/bin/env node
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
'use strict';

const net = require('net');
const path = require('path');
const { spawn } = require('child_process');
const { DEFAULT_URL } = require('./e2e-helpers.js');

const authGate = require('./e2e-auth-gate.test.js');
const layoutSync = require('./e2e-layout-sync.test.js');
const keyRelease = require('./e2e-key-release.test.js');
const kvmTransfer = require('./e2e-kvm-transfer-lifecycle.test.js');

function checkPortOpen(port = 47800, host = '127.0.0.1', timeoutMs = 1000) {
  return new Promise((resolve) => {
    const socket = new net.Socket();
    let isConnected = false;

    socket.setTimeout(timeoutMs);
    socket.once('connect', () => {
      isConnected = true;
      socket.destroy();
      resolve(true);
    });

    socket.once('timeout', () => {
      socket.destroy();
      resolve(false);
    });

    socket.once('error', () => {
      resolve(false);
    });

    socket.connect(port, host);
  });
}

async function runE2E() {
  console.log('\n\x1b[1m\x1b[35m============================================================\x1b[0m');
  console.log('\x1b[1m\x1b[35m               GlideDesk Live E2E Test Suite                \x1b[0m');
  console.log('\x1b[1m\x1b[35m============================================================\x1b[0m\n');

  const url = new URL(DEFAULT_URL);
  const port = parseInt(url.port, 10) || 47800;
  const host = url.hostname || '127.0.0.1';

  let spawnedProcess = null;
  const isOpen = await checkPortOpen(port, host);

  if (!isOpen) {
    console.log(`[E2E Runner] No instance listening on ${DEFAULT_URL}. Attempting to launch GlideDesk...`);
    const rootDir = path.resolve(__dirname, '../..');
    const binaryPath = path.join(rootDir, 'src-tauri/target/debug/glidedesk');

    spawnedProcess = spawn(binaryPath, [], {
      cwd: rootDir,
      stdio: 'ignore',
      detached: false,
    });

    // Poll until port opens (up to 5s)
    let ready = false;
    for (let i = 0; i < 25; i++) {
      await new Promise((r) => setTimeout(r, 200));
      if (await checkPortOpen(port, host)) {
        ready = true;
        break;
      }
    }

    if (!ready) {
      if (spawnedProcess) spawnedProcess.kill();
      throw new Error(`Failed to connect to GlideDesk at ${DEFAULT_URL} — server did not start in time.`);
    }
    console.log(`[E2E Runner] Launched temporary GlideDesk instance (PID ${spawnedProcess.pid})`);
  } else {
    console.log(`[E2E Runner] Target server is active at ${DEFAULT_URL}`);
  }

  const results = [];
  const suites = [
    { name: 'Auth Gate & SAS Pairing Security', fn: authGate.run },
    { name: 'Opposite Layout Sync & Handshake', fn: layoutSync.run },
    { name: 'Graceful Disconnect & Key Release', fn: keyRelease.run },
    { name: 'KVM Edge-Crossing & File Transfer', fn: kvmTransfer.run },
  ];

  let passed = 0;
  let failed = 0;

  for (const suite of suites) {
    const start = Date.now();
    try {
      await suite.fn();
      const duration = ((Date.now() - start) / 1000).toFixed(2);
      results.push({ name: suite.name, status: 'PASS', duration: `${duration}s` });
      passed++;
    } catch (err) {
      const duration = ((Date.now() - start) / 1000).toFixed(2);
      console.error(`\x1b[31m✖ Suite failed: ${suite.name}\x1b[0m`, err);
      results.push({ name: suite.name, status: 'FAIL', duration: `${duration}s` });
      failed++;
    }
  }

  if (spawnedProcess) {
    console.log(`[E2E Runner] Shutting down temporary GlideDesk instance...`);
    spawnedProcess.kill();
  }

  console.log('\n\x1b[1m\x1b[35m============================================================\x1b[0m');
  console.log('\x1b[1m\x1b[35m                    E2E Test Summary                        \x1b[0m');
  console.log('\x1b[1m\x1b[35m============================================================\x1b[0m');

  for (const r of results) {
    const icon = r.status === 'PASS' ? '\x1b[32m✔ [PASS]\x1b[0m' : '\x1b[31m✖ [FAIL]\x1b[0m';
    console.log(` ${icon} ${r.name.padEnd(42)} \x1b[90m(${r.duration})\x1b[0m`);
  }

  console.log('------------------------------------------------------------');
  if (failed === 0) {
    console.log(`\x1b[32m ALL ${passed} E2E TEST SUITES PASSED! \x1b[0m\n`);
    process.exit(0);
  } else {
    console.error(`\x1b[31m ${failed} of ${passed + failed} E2E TEST SUITES FAILED. \x1b[0m\n`);
    process.exit(1);
  }
}

if (require.main === module) {
  runE2E().catch((err) => {
    console.error('Fatal E2E error:', err);
    process.exit(1);
  });
}

module.exports = { runE2E };
