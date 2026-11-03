// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// linux/pointeraccel.js: the desktop's mouse acceleration read from KDE's and Sway's settings.
'use strict';

const assert = require('assert');
const { parseKde, parseSway, read } = require('../../linux/pointeraccel.js');

(async () => {
  // KDE: the mouse's group, not the touchpad's; 1 is flat.
  const rc = [
    '[Libinput][1267][12608][ELAN0501:00 04F3:3140 Touchpad]',
    'PointerAcceleration=0.8',
    '',
    '[Libinput][1133][49271][Logitech USB Optical Mouse]',
    'PointerAccelerationProfile=1',
    'PointerAcceleration=-0.200',
    '',
    '[Mouse]',
    'X11LibInputXAccelProfileFlat=false',
  ].join('\n');
  assert.deepStrictEqual(parseKde(rc), { accelProfile: 'flat', accelSpeed: -0.2 });
  assert.deepStrictEqual(parseKde('[Libinput][1][2][Mouse]\nPointerAcceleration=0.5\n'), { accelProfile: 'adaptive', accelSpeed: 0.5 });
  assert.strictEqual(parseKde('[Mouse]\ncursorTheme=breeze\n'), null, 'nothing set: the default');
  assert.deepStrictEqual(parseKde('[Libinput][1][2][Mouse]\nPointerAcceleration=7\n'), { accelProfile: 'adaptive', accelSpeed: 1 }, 'kept within -1..1');

  // Sway: the first pointer.
  const inputs = JSON.stringify([
    { type: 'keyboard', libinput: {} },
    { type: 'touchpad', libinput: { accel_profile: 'adaptive', accel_speed: 0.9 } },
    { type: 'pointer', libinput: { accel_profile: 'flat', accel_speed: 0.3 } },
  ]);
  assert.deepStrictEqual(parseSway(inputs), { accelProfile: 'flat', accelSpeed: 0.3 });
  assert.strictEqual(parseSway('not json'), null);

  // Another desktop: libinput's default.
  assert.deepStrictEqual(await read({ XDG_CURRENT_DESKTOP: 'XFCE' }), { accelProfile: 'adaptive', accelSpeed: 0 });

  console.log('pointeraccel: ok');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
