// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// A tab's badge() runs on every state push, and a throw there aborts the whole render — which used
// to take the pairing banner (Allow / Codes match) with it. So a badge must answer for a state that
// leaves a feature's runtime block out: the backend's TerminalState has only the settings fields.
'use strict';

const assert = require('assert');

const defs = [];
global.UI = { tab: (d) => defs.push(d) };
require('../../ui-terminal.js');

const terminal = defs.find((d) => d.id === 'terminal');
assert.ok(terminal, 'the Terminal tab registers itself');

// No runtime status, no SSH server block: what main sends today.
const bare = { terminal: { allowMac: true, enabled: true } };
assert.strictEqual(terminal.badge(bare), '', 'nothing running: no badge');
assert.strictEqual(terminal.badge({ terminal: { allowMac: false, enabled: false } }), '');
assert.strictEqual(terminal.badge({}), '', 'no terminal block at all');
assert.strictEqual(terminal.badge(undefined), '', 'no state at all');

// With the runtime block, the same answers as before.
assert.strictEqual(terminal.badge({ terminal: { status: 'ready', server: {} } }), 'on');
assert.strictEqual(terminal.badge({ terminal: { status: 'off', server: { info: { enabled: true } } } }), 'on');
assert.strictEqual(terminal.badge({ terminal: { status: 'error', server: { info: { enabled: true } } } }), 'warn');
assert.strictEqual(terminal.badge({ terminal: { status: 'unavailable', server: {} } }), 'warn');
