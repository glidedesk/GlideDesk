// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// General › Received files (Windows, Linux): the folder where files arrive that weren't dropped on
// a folder — Send files from another computer or a phone, the phone's Share, and what's saved from a
// phone's Files and Photos — settings.files.receivedFolder, '' = Downloads. A chosen folder that
// isn't there now (a drive that's unplugged) gives Downloads meanwhile, it isn't made again.
'use strict';

const fs = require('fs');
const path = require('path');

// For a while, another folder: Explorer's Import from phone and Capture from phone (explorer.js)
// put what comes from the phone in the folder that was right-clicked. {dir, until, why} | null
let redirect = null;
function redirectTo(dir, ms, why = '') {
  redirect = dir && path.isAbsolute(dir) ? { dir, until: Date.now() + ms, why } : null;
}
function redirected() {
  if (redirect && (Date.now() > redirect.until || !isDir(redirect.dir))) redirect = null;
  return redirect;
}
const isDir = (d) => { try { return fs.statSync(d).isDirectory(); } catch (_) { return false; } };

// The folder to use now. downloads: app.getPath('downloads').
function receivedDir(settings, downloads) {
  const r = redirected();
  if (r) return r.dir;
  const f = settings && settings.files && settings.files.receivedFolder;
  if (typeof f === 'string' && f && path.isAbsolute(f)) {
    try {
      if (fs.statSync(f).isDirectory()) return f;
    } catch (_) { /* not there now */ }
  }
  return downloads;
}

// settings.js: a saved value as kept ('' = Downloads).
function normalizeFolder(v) {
  return typeof v === 'string' && v.length <= 1000 && path.isAbsolute(v) ? v : '';
}

module.exports = { receivedDir, normalizeFolder, redirectTo, redirected };
