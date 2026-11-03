// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The Multi-device switch (docs/multi-device.md): `on` in multi.json, read once at start by main.js
// (before store.init, so straight from userData). With it on, mesh/main.js runs instead of main.js's own
// code. Changing it restarts TwinDesk, so the two never run side by side; the control window comes back on
// the Multi-device tab (settings ui.tab).
'use strict';

const fs = require('fs');
const path = require('path');
const { app } = require('electron');

const FILE = 'multi.json'; // the mesh keeps its own settings here too (mesh/store.js)

function read() {
  try {
    return JSON.parse(fs.readFileSync(path.join(app.getPath('userData'), FILE), 'utf8')) || {};
  } catch (e) {
    return {};
  }
}

function on() {
  return read().on === true;
}

// Switch it and restart, as Quit does (before-quit gives everything back) and then start again.
function set(value) {
  const m = read();
  if ((m.on === true) === !!value) return false;
  require('./store').write(FILE, { ...m, on: !!value });
  app.relaunch();
  app.quit();
  return true;
}

module.exports = { on, set };
