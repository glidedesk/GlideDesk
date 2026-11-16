// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tests for shared/mesh-view.js: what the Multi-device page shows for zero, one, two and ten
// devices, and the reasons it can give. Run with: node shared/test/ui-multi.js
'use strict';

const assert = require('assert');
const V = require('../mesh-view.js');

let checks = 0;
function ok(name, fn) {
  fn();
  checks += 1;
  console.log('  ✔ ' + name);
}

function session(id, extra) {
  return Object.assign({ id, name: id.toUpperCase(), os: 'win', address: '192.168.4.11', role: 'idle', mesh: true, pool: true }, extra || {});
}

// The page must not exist with fewer than two devices (spec FR-003): with one, this app is the
// two-device app and nothing multi-device may be visible.
ok('hidden with nobody connected', () => {
  assert.strictEqual(V.visible({}), false);
  assert.strictEqual(V.visible({ mesh: { active: false, sessions: [] } }), false);
  assert.strictEqual(V.sessions({}).length, 0);
  assert.strictEqual(V.summary({}), 'Waiting');
});

ok('hidden with one device', () => {
  const state = { mesh: { active: false, sessions: [session('a')] } };
  assert.strictEqual(V.visible(state), false);
  assert.strictEqual(V.sessions(state).length, 1);
  assert.strictEqual(V.summary(state), 'One device');
});

ok('shown from two devices on', () => {
  const two = { mesh: { active: true, sessions: [session('a'), session('b')] } };
  assert.strictEqual(V.visible(two), true);
  assert.strictEqual(V.summary(two), '2 devices');
});

ok('all ten devices are listed, none duplicated', () => {
  const many = { mesh: { active: true, sessions: Array.from({ length: 10 }, (_, i) => session('d' + i)) } };
  const list = V.sessions(many);
  assert.strictEqual(list.length, 10);
  assert.strictEqual(new Set(list.map((s) => s.id)).size, 10);
  assert.strictEqual(V.summary(many), '10 devices');
  assert.strictEqual(V.visible(many), true);
});

ok('roles are shown in words, unknown roles fall back to idle', () => {
  const state = {
    mesh: {
      active: true,
      sessions: [session('a', { role: 'driving' }), session('b', { role: 'driven' }), session('c', { role: 'nonsense' })],
    },
  };
  const list = V.sessions(state);
  assert.strictEqual(list[0].role, 'driving');
  assert.strictEqual(list[1].role, 'driven');
  assert.strictEqual(list[2].role, 'idle');
  assert.strictEqual(V.roleText('driving'), 'This computer drives it');
  assert.strictEqual(V.roleText('driven'), 'It drives this computer');
  assert.strictEqual(V.roleText('idle'), 'Idle');
});

ok('a state without a mesh payload still lists the connected devices', () => {
  const state = { clients: [{ id: 'a', name: 'A', os: 'mac', address: '10.0.0.2' }, { id: 'b', name: 'B', os: 'linux' }] };
  const list = V.sessions(state);
  assert.strictEqual(list.length, 2);
  assert.strictEqual(list[0].role, 'idle');
  assert.strictEqual(list[0].mesh, false);
  assert.strictEqual(V.visible(state), true, 'two connected devices are two devices, however they are reported');
});

ok('a device with no address or name still renders', () => {
  const state = { mesh: { active: true, sessions: [session('a'), { id: 'b', role: 'idle' }] } };
  const b = V.sessions(state)[1];
  assert.strictEqual(b.name, 'b', 'the id stands in for a missing name');
  assert.strictEqual(b.address, '');
  assert.strictEqual(b.pool, true, 'the pool defaults on');
});

ok('every refusal reason reads as a sentence', () => {
  for (const why of ['limit', 'not_allowed', 'not_connected', 'busy', 'not_ready', 'too_large', 'concealed', 'something_else']) {
    const text = V.refusalText({ device: 'Desktop', why });
    assert.ok(text.length > 10, why + ' must say something');
    assert.ok(/[.!]$/.test(text), why + ' must be a sentence: ' + text);
  }
  assert.ok(V.refusalText({ device: 'Desktop', why: 'limit' }).includes('ten devices'));
  assert.strictEqual(V.refusalText(null), '');
});

ok('device kinds are named for people, not for the wire', () => {
  assert.strictEqual(V.kind('android'), 'Phone');
  assert.strictEqual(V.kind('win'), 'Windows');
  assert.strictEqual(V.kind('mac'), 'Mac');
  assert.strictEqual(V.kind(''), '');
});

// The page itself. A switch or a button that changes nothing is worse than no control at all
// (spec Clarifications), so the wiring is checked here and not only by eye.
const fs = require('fs');
const path = require('path');
const ROOT = path.join(__dirname, '..', '..', '..');
const read = (p) => fs.readFileSync(path.join(ROOT, p), 'utf8');

