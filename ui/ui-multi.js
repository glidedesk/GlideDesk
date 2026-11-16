// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Multi-device tab: who is connected right now, and what the mesh does with them.
//
// The page only exists once two devices are connected (`MeshView.visible`): with one, this app is
// the two-device app and there is nothing multi-device to show (specs/001-multi-device-mesh,
// FR-003). Everything it displays is worked out in `shared/mesh-view.js`, which is tested; this
// file only renders it.
'use strict';

UI.tab({
  id: 'multi',
  order: 95,
  title: 'Multi-device',
  color: 'linear-gradient(#5e5ce6, #3634a3)',
  desc: 'Several computers and phones connected at once',
  bottom: true,
  // No switch and nothing to turn on: the mesh activates with the second device and goes quiet
  // again when it leaves. The tab follows the same rule, so it cannot be opened when it would be
  // describing something that is not happening.
  hidden: (s) => !MeshView.visible(s),
  build(body) {
    const { el } = UI;
    const statusPill = el('span', { class: 'pill muted', text: 'Waiting' });
    const meshRow = UI.row('Multi-Device Mesh',
      'Every connected computer and phone shares the clipboard, and the mouse and keyboard can move between all of them.',
      [statusPill]
    );
    const list = el('div', {});
    const empty = UI.note('info', 'Connect another computer or phone with "Connect by address" or "Found nearby" on the Overview page.');
    // Why the last crossing or copy did not happen (`FR-023`). Hidden until there is something to say.
    const refusal = UI.note('warn', '');
    refusal.hidden = true;

    body.append(
      UI.card('Mesh Networking', [meshRow, refusal]),
      UI.card('Connected now', [list, empty]),
      MeshIntro.art(),
    );

    let key = '';
    return (s) => {
      const devices = MeshView.sessions(s);
      const n = devices.length;
      statusPill.textContent = MeshView.summary(s);
      statusPill.className = 'pill ' + (n >= 2 ? 'good' : 'muted');
      empty.hidden = n > 0;
      refusal.set(MeshView.refusalText(s && s.mesh && s.mesh.lastRefusal));

      const next = JSON.stringify(devices.map((d) => [d.id, d.name, d.os, d.address, d.role]));
      if (next === key) return;
      key = next;

      list.replaceChildren(...devices.map((d) => {
        const hint = [MeshView.kind(d.os), d.address, MeshView.roleText(d.role)]
          .filter(Boolean)
          .join(' · ');
        // One link ends; the other devices stay connected. This is the only per-device action the
        // page has, and it is the one a person reaches for when a device should no longer be here.
        const off = UI.button('Disconnect', () => {
          if (!UI.api || !UI.api.disconnectDevice) return;
          UI.api.disconnectDevice(d.id).catch((e) => {
            refusal.set((e && e.message) ? e.message : 'Could not disconnect ' + (d.name || d.id) + '.');
          });
        }, 'small');
        return UI.row(d.name || d.id, hint, [UI.osIcon(d.os), off]);
      }));
    };
  },
});
