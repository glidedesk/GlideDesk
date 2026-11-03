// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The address of the tab a browser window shows, from the browser's own files, for ReZume
// (shared/rezume.js) where the window itself doesn't say it (Linux without the accessibility bus,
// Firefox on the Mac, a Windows browser UI Automation can't read). Nothing to allow: the files are
// the person's own.
//   Firefox: its session (sessionstore-backups/recovery.jsonlz4, mozLz4, written every 15 s): the
//   tab whose title the window shows, else the selected tab of the selected window.
//   Chromium browsers (Chrome, Edge, Brave, Vivaldi, Opera, Arc): the most recent page in History
//   whose title the window shows (a copy, read with node:sqlite; the browser keeps it locked).
//   tabOf(app, windowTitle) → {url, title} | null; family(app) → 'firefox' | 'chromium' | ''.
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');

const home = os.homedir();
const plat = process.platform;
const local = process.env.LOCALAPPDATA || path.join(home, 'AppData', 'Local');
const roaming = process.env.APPDATA || path.join(home, 'AppData', 'Roaming');
const macSupport = path.join(home, 'Library', 'Application Support');
const xdgConfig = process.env.XDG_CONFIG_HOME || path.join(home, '.config');

// app (exe name, bundle id or window class) → where its profiles are, by browser.
const BROWSERS = [
  { re: /firefox|org\.mozilla|^navigator$|librewolf|waterfox|zen/i, family: 'firefox',
    roots: { win32: [path.join(roaming, 'Mozilla', 'Firefox', 'Profiles'), path.join(roaming, 'librewolf', 'Profiles'), path.join(roaming, 'zen', 'Profiles')],
      darwin: [path.join(macSupport, 'Firefox', 'Profiles'), path.join(macSupport, 'zen', 'Profiles')],
      linux: [path.join(home, '.mozilla', 'firefox'), path.join(home, 'snap', 'firefox', 'common', '.mozilla', 'firefox'), path.join(home, '.var', 'app', 'org.mozilla.firefox', '.mozilla', 'firefox'), path.join(home, '.librewolf')] } },
  { re: /msedge|microsoft[-.]?edge|edgemac/i, family: 'chromium',
    roots: { win32: [path.join(local, 'Microsoft', 'Edge', 'User Data')], darwin: [path.join(macSupport, 'Microsoft Edge')], linux: [path.join(xdgConfig, 'microsoft-edge')] } },
  { re: /brave/i, family: 'chromium',
    roots: { win32: [path.join(local, 'BraveSoftware', 'Brave-Browser', 'User Data')], darwin: [path.join(macSupport, 'BraveSoftware', 'Brave-Browser')], linux: [path.join(xdgConfig, 'BraveSoftware', 'Brave-Browser'), path.join(home, '.var', 'app', 'com.brave.Browser', 'config', 'BraveSoftware', 'Brave-Browser')] } },
  { re: /vivaldi/i, family: 'chromium',
    roots: { win32: [path.join(local, 'Vivaldi', 'User Data')], darwin: [path.join(macSupport, 'Vivaldi')], linux: [path.join(xdgConfig, 'vivaldi')] } },
  { re: /opera/i, family: 'chromium', single: true,
    roots: { win32: [path.join(roaming, 'Opera Software', 'Opera Stable')], darwin: [path.join(macSupport, 'com.operasoftware.Opera')], linux: [path.join(xdgConfig, 'opera')] } },
  { re: /thebrowser|^arc$/i, family: 'chromium', roots: { darwin: [path.join(macSupport, 'Arc', 'User Data')], win32: [] , linux: [] } },
  { re: /chromium/i, family: 'chromium',
    roots: { win32: [path.join(local, 'Chromium', 'User Data')], darwin: [path.join(macSupport, 'Chromium')], linux: [path.join(xdgConfig, 'chromium'), path.join(home, 'snap', 'chromium', 'common', 'chromium'), path.join(home, '.var', 'app', 'org.chromium.Chromium', 'config', 'chromium')] } },
  { re: /chrome/i, family: 'chromium',
    roots: { win32: [path.join(local, 'Google', 'Chrome', 'User Data')], darwin: [path.join(macSupport, 'Google', 'Chrome')], linux: [path.join(xdgConfig, 'google-chrome'), path.join(home, '.var', 'app', 'com.google.Chrome', 'config', 'google-chrome')] } },
];

const browserOf = (app) => BROWSERS.find((b) => b.re.test(String(app || ''))) || null;
const family = (app) => (browserOf(app) || {}).family || '';

const mtime = (p) => { try { return fs.statSync(p).mtimeMs; } catch (_) { return 0; } };
const dirs = (p) => { try { return fs.readdirSync(p, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => path.join(p, d.name)); } catch (_) { return []; } };

// The most recently written of these files.
function newest(files) {
  let best = null;
  let at = 0;
  for (const f of files) {
    const t = mtime(f);
    if (t > at) { at = t; best = f; }
  }
  return best && { file: best, at };
}

// The window shows this page: "Title", "Title - Google Chrome", "Title — Mozilla Firefox", "Title - Personal - Microsoft Edge".
function titleMatches(windowTitle, title) {
  const w = String(windowTitle || '').trim();
  const t = String(title || '').trim();
  if (!w || !t) return false;
  return w === t || [' - ', ' — ', ' – ', ' · '].some((s) => w.startsWith(t + s));
}

// ---- Firefox ----

