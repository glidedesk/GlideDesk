// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Cursor tab: share the pointer across the screen edge. Two direction cards, the
// crossing map (shared/kvm-map-ui.js: drag the other computer where it is; either side can change it)
// and the live state below. Behaviour: kvm.js and windows-helper.
'use strict';

UI.tab({
  id: 'cursor',
  order: 20,
  title: 'Mouse',
  color: 'linear-gradient(#ffb340, #ff9500)',
  desc: 'Move the pointer off the edge of the screen to go to the other computer.',
  // One switch: cursor sharing, the same one the hotkey's double-tap and the other
  // computer flip (kvm.link keeps both in step); on also turns the mouse on. Through Bluetooth
  // it is Over Bluetooth (kvm.setShared).
  master: { get: (s) => s.kvm.mouse && s.kvm.linked && !s.kvmState.paused, set: (v) => UI.api.setKvm(v ? { mouse: true, shared: true } : { shared: false }) },
  badge: (s) => (s.kvm.mouse && s.kvm.linked && !s.kvmState.paused ? 'on' : ''),
  build(body) {
    const { el, api } = UI;
    const set = (partial) => api.setKvm(partial);
    // kvm.layout says where the PC is seen from the Mac; the Mac is on the opposite side of the PC.
    const MAC_SIDE = { left: 'right', right: 'left', top: 'bottom', bottom: 'top' };

    const speed = UI.slider({ min: 0.25, max: 3, step: 0.05, format: (v) => v.toFixed(2) + '×' }, (v) => set({ speed: v }));
    const reverse = UI.switch((v) => set({ reverseScroll: v }), { label: 'Reverse scrolling' });
    const gestures = UI.switch((v) => set({ gestures: v }), { label: 'Trackpad gestures' });
    const gestureRow = UI.row('Trackpad gestures', 'Pinch to zoom, swipe between desktops, Task View / Mission Control, back and forward — done the native way on {name}. The same on both', [gestures]);
    const ble = UI.switch((v) => set({ ble: v }), { label: 'Over Bluetooth' });
    const bleRow = UI.row('Over Bluetooth', 'When {name} isn’t on this network and you’re connected through Bluetooth (slower). Off: nothing crosses until you’re on Wi-Fi again. The same on both', [ble]);
    // A phone as the peer: how the pointer and keys reach it (settings.phone.input, the phone's own toggle too).
    const via = UI.segmented([{ value: 'adb', label: 'ADB' }, { value: 'phone', label: 'Accessibility' }], (v) => api.setPhone({ input: v, inputPicked: true }));
    // With a phone: its own card on top, with the way to its wireless debugging (Connection).
    const adbBtn = UI.button('Wireless Debugging', () => UI.go('connection'), 'small');
    const viaRow = UI.row('Control the phone with', '', [via.el, adbBtn]);
    const viaCard = UI.card('Phone', [viaRow]);
    const resistance = UI.slider({ min: 0, max: 400, step: 10, format: (v) => (v ? v + ' px' : 'None') }, (v) => set({ resistancePx: v }));
    const pair = UI.dirCards(
      { on: { get: (s) => s.kvm.beDriven, set: (v) => set({ beDriven: v }) }, label: 'Use {name}’s mouse and keyboard on {me}', hint: 'Push the pointer past the edge there', rows: [
        UI.row('Pointer speed here', '', [speed]),
        UI.row('Reverse scrolling', '', [reverse]),
      ] },
      { on: { get: (s) => s.kvm.driveOther, set: (v) => set({ driveOther: v }) }, label: 'Use {me}’s mouse and keyboard on {name}', hint: 'Push the pointer past the edge', rows: [
        UI.row('Edge resistance', 'Push a little further to cross', [resistance]),
      ] },
    );

    // The crossing map: both computers' monitors; drag the other one where it is on the desk.
    const map = KvmMapUI.create({ onMove: (side, along) => api.moveKvm({ side, along }), onIdentify: () => api.identifyDisplays() });
    const status = UI.inputStatus();
    const lock = UI.cursorLock();

    body.append(
      viaCard,
      pair.el,
      lock.el,
      UI.card('Feel', [gestureRow]),
      UI.card('Not on the same network', [bleRow]),
      UI.card('Right now', [status], { cls: 'keep-on' }),
      el('div', { class: 'card-title', text: 'Where the other computer is' }),
      el('div', { class: 'card layout' }, [map.el]),
    );

    return (s) => {
      const k = s.kvm;
      pair.update(s);
      status.update(s);
      speed.set(k.speed);
      reverse.set(k.reverseScroll);
      gestures.set(k.gestures !== false);
      resistance.set(k.resistancePx);
      ble.set(k.ble !== false);
      lock.update(s);
      viaCard.hidden = !UI.peer(s).phone;
      const br = (s.phone || {}).bridge || {};
      const pset = (s.phone || {}).settings || {};
      via.set(br.input || (pset.input === 'phone' ? 'phone' : 'adb'));
      adbBtn.hidden = br.state === 'ready';
      viaRow.setHint(br.inputMode === 'auto' ? (br.state === 'ready' ? 'Automatic: ADB, wireless debugging is ready' : 'Automatic: Accessibility until wireless debugging is set up')
        : br.input === 'phone' ? 'Accessibility: TwinDesk on the phone taps and types. Works over Wi-Fi, Wi-Fi Direct and Bluetooth, no developer options'
          : 'ADB: full control through wireless debugging' + (br.state === 'ready' ? '' : ' (not set up yet)'));
      // kvm.layout says where this computer is seen from the other one; the other one is opposite.
      const p = UI.peer(s);
      const m = (s.kvmState && s.kvmState.map) || { mine: [], theirs: [], along: null };
      map.update({ mine: m.mine, theirs: m.theirs, side: k.pcSide ? MAC_SIDE[k.pcSide] : null, along: m.along, me: p.me, them: p.Name });
    };
  },
});
