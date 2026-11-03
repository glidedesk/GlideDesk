// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Files pasted into an app (mac/fclip.js, linux/fclip.js; Windows' helper does its own): the paste
// shortcut was pressed with an app in front, not the file manager, so the files arrive in a folder
// of ours, go on the clipboard as files (as the file manager's Copy puts them), and the helper
// presses ⌘V / Ctrl+V there, once the shortcut's own keys are likely let go.
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');

const KEEP_MS = 24 * 3600 * 1000; // an app may read the files late: they stay a day
const SETTLE_MS = 500; // after the files land, before the paste keys

// A new folder for one paste; earlier ones go after a day.
function pasteDir() {
  const root = path.join(os.tmpdir(), 'TwinDesk Paste');
  try {
    for (const d of fs.readdirSync(root)) {
      const p = path.join(root, d);
      if (Date.now() - fs.statSync(p).mtimeMs > KEEP_MS) fs.rmSync(p, { recursive: true, force: true });
    }
  } catch (_) { /* none yet */ }
  const dir = path.join(root, String(Date.now()));
  try {
    fs.mkdirSync(dir, { recursive: true });
    return dir;
  } catch (_) {
    return '';
  }
}

const xml = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const uri = (p) => 'file://' + p.split('/').map(encodeURIComponent).join('/');

// The files on the clipboard as files. Electron 44 (the Mac) has no writeBuffer: a text/uri-list
// ClipboardItem, which macOS also offers as NSFilenamesPboardType.
async function toClipboard(paths) {
  const { clipboard, ClipboardItem } = require('electron');
  if (typeof clipboard.writeBuffer !== 'function') {
    await clipboard.write([new ClipboardItem({ 'text/uri-list': paths.map(uri).join('\r\n') + '\r\n' })]);
  } else if (process.platform === 'darwin') {
    const plist = '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n' +
      `<plist version="1.0"><array>${paths.map((p) => `<string>${xml(p)}</string>`).join('')}</array></plist>`;
    clipboard.writeBuffer('NSFilenamesPboardType', Buffer.from(plist, 'utf8'));
  } else {
    clipboard.writeBuffer('text/uri-list', Buffer.from(paths.map(uri).join('\r\n') + '\r\n', 'utf8'));
  }
}

// On the clipboard, then the paste keys through the helper's injector. Resolves when pressed.
async function pasteInto(helper, paths) {
  const there = paths.filter((p) => fs.existsSync(p));
  if (!there.length) throw new Error('nothing to paste');
  await toClipboard(there);
  const mod = process.platform === 'darwin' ? 'MetaLeft' : 'ControlLeft';
  return new Promise((resolve) => setTimeout(() => {
    for (const [t, code] of [['k.kd', mod], ['k.kd', 'KeyV'], ['k.ku', 'KeyV'], ['k.ku', mod]]) helper.send('inject', { ev: { t, code } });
    resolve();
  }, SETTLE_MS));
}

module.exports = { pasteDir, pasteInto, toClipboard };
