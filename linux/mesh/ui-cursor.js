// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// Cursor tab: share the pointer across the screen edge. The two direction cards and the live state.
// Which device each edge leads to is the layout in the Multi-device tab (ui-multi.js, docs/mesh.md).
// Behaviour: kvm.js and windows-helper.
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

    const speed = UI.slider({ min: 0.25, max: 3, step: 0.05, format: (v) => v.toFixed(2) + '×' }, (v) => set({ speed: v }));
    const reverse = UI.switch((v) => set({ reverseScroll: v }), { label: 'Reverse scrolling' });
    const gestures = UI.switch((v) => set({ gestures: v }), { label: 'Trackpad gestures' });
    const gestureRow = UI.row('Trackpad gestures', 'Pinch to zoom, swipe between desktops, Task View / Mission Control, back and forward — done the native way on {name}. The same on both', [gestures]);
    const ble = UI.switch((v) => set({ ble: v }), { label: 'Over Bluetooth' });
    const bleRow = UI.row('Over Bluetooth', 'When {name} isn’t on this network and you’re connected through Bluetooth (slower). Off: nothing crosses until you’re on Wi-Fi again. The same on both', [ble]);
    const resistance = UI.slider({ min: 0, max: 400, step: 10, format: (v) => (v ? v + ' px' : 'None') }, (v) => set({ resistancePx: v }));
    // A phone as the peer: how the pointer and keys reach it (settings.phone.input, the phone's own toggle too).
    const via = UI.segmented([{ value: 'adb', label: 'ADB' }, { value: 'phone', label: 'Accessibility' }], (v) => api.setPhone({ input: v, inputPicked: true }));
    const viaRow = UI.row('Through', 'ADB: full control. Accessibility: TwinDesk on the phone taps and types itself, over Wi-Fi, Wi-Fi Direct or Bluetooth, no developer options', [via.el]);
    const pair = UI.dirCards(
      { on: { get: (s) => s.kvm.beDriven, set: (v) => set({ beDriven: v }) }, label: 'Use {name}’s mouse and keyboard on {me}', hint: 'Push the pointer past the edge there', rows: [
        UI.row('Pointer speed here', '', [speed]),
        UI.row('Reverse scrolling', '', [reverse]),
      ] },
      { on: { get: (s) => s.kvm.driveOther, set: (v) => set({ driveOther: v }) }, label: 'Use {me}’s mouse and keyboard on {name}', hint: 'Push the pointer past the edge', rows: [
        UI.row('Edge resistance', 'Push a little further to cross', [resistance]),
        viaRow,
      ] },
    );

    const status = UI.inputStatus();
    const lock = UI.cursorLock();

    body.append(
      pair.el,
      lock.el,
      UI.card('Feel', [gestureRow]),
      UI.card('Not on the same network', [bleRow]),
      UI.card('Right now', [status], { cls: 'keep-on' }),
      UI.note('info', 'Which device each edge of the screen leads to: Multi-device › Layout.'),
    );

    function render(s) {
      pair.update(s);
      viaRow.hidden = !UI.peer(s).phone;
      via.set(((s.phone || {}).bridge || {}).input || (((s.phone || {}).settings || {}).input === 'phone' ? 'phone' : 'adb'));
      status.update(s);
      lock.update(s);
      speed.set(s.kvm.speed);
      reverse.set(s.kvm.reverseScroll);
      gestures.set(s.kvm.gestures !== false);
      ble.set(s.kvm.ble !== false);
      resistance.set(s.kvm.resistancePx);
    }
    return render;
  },
});
