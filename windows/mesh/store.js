// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// The mesh's files in the user data folder (docs/multi-device.md, rule 4). Everything goes through
// ../store.js, except the one-to-one app's pairings, which the mesh starts from but never writes:
//   pairings.json  the hosts this PC dials (links.js); the host paired in the one-to-one app
//                  (pairing.json) joins the list
//   paired.json    read and written as paired-mesh.json, the devices that dial this PC (hostpair.js);
//                  each device paired in the one-to-one app (paired.json) joins the list
// Each joins once (multi.json `imported`): one forgotten here stays forgotten, and one paired again in
// the one-to-one app (a new token) joins again.
const store = require('../store');

const MULTI = 'multi.json';
const ONE_TO_ONE = new Set(['pairing.json', 'paired.json']);

function imported() {
  const im = store.read(MULTI, {}).imported || {};
  return { hosts: Array.isArray(im.hosts) ? im.hosts : [], guests: Array.isArray(im.guests) ? im.guests : [] };
}

function remember(kind, key) {
  const m = store.read(MULTI, {});
  const im = imported();
  im[kind] = [...im[kind], key].slice(-50);
  store.write(MULTI, { ...m, imported: im });
}

// The hosts this PC dials: the mesh's list, and the one-to-one app's host when it is new to it.
function hosts(fallback) {
  let list = store.read('pairings.json', null);
  const old = store.read('pairing.json', null);
  if (old && typeof old.token === 'string' && old.token && !imported().hosts.includes(old.token)) {
    list = Array.isArray(list) ? list : [];
    if (!list.some((e) => e && e.macId === old.macId)) list.push(old);
    store.write('pairings.json', list);
    remember('hosts', old.token);
  }
  return Array.isArray(list) ? list : fallback;
}

// The devices that dial this PC: the mesh's list, and the one-to-one app's devices new to it.
function guests(fallback) {
  const mine = store.read('paired-mesh.json', null);
  let list = Array.isArray(mine) ? mine : [];
  let changed = false;
  const seen = imported().guests;
  for (const e of store.read('paired.json', [])) {
    if (!e || typeof e.tokenHash !== 'string' || seen.includes(e.tokenHash)) continue;
    if (!list.some((x) => x && x.id === e.id)) list = [...list, e];
    remember('guests', e.tokenHash);
    changed = true;
  }
  if (changed) store.write('paired-mesh.json', list);
  return Array.isArray(mine) || changed ? list : fallback;
}

function read(name, fallback) {
  if (name === 'pairings.json') return hosts(fallback);
  if (name === 'paired.json') return guests(fallback);
  return store.read(name, fallback);
}

function write(name, obj) {
  store.write(name === 'paired.json' ? 'paired-mesh.json' : name, obj);
}

function remove(name) {
  if (ONE_TO_ONE.has(name)) return; // the one-to-one app's: never
  store.remove(name);
}

module.exports = { read, write, remove };
