// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The Multi-device switch (docs/multi-device.md): `on` in multi.json, read once at start by main.js.
// With it on, mesh/main.js runs instead of main.js's own code. Changing it restarts TwinDesk, so the
// two never run side by side; the window comes back on the Multi-device tab (settings ui.tab).
const { app } = require('electron');
const store = require('./store');

const FILE = 'multi.json'; // the mesh keeps its own settings here too (mesh/settings.js)

function on() {
  return store.read(FILE, {}).on === true;
}

// Switch it and restart, as Quit does (before-quit gives everything back) and then start again.
function set(value) {
  const m = store.read(FILE, {});
  if ((m.on === true) === !!value) return false;
  store.write(FILE, { ...m, on: !!value });
  app.relaunch({ args: process.argv.slice(1).filter((a) => a !== '--hidden') }); // the window shows
  app.quit();
  return true;
}

module.exports = { on, set };
