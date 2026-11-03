// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The desktop's mouse acceleration, for the Wayland helper (linux-helper/src/wl.c): no app may
// read where the pointer is there, so the helper follows it from the mouse's moves with libinput's
// acceleration, and it has to be the one the desktop set, or a quick flick puts the helper's pointer
// at the edge while the real one is still on the screen (and crosses there). Read from GNOME
// (gsettings), KDE (kcminputrc), Hyprland (hyprctl) and Sway (swaymsg); elsewhere libinput's
// default. On X11 the helper reads the real pointer and doesn't need it.
//   get()            -> {accelProfile: 'adaptive' | 'flat', accelSpeed: -1..1} (the last read)
//   watch(onChange)  reads it now and every 30 s; onChange() when it differs (on Wayland only)
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile } = require('child_process');

const DEFAULT = { accelProfile: 'adaptive', accelSpeed: 0 };

const speedOf = (v) => {
  const n = Number(v);
  return Number.isFinite(n) ? Math.max(-1, Math.min(1, n)) : 0;
};
const profileOf = (v) => (/flat/i.test(String(v)) ? 'flat' : 'adaptive');

function run(cmd, args) {
  return new Promise((resolve) => {
    execFile(cmd, args, { timeout: 2000 }, (err, out) => resolve(err ? null : String(out).trim()));
  });
}

// gsettings prints 'flat' (quoted) and 0.25.
async function gnome() {
  const schema = 'org.gnome.desktop.peripherals.mouse';
  const [p, s] = await Promise.all([run('gsettings', ['get', schema, 'accel-profile']), run('gsettings', ['get', schema, 'speed'])]);
  if (p === null && s === null) return null;
  return { accelProfile: profileOf(p), accelSpeed: speedOf(s) };
}

// kcminputrc: a [Libinput][vendor][product][name] group per device, PointerAccelerationProfile
// (libinput's: 1 flat, 2 adaptive) and PointerAcceleration (-1..1). The first that sets either and
// isn't a touchpad: the mouse.
function parseKde(text) {
  let group = '';
  const devs = new Map();
  for (const line of String(text).split('\n')) {
    const g = line.match(/^\s*\[(.*)\]\s*$/);
    if (g) { group = g[1]; continue; }
    const kv = line.match(/^\s*(PointerAccelerationProfile|PointerAcceleration)\s*=\s*(\S+)/);
    if (!kv || !/^Libinput\]/.test(group) || /touchpad/i.test(group)) continue;
    const d = devs.get(group) || { ...DEFAULT };
    if (kv[1] === 'PointerAccelerationProfile') d.accelProfile = Number(kv[2]) === 1 ? 'flat' : 'adaptive';
    else d.accelSpeed = speedOf(kv[2]);
    devs.set(group, d);
  }
  return devs.size ? devs.values().next().value : null;
}

function kde() {
  const dir = process.env.XDG_CONFIG_HOME || path.join(os.homedir(), '.config');
  try {
    return parseKde(fs.readFileSync(path.join(dir, 'kcminputrc'), 'utf8')) || { ...DEFAULT };
  } catch (_) {
    return { ...DEFAULT }; // never changed: libinput's default
  }
}

// hyprctl getoption … -j: {"str": "flat"} and {"float": 0.25}.
async function hyprland() {
  const [p, s] = await Promise.all([run('hyprctl', ['getoption', 'input:accel_profile', '-j']), run('hyprctl', ['getoption', 'input:sensitivity', '-j'])]);
  const j = (t) => { try { return JSON.parse(t); } catch (_) { return {}; } };
  if (p === null && s === null) return null;
  return { accelProfile: profileOf(j(p).str), accelSpeed: speedOf(j(s).float) };
}

// swaymsg -t get_inputs -r: the first pointer's libinput {accel_profile, accel_speed}.
function parseSway(text) {
  let list;
  try { list = JSON.parse(text); } catch (_) { return null; }
  const d = Array.isArray(list) && list.find((i) => i && i.type === 'pointer' && i.libinput);
  return d ? { accelProfile: profileOf(d.libinput.accel_profile), accelSpeed: speedOf(d.libinput.accel_speed) } : null;
}

async function sway() {
  const out = await run('swaymsg', ['-t', 'get_inputs', '-r']);
  return out === null ? null : parseSway(out);
}

async function read(env = process.env) {
  const desk = String(env.XDG_CURRENT_DESKTOP || '').toLowerCase();
  let r = null;
  if (env.HYPRLAND_INSTANCE_SIGNATURE) r = await hyprland();
  else if (env.SWAYSOCK) r = await sway();
  else if (/kde/.test(desk)) r = kde();
  else if (/gnome|unity|budgie|pantheon/.test(desk)) r = await gnome();
  return r || { ...DEFAULT };
}

let cur = { ...DEFAULT };
const get = () => cur;

async function refresh(onChange) {
  const r = await read().catch(() => null);
  if (!r || (r.accelProfile === cur.accelProfile && r.accelSpeed === cur.accelSpeed)) return;
  cur = r;
  onChange();
}

const watchers = [];
function watch(onChange) {
  if (!process.env.WAYLAND_DISPLAY) return;
  watchers.push(onChange);
  const all = () => watchers.forEach((fn) => fn());
  if (watchers.length > 1) return;
  refresh(all);
  setInterval(() => refresh(all), 30000).unref(); // the desktop's mouse settings changed meanwhile
}

module.exports = { get, watch, read, parseKde, parseSway };
