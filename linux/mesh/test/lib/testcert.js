// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// Throwaway TLS certificate for the tests: made once per test process with openssl into a temp
// folder, which is deleted when the process exits. No key material is kept in the repository.
'use strict';

const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');
const P = require('../../../../shared/protocol.js');

let cached = null;

// Returns {key, cert, fp}: PEM strings, as linux/server.js takes them (fp: normalized sha256 fingerprint).
function loadTestCert() {
  if (cached) return cached;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'twindesk-test-'));
  const key = path.join(dir, 'k.pem');
  const cert = path.join(dir, 'c.pem');
  const env = { ...process.env, MSYS_NO_PATHCONV: '1' };
  process.on('exit', () => fs.rmSync(dir, { recursive: true, force: true }));
  execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', cert, '-days', '2', '-subj', '/CN=Test'], { env, stdio: 'ignore' });
  const fp = P.normFingerprint(new crypto.X509Certificate(fs.readFileSync(cert, 'utf8')).fingerprint256);
  cached = { key: fs.readFileSync(key, 'utf8'), cert: fs.readFileSync(cert, 'utf8'), fp };
  return cached;
}

module.exports = { loadTestCert };
