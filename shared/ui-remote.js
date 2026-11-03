// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Remote tab (Mac, PC and Linux; with a phone): the phone's Remote page — a touchpad and keyboard,
// or a gamepad whose buttons press keys — moves this computer's pointer, clicks, scrolls and types
// (shared/phone-remote.js). Its switch at the top lets it, on until turned off; it's taken only
// while this computer's own mouse is at home (not on the phone or another computer: Cursor).
// Real gamepad: the gamepad page as a real controller here (an Xbox 360 controller on Windows, a
// uinput gamepad on Linux), its driver installed the first time it's turned on; the Mac says why
// it can't and points to the phone's As a Bluetooth gamepad.
'use strict';

UI.tab({
  id: 'remote',
  group: 'Input',
  order: 28,
  title: 'Remote',
  icon: 'remote',
  color: 'linear-gradient(#bf5af2, #8944ab)',
  desc: 'Use the phone as this computer’s touchpad, keyboard or gamepad.',
  hidden: (s) => !UI.isPhone(s), // only while a phone is connected
  badge: (s) => (s.remote && s.remote.using ? 'on' : ''),
  master: {
    get: (s) => !!(s.remote && s.remote.allow),
    set: (v) => UI.api.setRemote({ allow: v }),
    disabled: (s) => !s.remote,
  },
  build(body) {
    const status = UI.row('Now', '');
    const how = UI.row('On {name}', 'Open TwinDesk › Remote there, then Touchpad and keyboard or Gamepad. Its speed, tap to click and the gamepad’s keys are set there');
    const card = UI.card('{name} as a remote', [status, how]);
    const note = UI.note('warn', '');
    const padSwitch = UI.switch((v) => UI.api.setRemote({ pad: v }), { label: 'Real gamepad' });
    const padRow = UI.row('Real gamepad', '', [padSwitch]);
    const padNote = UI.note('info', '');
    const padCard = UI.card('Gamepad', [padRow, padNote]);
    body.append(note, card, padCard);
    return (s) => {
      const r = s.remote || {};
      const p = r.pad || {};
      padSwitch.set(p.on);
      padSwitch.disable(!p.supported || !!p.busy);
      padRow.setHint(!p.supported ? 'Not on this computer'
        : p.busy ? p.busy
          : !p.on ? 'Off: the gamepad page’s sticks and buttons press keys (WASD, the mouse), as set on {name}'
            : p.live ? `On: {name}’s gamepad page is a game controller here${p.name ? ` (${p.name})` : ''}`
              : 'On: ready when {name} is connected');
      padNote.set(p.message || '');
      status.setHint(!r.allow ? 'Off: {name}’s Remote page can’t move or type here'
        : r.why ? r.why
          : r.using ? 'In use: {name} is moving the pointer or typing here'
            : r.opened ? '{name}’s Remote page is open and ready'
              : 'Ready: open the Remote page on {name}');
      // On, but not taken now (the mouse is on another device).
      note.set(r.allow && r.why ? r.why : '');
    };
  },
});
