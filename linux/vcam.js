// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The virtual camera on Linux (shared/vcam.js's driver): "TwinDesk Camera", a v4l2loopback device
// every app can pick. Installed once (one password prompt, pkexec): the v4l2loopback module from
// the distribution's packages when it isn't there (DKMS: it needs the kernel's headers, and with
// Secure Boot the module has to be signed), loaded with our device now and at every start
// (modules-load.d, modprobe.d). linux-helper/bin/twindesk-vcam writes the frames to it (YUYV),
// all the while the switch is on: with exclusive_caps, apps list it only while something writes.
const { execFile, spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const NAME = 'TwinDesk Camera';
const WRITER = path.join(__dirname, '..', 'linux-helper', 'bin', 'twindesk-vcam');
const SYS = '/sys/class/video4linux';

function run(cmd, args, timeout = 600000) {
  return new Promise((resolve) => {
    execFile(cmd, args, { timeout }, (err, stdout, stderr) => {
      resolve({ ok: !err, code: err ? (typeof err.code === 'number' ? err.code : -1) : 0, out: String(stdout || '').trim(), err: String(stderr || '').trim() });
    });
  });
}

// /dev/videoN of our device, or ''.
function device() {
  try {
    for (const d of fs.readdirSync(SYS)) {
      let name = '';
      try { name = fs.readFileSync(path.join(SYS, d, 'name'), 'utf8').trim(); } catch {}
      if (name === NAME) return '/dev/' + d;
    }
  } catch {}
  return '';
}

// Root, through pkexec. Its own options only when nothing else (OBS) has set v4l2loopback's: then
// a device of ours is added beside the others (v4l2loopback-ctl), for this start of the computer.
const INSTALL = `
set -e
if ! modinfo v4l2loopback >/dev/null 2>&1; then
  if command -v apt-get >/dev/null 2>&1; then DEBIAN_FRONTEND=noninteractive apt-get install -y -q v4l2loopback-dkms >/dev/null
  elif command -v dnf >/dev/null 2>&1; then dnf install -y -q v4l2loopback >/dev/null
  elif command -v zypper >/dev/null 2>&1; then zypper --non-interactive -q install v4l2loopback-kmp-default >/dev/null
  elif command -v pacman >/dev/null 2>&1; then pacman -S --noconfirm --needed v4l2loopback-dkms >/dev/null
  else echo "Install v4l2loopback first (no package manager found)" >&2; exit 3; fi
fi
modinfo v4l2loopback >/dev/null 2>&1 || { echo "v4l2loopback didn't build for this kernel: install its headers (linux-headers-$(uname -r))" >&2; exit 4; }
OTHERS=$(grep -ls 'options v4l2loopback' /etc/modprobe.d/*.conf 2>/dev/null | grep -v twindesk-vcam.conf || true)
if [ -z "$OTHERS" ]; then
  printf 'v4l2loopback\\n' > /etc/modules-load.d/twindesk-vcam.conf
  printf 'options v4l2loopback devices=1 exclusive_caps=1 card_label="${NAME}"\\n' > /etc/modprobe.d/twindesk-vcam.conf
fi
if lsmod | grep -q '^v4l2loopback '; then
  if command -v v4l2loopback-ctl >/dev/null 2>&1 && v4l2loopback-ctl add -n "${NAME}" -x 1 >/dev/null 2>&1; then :
  elif modprobe -r v4l2loopback 2>/dev/null; then modprobe v4l2loopback devices=1 exclusive_caps=1 card_label="${NAME}"
  else echo "v4l2loopback is in use by another app: close it (OBS?) and try again" >&2; exit 5; fi
else
  modprobe v4l2loopback devices=1 exclusive_caps=1 card_label="${NAME}" 2>/tmp/twindesk-vcam.err || {
    grep -qi 'key was rejected\\|required key' /tmp/twindesk-vcam.err && echo "Secure Boot doesn't take the unsigned v4l2loopback module: enroll its key (mokutil) or sign it" >&2 || cat /tmp/twindesk-vcam.err >&2
    exit 6
  }
fi
`;

const driver = {
  name: NAME,
  always: true,

  async check() {
    if (!fs.existsSync(WRITER)) return { supported: false, message: 'Not in this build of TwinDesk' };
    return { supported: true, installed: !!device() };
  },

  async install() {
    const r = await run('pkexec', ['sh', '-c', INSTALL]);
    if (!r.ok) throw new Error(r.code === 126 ? 'The password prompt was closed' : (r.err.split('\n').pop() || 'pkexec failed'));
    for (let i = 0; i < 30 && !device(); i++) await new Promise((res) => setTimeout(res, 100));
    if (!device()) throw new Error('v4l2loopback is loaded but has no “' + NAME + '” device');
  },

  // Our device goes; the module only when it's ours alone.
  async remove() {
    const dev = device();
    const r = await run('pkexec', ['sh', '-c', `
rm -f /etc/modules-load.d/twindesk-vcam.conf /etc/modprobe.d/twindesk-vcam.conf
if [ -n "${dev}" ] && command -v v4l2loopback-ctl >/dev/null 2>&1 && v4l2loopback-ctl delete ${dev} >/dev/null 2>&1; then :
else modprobe -r v4l2loopback; fi
`]);
    if (!r.ok) throw new Error(r.code === 126 ? 'The password prompt was closed' : (r.err.split('\n').pop() || 'pkexec failed'));
  },

  feed(onExit) {
    const p = spawn(WRITER, [device()], { stdio: ['pipe', 'ignore', 'pipe'] });
    p.stderr.on('data', (d) => console.log('[vcam] ' + String(d).trim()));
    p.on('exit', onExit);
    p.on('error', onExit);
    p.stdin.on('error', () => {});
    return p;
  },
};

module.exports = driver;
