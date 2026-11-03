// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Mirroring and Apps (Mac, PC and Linux), the phone's two pages that were on the Phone page:
//   Mirroring: the phone's screen in a window, what's open here, and how phone windows look and
//   behave (sound, picture, controls, keys).
//   Apps: the phone's apps, each opened in a window of its own.
// Only while a phone is connected; both need wireless debugging (Connection).
'use strict';

(function () {
  const mac = /Mac/.test(navigator.platform);
  const linux = /Linux/.test(navigator.platform);
  const here = mac ? 'this Mac' : linux ? 'this computer' : 'this PC';
  const isPhone = (s) => !!(s && s.phone && s.phone.connected);

  // Wireless debugging isn't ready: one line, with the way to Connection.
  const needAdb = () => {
    const row = UI.row('Wireless debugging isn’t set up', 'Mirroring and phone apps need it. Set it up once on the Connection page.', [UI.button('Set Up', () => UI.go('connection'), 'small primary')]);
    const card = UI.card('', [row]);
    return { el: card, update(s) { const br = (s.phone || {}).bridge || {}; card.hidden = br.state === 'ready'; row.setHint(br.state === 'connecting' ? 'Connecting to the phone’s wireless debugging…' : 'Mirroring and phone apps need it. Set it up once on the Connection page.'); } };
  };

  UI.tab({
    id: 'mirror',
    order: 9,
    title: 'Mirroring',
    icon: 'phone',
    color: 'linear-gradient(#64d2ff, #0a84ff)',
    desc: 'The phone’s screen in a window, with touch, mouse and keyboard.',
    hidden: (s) => !isPhone(s),
    build(body) {
      const { el, api } = UI;
      const adb = needAdb();
      const showBtn = UI.button('Show Phone Screen', () => api.phoneScreen(true), 'primary');
      const showRow = UI.row('Phone screen', 'Opens in its own window. Close it any time; the phone keeps running.', [showBtn]);
      const winList = el('div');
      const winCard = UI.card('Open on ' + here, [winList]);

      const sound = UI.switch((v) => api.setPhone({ sound: v }), { label: 'Phone sound' });
      const quality = UI.select([
        { value: '4', label: 'Data saver · 4 Mb/s' }, { value: '8', label: 'Balanced · 8 Mb/s' },
        { value: '16', label: 'Sharp · 16 Mb/s' }, { value: '24', label: 'Sharpest · 24 Mb/s' },
      ], (v) => api.setPhone({ bitrateMbps: Number(v) }));
      const rows = [
        UI.row('Play phone sound here', 'Android 11 or later', [sound]),
        UI.row('Picture quality', 'For the phone screen and app windows', [quality]),
      ];
      // Windows: where the buttons sit (settings.phone.controls); the panel itself opens and closes
      // from each window's title bar and stays as it was left (settings.phone.panel).
      let controls = null;
      if (!mac && !linux) {
        controls = UI.select([
          { value: 'top', label: 'Title bar' }, { value: 'side', label: 'Side panel' }, { value: 'bottom', label: 'Bottom bar' },
        ], (v) => api.setPhone({ controls: v }));
        rows.push(UI.row('Phone controls', 'Where Back, Home, Recents and the other buttons sit. The panel button in each window’s title bar shows or hides them.', [controls]));
      }
      let enterSends = null, arrowSwipe = null;
      if (!linux) {
        enterSends = UI.switch((v) => api.setPhone({ enterSends: v }), { label: (mac ? 'Return' : 'Enter') + ' sends messages' });
        rows.push(UI.row((mac ? 'Return' : 'Enter') + ' sends messages', 'In WhatsApp, Instagram and Telegram. ' + (mac ? '⇧Return' : 'Shift+Enter') + ' adds a new line.', [enterSends]));
        arrowSwipe = UI.switch((v) => api.setPhone({ arrowSwipe: v }), { label: 'Arrow keys swipe' });
        rows.push(UI.row('Arrow keys swipe', 'Up and down through feeds and Shorts, left and right through pages. Off: arrows move the text cursor.', [arrowSwipe]));
      }
      const optsCard = UI.card('Phone windows', rows);
      const kbRow = UI.row('Mouse and keyboard', 'Push the pointer past the screen edge to use the phone.', [UI.button('Mouse Settings', () => UI.go('cursor'), 'small')]);

      body.append(adb.el, UI.card('', [showRow]), winCard, optsCard, UI.card('', [kbRow]));

      let winKey = '';
      return (s) => {
        adb.update(s);
        const ph = s.phone || {};
        const br = ph.bridge || {};
        showBtn.disabled = br.state !== 'ready';
        const st = ph.settings || {};
        sound.set(!!st.sound);
        quality.set(String(st.bitrateMbps || 8));
        if (controls) controls.set(st.controls || 'top');
        if (enterSends) enterSends.set(st.enterSends !== false);
        if (arrowSwipe) arrowSwipe.set(st.arrowSwipe === true);
        const wins = br.windows || [];
        winCard.hidden = !wins.length;
        const wk = JSON.stringify(wins);
        if (wk !== winKey) {
          winKey = wk;
          winList.replaceChildren(...wins.map((w) => UI.row(w.kind === 'screen' ? 'Phone screen' : w.title, w.kind === 'screen' ? 'Mirror' : w.app, [UI.button('Close', () => api.phoneCloseView(w.id), 'small')])));
        }
      };
    },
  });

  UI.tab({
    id: 'papps',
    order: 7.5,
    title: 'Apps',
    heading: 'Phone apps',
    icon: 'apps',
    color: 'linear-gradient(#5ac8fa, #007aff)',
    desc: 'Open any phone app in its own window.',
    hidden: (s) => !isPhone(s),
    build(body) {
      const { el, api } = UI;
      const adb = needAdb();
      const search = el('input', { type: 'search', placeholder: 'Search apps', autocomplete: 'off', spellcheck: false, attrs: { 'aria-label': 'Search apps' } });
      const count = el('span', { class: 'hint' });
      const grid = el('div', { class: 'app-grid' });
      const note = UI.note('info', '');
      search.oninput = () => { key = ''; UI.state && render(UI.state); };
      const menuWord = mac ? 'add it to the Dock' : linux ? '' : 'pin it to Start';
      body.append(adb.el, el('div', { class: 'papps-bar' }, [search, count]), note, el('div', { class: 'card' }, [grid]));

      let key = '';
      function render(s) {
        adb.update(s);
        const ph = s.phone || {};
        const ready = (ph.bridge || {}).state === 'ready';
        const apps = (UI.phoneAssets && UI.phoneAssets.apps) || [];
        const term = search.value.trim().toLowerCase();
        const shown = term ? apps.filter((a) => a.name.toLowerCase().includes(term)) : apps;
        count.textContent = apps.length ? (term ? `${shown.length} of ${apps.length}` : `${apps.length} apps`) : '';
        note.set(ready && !apps.length ? 'Loading the phone’s apps…' : '');
        const k = JSON.stringify([shown.length, term, ph.assets, ready]);
        if (k === key) return;
        key = k;
        grid.replaceChildren(...shown.slice(0, 400).map((a) => {
          const b = el('button', { class: 'app-tile', title: `Open ${a.name}` + (menuWord ? ` · Right-click to ${menuWord}` : ''), disabled: !ready }, [UI.appIcon(a.icon), el('span', { text: a.name })]);
          b.onclick = () => api.phoneOpenApp({ id: a.id, name: a.name });
          if (api.phoneAppMenu && menuWord) b.oncontextmenu = (e) => { e.preventDefault(); api.phoneAppMenu({ id: a.id, name: a.name }); };
          return b;
        }));
      }
      return render;
    },
  });
})();