// An LZ4 block (Firefox's mozLz4 files) into `size` bytes.
function lz4Block(src, size) {
  const out = Buffer.alloc(size);
  let i = 0;
  let o = 0;
  while (i < src.length) {
    const tok = src[i++];
    let lit = tok >> 4;
    if (lit === 15) for (let b = 255; b === 255 && i < src.length;) { b = src[i++]; lit += b; }
    if (o + lit > size || i + lit > src.length) throw new Error('bad lz4');
    src.copy(out, o, i, i + lit);
    i += lit;
    o += lit;
    if (i >= src.length) break;
    const off = src[i] | (src[i + 1] << 8);
    i += 2;
    let len = tok & 15;
    if (len === 15) for (let b = 255; b === 255 && i < src.length;) { b = src[i++]; len += b; }
    len += 4;
    let from = o - off;
    if (!off || from < 0 || o + len > size) throw new Error('bad lz4');
    while (len--) out[o++] = out[from++];
  }
  return out.subarray(0, o);
}

function mozLz4(buf) {
  if (buf.length < 12 || buf.toString('latin1', 0, 8) !== 'mozLz40\0') throw new Error('not mozLz4');
  return lz4Block(buf.subarray(12), buf.readUInt32LE(8));
}

let ffCache = { file: '', at: 0, session: null };
function firefoxSession(roots) {
  const files = [];
  for (const r of roots) for (const prof of dirs(r)) files.push(path.join(prof, 'sessionstore-backups', 'recovery.jsonlz4'));
  const n = newest(files);
  if (!n) return null;
  if (ffCache.file !== n.file || ffCache.at !== n.at) {
    ffCache = { file: n.file, at: n.at, session: JSON.parse(mozLz4(fs.readFileSync(n.file)).toString('utf8')) };
  }
  return ffCache.session;
}

function firefoxTab(roots, windowTitle) {
  const s = firefoxSession(roots);
  if (!s || !Array.isArray(s.windows)) return null;
  const current = (tab) => (tab && Array.isArray(tab.entries) ? tab.entries[Math.max(0, (tab.index || tab.entries.length) - 1)] : null);
  const selected = s.windows.map((w) => current((w.tabs || [])[Math.max(0, (w.selected || 1) - 1)]));
  const all = s.windows.flatMap((w) => (w.tabs || []).map(current));
  const hit = [...selected, ...all].find((e) => e && titleMatches(windowTitle, e.title))
    || (!windowTitle ? selected[Math.max(0, (s.selectedWindow || 1) - 1)] : null);
  return hit && hit.url ? { url: hit.url, title: hit.title || '' } : null;
}

// ---- Chromium ----

let crCache = { file: '', at: 0, rows: [] };
function chromiumRows(b) {
  const roots = b.roots[plat] || [];
  const files = [];
  for (const r of roots) {
    if (b.single) files.push(path.join(r, 'History'));
    else for (const prof of dirs(r)) files.push(path.join(prof, 'History'));
  }
  const n = newest(files);
  if (!n) return [];
  if (crCache.file === n.file && crCache.at === n.at) return crCache.rows;
  const { DatabaseSync } = require('node:sqlite');
  const tmp = path.join(os.tmpdir(), 'twindesk-history-' + process.pid);
  try {
    fs.copyFileSync(n.file, tmp);
    for (const x of ['-wal', '-journal']) {
      try { fs.copyFileSync(n.file + x, tmp + x); } catch (_) { try { fs.unlinkSync(tmp + x); } catch (__) { /* none */ } }
    }
    const db = new DatabaseSync(tmp, { readOnly: true });
    try {
      crCache = { file: n.file, at: n.at, rows: db.prepare('SELECT url, title FROM urls ORDER BY last_visit_time DESC LIMIT 400').all() };
    } finally { db.close(); }
  } finally {
    for (const x of ['', '-wal', '-journal']) { try { fs.unlinkSync(tmp + x); } catch (_) { /* gone */ } }
  }
  return crCache.rows;
}

function tabOf(app, windowTitle) {
  const b = browserOf(app);
  if (!b) return null;
  try {
    if (b.family === 'firefox') return firefoxTab(b.roots[plat] || [], windowTitle);
    const row = chromiumRows(b).find((r) => titleMatches(windowTitle, r.title));
    return row ? { url: row.url, title: row.title } : null;
  } catch (e) {
    return null;
  }
}

// The title shows this file's name, as a word of its own ("report.odt - LibreOffice Writer", "*notes.txt - Mousepad", "report - Word").
function titleShows(title, name) {
  const t = String(title || '');
  const n = String(name || '');
  if (n.length < 2) return false;
  const i = t.toLowerCase().indexOf(n.toLowerCase());
  if (i < 0) return false;
  const before = i === 0 || ' *[(-—–•|'.includes(t[i - 1]);
  const end = i + n.length;
  return before && (end === t.length || ' *])-—–•|:'.includes(t[end]));
}

// A window's title by part: "Page - Google Chrome" → {page: 'Page', app: 'Google Chrome'},
// "report - Word" → {page: 'report', app: 'Word'}; Edge's profile ("- Personal -") goes too.
function splitTitle(t) {
  const s = String(t || '').replace(/​/g, '').trim();
  const m = /^(.*\S)\s+[-—–]\s+([^-—–]+)$/.exec(s);
  if (!m) return { page: s, app: '' };
  return { page: m[1].replace(/\s+[-—–]\s+(Personal|Work|School|Guest|Profile \d+)$/i, ''), app: m[2].trim() };
}

module.exports = { tabOf, family, titleMatches, titleShows, splitTitle, lz4Block, mozLz4, firefoxTab };
