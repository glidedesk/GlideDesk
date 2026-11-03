// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// shared/received.js: General › Received files, the folder in use and what's kept of a saved one.
'use strict';

const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { receivedDir, normalizeFolder } = require('../received.js');

const DL = path.join(os.homedir(), 'Downloads');
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'twindesk-received-'));
const at = (receivedFolder) => receivedDir({ files: { receivedFolder } }, DL);

try {
  // Not chosen: Downloads.
  assert.strictEqual(at(''), DL);
  assert.strictEqual(receivedDir({ files: {} }, DL), DL);
  assert.strictEqual(receivedDir(null, DL), DL);
  // Chosen and there: that folder.
  assert.strictEqual(at(dir), dir);
  // Chosen but gone (an unplugged drive): Downloads meanwhile, and nothing is made.
  const gone = path.join(dir, 'gone');
  assert.strictEqual(at(gone), DL);
  assert.ok(!fs.existsSync(gone));
  // A file, not a folder; a relative path: Downloads.
  const file = path.join(dir, 'a.txt');
  fs.writeFileSync(file, 'x');
  assert.strictEqual(at(file), DL);
  assert.strictEqual(at('Received'), DL);

  // Saved: an absolute path as it is, anything else ''.
  assert.strictEqual(normalizeFolder(dir), dir);
  assert.strictEqual(normalizeFolder(''), '');
  assert.strictEqual(normalizeFolder('relative/dir'), '');
  assert.strictEqual(normalizeFolder(42), '');
  assert.strictEqual(normalizeFolder(path.join(dir, 'x'.repeat(1200))), '');
  console.log('received: ok');
} finally {
  fs.rmSync(dir, { recursive: true, force: true });
}