ok('the page is hidden until two devices are connected', () => {
  const page = read('ui/ui-multi.js');
  assert.ok(/hidden:\s*\(s\)\s*=>\s*!MeshView\.visible\(s\)/.test(page),
    'the tab must disappear with fewer than two devices (FR-003)');
  assert.ok(page.includes('MeshView.sessions(s)'), 'the list is built from the tested model');
  assert.ok(!page.includes('setMulti'), 'there is no multi-device switch to press');
});

ok('the page loads its model before itself', () => {
  const html = read('ui/index.html');
  const model = html.indexOf('shared/mesh-view.js');
  const page = html.indexOf('src="ui-multi.js"');
  assert.ok(model > 0, 'index.html must load shared/mesh-view.js');
  assert.ok(model < page, 'the model must be loaded before the page uses it');
});

ok('every control on the page reaches a command', () => {
  const page = read('ui/ui-multi.js');
  assert.ok(page.includes('disconnectDevice'), 'the per-device button must call a command');
  assert.ok(page.includes('assignEdge'), 'the edge picker must call a command');
  const bridge = read('ui/shared/tauri-bridge.js');
  assert.ok(/disconnectDevice:\s*\(.*\)\s*=>\s*invoke\('disconnect_device'/.test(bridge),
    'the bridge must expose disconnect_device');
  assert.ok(/assignEdge:\s*\(.*\)\s*=>\s*invoke\('assign_edge'/.test(bridge),
    'the bridge must expose assign_edge');
  const main = read('src-tauri/src/main.rs');
  assert.ok(main.includes('commands::disconnect_device'), 'the command must be registered');
  assert.ok(main.includes('commands::assign_edge'), 'the edge command must be registered');
  const commands = read('src-tauri/src/commands.rs');
  assert.ok(commands.includes('pub async fn disconnect_device'), 'the command must exist');
  assert.ok(commands.includes('pub fn assign_edge'), 'the edge command must exist');
});

ok('the edge picker uses the tested model, not a second set of rules', () => {
  const page = read('ui/ui-multi.js');
  assert.ok(page.includes('EdgesUI.options('), 'the choices come from the tested model');
  assert.ok(page.includes('EdgesUI.change('), 'so does the command to send');
  assert.ok(!page.includes('edges[sid]'), 'no side logic of its own');
  const html = read('ui/index.html');
  const model = html.indexOf('shared/edges-ui.js');
  const self = html.indexOf('src="ui-multi.js"');
  assert.ok(model > 0 && model < self, 'edges-ui.js must load before the page');
});

// "Pair once, meet everyone" (FR-021): a device without a session, reported by a peer's roster,
// is listed but is not one of the connected devices.
ok('a device learned through a peer is listed, not counted as connected', () => {
  const state = {
    mesh: {
      active: true,
      sessions: [session('a'), session('b')],
      known: [
        { id: 'a', name: 'A', os: 'win', direct: true, via: [] },
        { id: 'b', name: 'B', os: 'mac', direct: true, via: [] },
        { id: 'phone', name: 'Phone', os: 'android', direct: false, via: ['a'] },
      ],
    },
  };
  assert.strictEqual(V.sessions(state).length, 2, 'the phone is not a session');
  const known = V.known(state);
  assert.strictEqual(known.length, 1, 'only the indirect device is listed here');
  assert.strictEqual(known[0].id, 'phone');
  assert.strictEqual(known[0].name, 'Phone');
  assert.deepStrictEqual(known[0].via, ['a']);
});

ok('a state from before the roster existed lists nobody', () => {
  assert.deepStrictEqual(V.known({}), []);
  assert.deepStrictEqual(V.known({ mesh: { sessions: [session('a'), session('b')] } }), []);
});

ok('the same device is not listed twice', () => {
  const state = {
    mesh: {
      active: true,
      sessions: [session('a'), session('b'), session('phone')],
      known: [
        { id: 'phone', name: 'Phone', os: 'android', direct: false, via: ['a', 'b'] },
      ],
    },
  };
  assert.deepStrictEqual(V.known(state), [], 'a device with a session belongs to Connected now');
});

ok('the "also on this desk" Forget reaches a command', () => {
  const page = read('ui/ui-multi.js');
  assert.ok(page.includes('MeshView.known('), 'the list comes from the tested model');
  assert.ok(page.includes('forgetDevice'), 'the Forget button must call a command');
  const bridge = read('ui/shared/tauri-bridge.js');
  assert.ok(/forgetDevice:\s*\(.*\)\s*=>\s*invoke\('forget_device'/.test(bridge),
    'the bridge must expose forget_device');
  const main = read('src-tauri/src/main.rs');
  assert.ok(main.includes('commands::forget_device'), 'the command must be registered');
  const commands = read('src-tauri/src/commands.rs');
  assert.ok(commands.includes('pub fn forget_device'), 'the command must exist');
});

console.log('ui-multi: ' + checks + ' checks passed');
