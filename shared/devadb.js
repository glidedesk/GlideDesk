// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The phone's quick settings Android doesn't let an app switch (Wi-Fi, mobile data, Bluetooth on
// Android 13+, battery saver), and Lock without its Accessibility service: switched from here over
// wireless debugging when it's ready (shared/devctl.js otherwise asks the phone, which opens the
// setting's panel). The phone tells the new state itself (DevCtl.kt watches for it).
//   viaAdb(adb, serial, cmd) → Promise<boolean>: done here.
'use strict';

const QS = {
  wifi: (on) => `svc wifi ${on ? 'enable' : 'disable'}`,
  data: (on) => `svc data ${on ? 'enable' : 'disable'}`,
  // svc bluetooth is Android 13+; the manager command before that.
  bluetooth: (on) => `svc bluetooth ${on ? 'enable' : 'disable'} 2>/dev/null || cmd bluetooth_manager ${on ? 'enable' : 'disable'}`,
  saver: (on) => `cmd power set-mode ${on ? 1 : 0} 2>/dev/null || settings put global low_power ${on ? 1 : 0}`,
  rotate: (on) => `settings put system accelerometer_rotation ${on ? 1 : 0}`,
};

async function viaAdb(adb, serial, cmd) {
  if (!adb || !serial || !cmd) return false;
  let line = '';
  if (cmd.do === 'qs' && QS[cmd.id]) line = QS[cmd.id](!!cmd.on);
  else if (cmd.do === 'lock') line = 'input keyevent 223'; // KEYCODE_SLEEP: the screen off, locked
  if (!line) return false;
  try {
    const r = await adb.shell(serial, line, { timeout: 8000 });
    return !r.code;
  } catch (e) {
    return false;
  }
}

module.exports = { viaAdb };
