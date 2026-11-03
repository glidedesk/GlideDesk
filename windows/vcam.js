// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The virtual camera on Windows 11 (shared/vcam.js's driver): "TwinDesk Camera", a camera every
// app can pick (Windows' MFCreateVirtualCamera). Its media source, windows-helper/bin/
// twindesk-vcam-<arch>.dll, is registered once for the whole PC (one UAC prompt,
// scripts/vcam-install.ps1); twindesk-vcam.exe then adds the camera for this user, and feeds it the
// frames TwinDesk sends (NV12, 1280×720) through shared memory. Windows 10 has no virtual cameras.
const { app } = require('electron');
const { execFile, spawn } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { scriptElevated } = require('./ps');

const BIN = path.join(__dirname, '..', 'windows-helper', 'bin');
const EXE = path.join(BIN, 'twindesk-vcam.exe');
// Frame Server runs Windows' own architecture, whichever this app is.
const ARCH = /arm64/i.test(process.env.PROCESSOR_ARCHITEW6432 || process.env.PROCESSOR_ARCHITECTURE || '') ? 'arm64' : 'x64';
const DLL = path.join(BIN, `twindesk-vcam-${ARCH}.dll`);
const CLSID = '{36D4122A-6D38-4C6A-B622-E9DD756F6C5D}';
const REG = path.join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'reg.exe');
const DIR = path.join(process.env.ProgramData || 'C:\\ProgramData', 'TwinDesk', 'vcam');

function run(file, args, timeout = 20000) {
  return new Promise((resolve) => {
    execFile(file, args, { windowsHide: true, timeout }, (err, stdout) => resolve({ err, out: String(stdout || '') }));
  });
}

async function exe(cmd) {
  const { err, out } = await run(EXE, [cmd]);
  let r = {};
  try { r = JSON.parse(out.trim().split('\n').pop()); } catch {}
  if (err && !r.error) r.error = err.message;
  return r;
}

// The name this build's DLL is registered under (its own hash: an older copy may still be loaded).
function dllName() {
  const hash = crypto.createHash('sha256').update(fs.readFileSync(DLL)).digest('hex').slice(0, 12);
  return `twindesk-vcam-${ARCH}-${hash}.dll`;
}

// Is this build's DLL the registered one?
async function registered() {
  const { out } = await run(REG, ['query', `HKLM\\Software\\Classes\\CLSID\\${CLSID}\\InprocServer32`, '/ve']);
  const m = /REG_SZ\s+(.+)/.exec(out);
  const want = path.join(DIR, dllName());
  return !!m && m[1].trim().toLowerCase() === want.toLowerCase() && fs.existsSync(want);
}

const driver = {
  name: 'TwinDesk Camera',

  async check() {
    if (Number((os.release().split('.')[2]) || 0) < 22000) return { supported: false, message: 'Needs Windows 11: Windows 10 has no virtual cameras' };
    if (!fs.existsSync(EXE) || !fs.existsSync(DLL)) return { supported: false, message: 'Not in this build of TwinDesk' };
    const [reg, st] = await Promise.all([registered(), exe('status')]);
    return { supported: true, installed: reg && st.camera === true };
  },

  // One UAC prompt when this build's DLL isn't the registered one; adding the camera needs none.
  async install() {
    if (!(await registered())) {
      const log = path.join(app.getPath('userData'), 'vcam-result.txt');
      try { fs.unlinkSync(log); } catch {}
      try {
        await scriptElevated('vcam-install', { Dll: DLL, Name: dllName(), Log: log }, 120000);
      } catch (e) {
        throw new Error(/cancel/i.test(e.message) ? 'The administrator prompt was declined' : e.message);
      }
      const r = fs.existsSync(log) ? fs.readFileSync(log, 'utf8').trim() : 'error: the setup didn’t finish';
      try { fs.unlinkSync(log); } catch {}
      if (r !== 'ok') throw new Error(r.replace(/^error: /, ''));
    }
    const st = await exe('status');
    if (st.camera !== true) {
      const r = await exe('add');
      if (!r.ok) throw new Error('Windows didn’t add the camera: ' + (r.error || 'unknown'));
    }
  },

  async remove() {
    const r = await exe('remove');
    if (!r.ok) throw new Error(r.error || 'unknown');
  },

  // The frames' way in: twindesk-vcam.exe feed, each frame on its stdin.
  feed(onExit) {
    const p = spawn(EXE, ['feed'], { windowsHide: true, stdio: ['pipe', 'ignore', 'ignore'] });
    p.on('exit', onExit);
    p.on('error', onExit);
    p.stdin.on('error', () => {});
    return p;
  },
};

module.exports = driver;
