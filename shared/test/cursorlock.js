// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The cursor lock key (shared/cursorlock.js): what may be a lock key, and the key while driving.
// Run with: node shared/test/cursorlock.js
'use strict';

const assert = require('assert');
const CL = require('../cursorlock.js');

// validKeys: a bare key only from the ones nobody types with; a combination needs more than Shift.
assert.strictEqual(CL.validKeys('Scrolllock', 'X'), 'Scrolllock');
assert.strictEqual(CL.validKeys('F14', 'X'), 'F14');
assert.strictEqual(CL.validKeys('F24', 'X'), 'F24');
assert.strictEqual(CL.validKeys('Control+Alt+L', 'X'), 'Control+Alt+L');
assert.strictEqual(CL.validKeys('Command+Shift+L', 'X'), 'Command+Shift+L');
assert.strictEqual(CL.validKeys('Super+Scrolllock', 'X'), 'Super+Scrolllock');
assert.strictEqual(CL.validKeys('', 'X'), '', 'no lock key');
for (const bad of ['L', 'F5', 'Shift+L', 'Pause', 'Control+Control+L', 'Hyper+L', 'Control+', 42, null, undefined, 'a'.repeat(80)]) {
  assert.strictEqual(CL.validKeys(bad, 'X'), 'X', `refused: ${bad}`);
}

assert.strictEqual(CL.keyName('KeyL'), 'L');
assert.strictEqual(CL.keyName('Digit4'), '4');
assert.strictEqual(CL.keyName('ScrollLock'), 'Scrolllock');
assert.strictEqual(CL.keyName('ArrowUp'), 'Up');
assert.strictEqual(CL.keyName('F14'), 'F14');

assert.strictEqual(CL.keysLabel('Scrolllock', 'linux'), 'Scroll Lock');
assert.strictEqual(CL.keysLabel('Control+Alt+L', 'win'), 'Ctrl+Alt+L');
assert.strictEqual(CL.keysLabel('Control+Command+L', 'mac'), '⌃⌘L');
assert.strictEqual(CL.keysLabel('', 'win'), 'None');

// LockKey: 'press' for the key, its release and autorepeat used up too; everything else (modifiers included) goes through.
const down = (code) => ({ t: 'k.kd', code });
const up = (code) => ({ t: 'k.ku', code });
let acc = 'Scrolllock';
const k = new CL.LockKey(() => acc);
assert.strictEqual(k.intercept(down('ScrollLock')), 'press');
assert.strictEqual(k.intercept(down('ScrollLock')), true, 'autorepeat: used up, not another press');
assert.strictEqual(k.intercept(up('ScrollLock')), true, 'its release is used up too');
assert.strictEqual(k.intercept(up('ScrollLock')), false, 'only once');
assert.strictEqual(k.intercept(down('KeyA')), false);
assert.strictEqual(k.intercept({ t: 'k.mm', dx: 1, dy: 1 }), false);
assert.strictEqual(k.intercept(down('ControlLeft')), false, 'a modifier goes through');
assert.strictEqual(k.intercept(down('ScrollLock')), false, 'a bare lock key with Ctrl held is another shortcut');
assert.strictEqual(k.intercept(up('ControlLeft')), false);

acc = 'Control+Alt+L';
assert.strictEqual(k.intercept(down('KeyL')), false, 'no modifiers: not it');
k.intercept(down('ControlRight'));
k.intercept(down('AltLeft'));
assert.strictEqual(k.intercept(down('KeyL')), 'press', 'either side of a modifier');
k.intercept(up('AltLeft'));
k.intercept(up('ControlRight'));
assert.strictEqual(k.intercept(up('KeyL')), true);

acc = 'Command+L'; // a Mac accelerator: Command is the Meta key
k.intercept(down('MetaLeft'));
assert.strictEqual(k.intercept(down('KeyL')), 'press');
k.reset();
assert.strictEqual(k.intercept(up('KeyL')), false, 'reset forgets what was used up');
assert.strictEqual(k.intercept(down('KeyL')), false, 'and what was held');

acc = 'F14'; // a Mac: a PC keyboard's Scroll Lock is its F14
k.reset();
assert.strictEqual(k.intercept(down('ScrollLock')), 'press');
k.intercept(up('ScrollLock'));
assert.strictEqual(k.intercept(down('F14')), 'press');
k.intercept(up('F14'));
acc = 'Scrolllock'; // and a Mac keyboard's F14 is Scroll Lock on a PC
assert.strictEqual(k.intercept(down('F14')), 'press');
k.intercept(up('F14'));

acc = '';
assert.strictEqual(k.intercept(down('ScrollLock')), false, 'no lock key: nothing is used up');

// GlobalLockKey: registers the wanted key once, drops it for '', says why a key couldn't be had.
const reg = new Set();
const gs = { register: (a, fn) => (a === 'Taken' ? false : (reg.add(a), typeof fn === 'function')), unregister: (a) => reg.delete(a) };
const g = new CL.GlobalLockKey(gs, () => {});
g.sync('Scrolllock');
assert.deepStrictEqual([...reg], ['Scrolllock']);
g.sync('Scrolllock');
assert.deepStrictEqual([...reg], ['Scrolllock'], 'the same key: nothing changes');
g.sync('F14');
assert.deepStrictEqual([...reg], ['F14'], 'another key: the old one is given back');
g.sync('');
assert.deepStrictEqual([...reg], [], 'disconnected: given back');
g.sync('Taken');
assert.match(g.error, /Couldn’t use Taken/);
g.sync('');
assert.strictEqual(g.error, '');

console.log('cursorlock: passed');
