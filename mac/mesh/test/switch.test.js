// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// The Multi-device switch (docs/multi-device.md, rules 1 and 2), with `electron` stubbed: off (no
// multi.json, or on: false), mac/main.js loads nothing of mac/mesh/ or the mesh core; on, it hands
// over to mesh/main.js, which loads with every module it requires. Each case runs in a child process of
// its own, as each is a start of the app.
// Run with: node mac/mesh/test/switch.test.js  (NODE_PATH as in CI)
'use strict';

const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');
const { runTests } = require('../../../windows/mesh/test/lib/run.js');

const MAC = path.resolve(__dirname, '..', '..');

// One start: electron stubbed (userData = dir), then mac/main.js; prints the app files it loaded.
function start(dir) {
  const script = `
    const Module = require('module');
    const values = { getPath: () => ${JSON.stringify(dir)}, getVersion: () => '0.0.0-test', isPackaged: false, hasSwitch: () => false };
    const target = function stub() {};
    const stub = new Proxy(target, {
      get(t, key) {
        if (key === Symbol.toPrimitive) return () => '';
        if (key === 'then') return () => stub; // app.whenReady().then(start): start never runs
        if (key in values) return values[key];
        return stub;
      },
      apply() { return stub; },
      construct() { return stub; },
    });
    const realLoad = Module._load;
    Module._load = function (request) { return request === 'electron' ? stub : realLoad.apply(this, arguments); };
    require(${JSON.stringify(path.join(MAC, 'main.js'))});
    const root = ${JSON.stringify(path.resolve(MAC, '..'))};
    console.log(JSON.stringify(Object.keys(require.cache).map((f) => path.relative(root, f).split(path.sep).join('/'))));
  `;
  const out = execFileSync(process.execPath, ['-e', `const path = require('path');${script}`], { env: process.env, encoding: 'utf8' });
  return JSON.parse(out.trim().split('\n').pop());
}

const MESH_CORE = ['shared/mesh.js', 'shared/intro.js', 'shared/layout.js', 'shared/kvm-edges.js', 'shared/screen-mesh.js'];

const tests = [];
const test = (name, fn) => tests.push([name, fn]);

test('off (no multi.json): main.js loads nothing of the mesh', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'twindesk-switch-'));
  const files = start(dir);
  assert.ok(files.includes('mac/main.js') && files.includes('mac/link.js'), 'the one-to-one app loaded');
  assert.deepStrictEqual(files.filter((f) => f.startsWith('mac/mesh/')), [], 'no mac/mesh/ module');
  assert.deepStrictEqual(files.filter((f) => MESH_CORE.includes(f)), [], 'no mesh core');
});

test('off (on: false): the same', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'twindesk-switch-'));
  fs.writeFileSync(path.join(dir, 'multi.json'), JSON.stringify({ on: false, layout: { rev: 3 } }));
  const files = start(dir);
  assert.deepStrictEqual(files.filter((f) => f.startsWith('mac/mesh/') || MESH_CORE.includes(f)), []);
});

test('on: main.js hands over to mesh/main.js, which loads with all it requires', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'twindesk-switch-'));
  fs.writeFileSync(path.join(dir, 'multi.json'), JSON.stringify({ on: true }));
  const files = start(dir);
  for (const f of ['mac/mesh/main.js', 'mac/mesh/hub.js', 'mac/mesh/server.js', 'mac/mesh/kvm.js', 'mac/mesh/ipc.js', 'mac/mesh/services.js', 'shared/mesh.js']) {
    assert.ok(files.includes(f), `${f} loaded`);
  }
});

runTests(tests);
