// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Connection › Wireless debugging › Pair with a QR code (Mac, PC and Linux): next to the 6-digit
// code, which stays. Show QR Code starts it here (api.adbQr, shared/adbqr.js) and draws the code for
// the phone's Developer options › Wireless debugging › Pair device with QR code.
// Only where the app has it. UI.adbQr() → {el, update(state, pairable)}; used by shared/ui-adb.js.
'use strict';

UI.adbQr = () => {
  const { el, api } = UI;
  if (!api.adbQr || !window.Qr) return null;
  let open = false;
  let drawn = '';
  const btn = UI.button('Show QR Code', () => { open = !open; api.adbQr(open); paint(UI.state); }, 'small');
  const row = UI.row('Pair with a QR code', 'On the phone: Wireless debugging › Pair device with QR code', [btn]);
  const img = el('div', { class: 'qr-img', attrs: { role: 'img', 'aria-label': 'Wireless debugging QR code' } });
  const note = el('div', { class: 'hint' });
  const box = el('div', { class: 'qr-box' }, [img, el('div', { class: 'qr-text' }, [
    el('strong', { text: 'Scan with the phone' }),
    el('ol', {}, ['Settings › Developer options › Wireless debugging.', 'Tap Pair device with QR code.', 'Scan this code. TwinDesk connects by itself.'].map((t) => el('li', { text: t }))),
    note,
  ])]);
  box.hidden = true;
  const wrap = el('div', {}, [row, box]);

  function paint(s) {
    const q = s && s.adbQr;
    btn.textContent = open ? 'Hide' : 'Show QR Code';
    box.hidden = !open;
    if (!open) return;
    if (q && q.text && q.text !== drawn) {
      drawn = q.text;
      img.replaceChildren(window.Qr.canvas(q.text, 4));
    }
    if (q && q.done) {
      open = false;
      drawn = '';
      box.hidden = true;
      btn.textContent = 'Show QR Code';
      row.setHint(q.done === 'paired' ? 'Paired. Connecting…' : q.done === 'timeout' ? 'The QR code timed out. Show it again to retry.' : q.done.replace(/^failed: /, 'Didn’t pair: '));
      return;
    }
    note.textContent = q && q.text ? 'Waiting for the phone…' : 'Wireless debugging isn’t available here.';
  }

  return {
    el: wrap,
    update(s, pairable) {
      wrap.hidden = !pairable;
      if (!pairable && open) { open = false; api.adbQr(false); }
      paint(s);
    },
  };
};
