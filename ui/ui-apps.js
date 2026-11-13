// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Apps tab. Shows the apps installed on the other computer; clicking one asks that
// computer to open it. The switch below it decides whether the other computer may do
// the same with this one's apps.
'use strict';

UI.tab({
  id: 'apps',
  hidden: (s) => UI.isPhone(s), // the Phone page has the phone's screen, sound and apps
  order: 80,
  title: 'Apps',
  color: 'linear-gradient(#bf5af2, #5e5ce6)',
  desc: 'Open apps on the other computer from here.',
  master: { get: (s) => s.macApps.on || s.pcApps.allow, set: (v) => { UI.api.setMacApps({ on: v }); UI.api.setPcApps({ allow: v }); } },
  build(body) {
    const { el, api } = UI;
    const note = UI.note('warn', '');
    const refresh = UI.button('Refresh', () => api.listApps(), 'small');
    const filter = el('input', { type: 'text', placeholder: 'Search apps', autocomplete: 'off', spellcheck: false, attrs: { 'aria-label': 'Search apps' } });
    const grid = el('div', { class: 'app-grid' });
    const status = UI.row('', '');
    const pcState = UI.row('', '');
    const pair = UI.dirCards(
      { on: { get: (s) => s.macApps.on, set: (v) => api.setMacApps({ on: v }) }, label: 'Use {name}’s apps', hint: 'They open on {name}' },
      { on: { get: (s) => s.pcApps.allow, set: (v) => api.setPcApps({ allow: v }) }, label: 'Let {name} use my apps', hint: 'They open here', rows: [pcState] },
    );
    const listCard = UI.card('Mac apps', [UI.row('Search', '', [filter]), grid, status], { actions: [refresh] });
    body.append(pair.el, note, listCard);
    filter.oninput = () => { key = ''; if (UI.state) render(UI.state); };

    let key = '';
    let asked = false;
    function render(s) {
      const a = s.apps;
      const connected = s.link.state === 'connected';
      pair.update(s); // the two direction switches follow the settings
      listCard.hidden = !s.macApps.on;
      listCard.querySelector('.card-title > span').textContent = UI.peer(s).name + '’s apps';
      if (connected && s.macApps.on && !asked && !a.list.length) { asked = true; api.listApps(); }
      const pa = s.pcApps;
      pcState.hidden = !pa.allow;
      pcState.label.textContent = !connected ? 'Waiting for {name}' : (pa.message || 'None yet');
      pcState.setHint(pa.message || '');
      if (!connected) asked = false;
      note.set(a.message || '');
      note.className = 'note ' + (/ is in Start/.test(a.message) ? 'info' : 'warn');
      refresh.disabled = !connected || a.loading;
      const q = filter.value.trim().toLowerCase();
      const k = JSON.stringify([a.list.length, a.loading, connected, q, !!a.peerPc]);
      if (k === key) return;
      key = k;
      const items = a.list.filter((x) => !q || x.name.toLowerCase().includes(q));
      grid.replaceChildren(...items.map((x) => {
        const b = el('button', { class: 'app-tile', title: 'Open ' + x.name + ' on {name}' }, [
          UI.appIcon(x.icon),
          el('span', { class: 'app-name', text: x.name }),
        ]);
        b.onclick = () => api.launchApp(x.id);
        if (a.peerPc) return b; // apps of another PC: no Start shortcut yet
        // Pin to Start (a shortcut in Start › GlideDesk Apps), on hover or right-click.
        const pin = el('span', { class: 'app-pin', title: 'Pin to Start', attrs: { role: 'button', tabindex: '0', 'aria-label': 'Pin ' + x.name + ' to Start' } }, [UI.icon('pin')]);
        pin.onclick = (e) => { e.stopPropagation(); api.pinApp(x.id); };
        pin.onkeydown = (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); api.pinApp(x.id); } };
        b.oncontextmenu = (e) => { e.preventDefault(); api.pinApp(x.id); };
        b.append(pin);
        return b;
      }));
      status.hidden = items.length > 0;
      status.label.textContent = !connected ? 'Waiting for {name}' : a.loading ? 'Loading…' : q ? 'No app matches' : 'No apps yet';
      status.setHint(!connected || a.loading || q ? '' : 'Click Refresh to ask {name} for its apps.');
    }
    return render;
  },
});
