// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Multi-device tab: who is connected right now, and what the mesh does with them.
'use strict';

UI.tab({
  id: 'multi',
  order: 95,
  title: 'Multi-device',
  color: 'linear-gradient(#5e5ce6, #3634a3)',
  desc: 'Several computers and phones connected at once',
  bottom: true,
  build(body) {
    const { el } = UI;
    const statusPill = el('span', { class: 'pill muted', text: 'Waiting' });
    const meshRow = UI.row('Multi-Device Mesh',
      'Every connected computer and phone shares the clipboard, and the mouse and keyboard can move between all of them.',
      [statusPill]
    );
    const list = el('div', {});
    const empty = UI.note('info', 'Connect another computer or phone with "Connect by address" or "Found nearby" on the Overview page.');

    body.append(
      UI.card('Mesh Networking', [meshRow]),
      UI.card('Connected now', [list, empty]),
      MeshIntro.art(),
    );

    let key = '';
    return (s) => {
      const clients = (s && Array.isArray(s.clients)) ? s.clients : [];
      const n = clients.length;
      statusPill.textContent = n === 0 ? 'Waiting'
        : n === 1 ? 'One device'
          : n + ' devices';
      statusPill.className = 'pill ' + (n ? 'good' : 'muted');
      empty.hidden = n > 0;

      const next = JSON.stringify(clients.map((c) => [c.id, c.name, c.os, c.address || '']));
      if (next === key) return;
      key = next;
      list.replaceChildren(...clients.map((c) => UI.row(
        c.name || c.id,
        c.address || '',
        [UI.osIcon(c.os)]
      )));
    };
  },
});
