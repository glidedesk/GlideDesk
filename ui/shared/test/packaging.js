// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
'use strict';

// Verifies packaging integrity for GlideDesk (Tauri v2 + Web UI):
// 1. Validates src-tauri/tauri.conf.json structure and paths
// 2. Ensures all scripts & stylesheets referenced in HTML templates exist
// 3. Verifies that all shared/ modules parse cleanly without syntax errors
const assert = require('assert');
const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..', '..');

// 1. Verify Tauri Configuration
const tauriConfPath = path.join(ROOT, 'src-tauri', 'tauri.conf.json');
assert.ok(fs.existsSync(tauriConfPath), 'src-tauri/tauri.conf.json must exist');

const conf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
assert.strictEqual(conf.productName, 'GlideDesk');
assert.strictEqual(conf.build.frontendDist, '../ui');
assert.ok(conf.version, 'Tauri conf must have a version defined');

// 2. Verify HTML template references exist in ui/
const htmlFiles = ['ui/index.html', 'ui/window-projection.html'];
let assetsChecked = 0;

for (const htmlRel of htmlFiles) {
  const fullPath = path.join(ROOT, htmlRel);
  assert.ok(fs.existsSync(fullPath), `${htmlRel} must exist`);
  const content = fs.readFileSync(fullPath, 'utf8');

  // Match <script src="...">
  for (const m of content.matchAll(/<script[^>]+src=["']([^"']+)["']/g)) {
    const assetRel = m[1];
    const assetPath = path.join(ROOT, 'ui', assetRel);
    assert.ok(fs.existsSync(assetPath), `${htmlRel} references missing script: ${assetRel}`);
    assetsChecked++;
  }

  // Match <link rel="stylesheet" href="...">
  for (const m of content.matchAll(/<link[^>]+href=["']([^"']+)["']/g)) {
    const assetRel = m[1];
    const assetPath = path.join(ROOT, 'ui', assetRel);
    assert.ok(fs.existsSync(assetPath), `${htmlRel} references missing stylesheet: ${assetRel}`);
    assetsChecked++;
  }
}

// 3. Verify all shared/ modules exist and parse without syntax errors
const sharedDir = path.join(ROOT, 'ui', 'shared');
const sharedFiles = fs.readdirSync(sharedDir).filter(f => f.endsWith('.js'));
assert.ok(sharedFiles.length > 5, 'Shared directory must contain core modules');

for (const f of sharedFiles) {
  const code = fs.readFileSync(path.join(sharedDir, f), 'utf8');
  assert.doesNotThrow(() => {
    new Function(code);
  }, `Syntax error in shared/${f}`);
}

console.log(`packaging: passed (${assetsChecked} frontend assets validated, ${sharedFiles.length} shared modules verified)`);
