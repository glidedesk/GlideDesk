// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Multi-device tab: who is connected right now, which side of this screen each one is on, and what
// the mesh does with them.
//
// The page only exists once two devices are connected (`MeshView.visible`): with one, this app is
// the two-device app and there is nothing multi-device to show (specs/001-multi-device-mesh,
// FR-003). Everything it displays is worked out in `shared/mesh-view.js` and `shared/edges-ui.js`,
// which are tested; this file only renders them and calls the commands.
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
    // Devices that were paired with one computer and reported here by its roster: listed so the
    // person can see the whole desk, each with a Forget that really removes it (`FR-021`).
    const knownList = el('div', {});
    const knownEmpty = UI.note('info', 'Nobody else on this desk yet.');
    // Why the last crossing or copy did not happen (`FR-023`). Hidden until there is something to say.
    const refusal = UI.note('warn', '');
    refusal.hidden = true;
    // What the edges add up to, so the arrangement can be read at a glance.
    const edgeNote = UI.note('info', EdgesUI.describe(null));

    body.append(
      UI.card('Mesh Networking', [meshRow, refusal]),
      UI.card('Connected now', [list, empty]),
      UI.card('Also on this desk', [knownList, knownEmpty]),
      UI.card('Screen edges', [edgeNote]),
      MeshIntro.art(),
    );

    let key = '';

    return (s) => {
      const devices = MeshView.sessions(s);
      const edges = (s && s.layout && s.layout.edges) ? s.layout.edges : null;
      const names = {};
      for (const d of devices) names[d.id] = d.name || d.id;

      const n = devices.length;
      statusPill.textContent = MeshView.summary(s);
      statusPill.className = 'pill ' + (n >= 2 ? 'good' : 'muted');
      empty.hidden = n > 0;
      refusal.set(MeshView.refusalText(s && s.mesh && s.mesh.lastRefusal));
      edgeNote.set(EdgesUI.describe(edges));

      const elsewhere = MeshView.known(s).filter((d) => d.id !== '' && !names[d.id]);
      knownEmpty.hidden = elsewhere.length > 0;
      knownList.replaceChildren(...elsewhere.map((d) => UI.row(
        d.name || d.id,
        [MeshView.kind(d.os), d.via.length ? 'Through ' + d.via.join(', ') : 'Reported by another computer']
          .filter(Boolean).join(' · '),
        [UI.osIcon(d.os), UI.button('Forget', () => {
          if (!UI.api || !UI.api.forgetDevice) return;
          UI.api.forgetDevice(d.id).catch((e) => {
            refusal.set((e && e.message) ? e.message : 'Could not forget ' + (d.name || d.id) + '.');
          });
        }, 'small')],
      )));

      const next = JSON.stringify([
        devices.map((d) => [d.id, d.name, d.os, d.address, d.role]),
        elsewhere.map((d) => [d.id, d.name, d.os, d.via]),
        EdgesUI.normalize(edges),
      ]);
      if (next === key) return;
      key = next;

      list.replaceChildren(...devices.map((d) => {
        const hint = [MeshView.kind(d.os), d.address, MeshView.roleText(d.role)]
          .filter(Boolean)
          .join(' · ');
        // Which side of this screen the device is on. One side leads to one device, so this is
        // both "where it is" and "how the pointer gets there".
        const picker = el('select', { attrs: { 'aria-label': 'Side of this screen for ' + (d.name || d.id) } },
          EdgesUI.options(edges, d.id, names).map((o) => el('option', {
            value: o.value,
            text: o.holder ? o.label + ' — ' + o.holder : o.label,
            disabled: o.disabled,
            selected: o.selected,
          })));
        // Put the device on a side of this screen, or take it off. The command is the only thing
        // that decides; a refusal (the side already leads to another device) is shown in the words
        // the app sent, never swallowed, and the picker goes back to where the device really is.
        picker.onchange = () => {
          const call = EdgesUI.change(edges, d.id, picker.value);
          if (!call || !UI.api || !UI.api.assignEdge) return;
          UI.api.assignEdge(call.side, call.deviceId).catch((e) => {
            refusal.set((e && e.message) ? e.message : 'That side could not be assigned.');
            picker.value = EdgesUI.sideOf(edges, d.id) || '';
          });
        };
        // One link ends; the other devices stay connected.
        const off = UI.button('Disconnect', () => {
          if (!UI.api || !UI.api.disconnectDevice) return;
          UI.api.disconnectDevice(d.id).catch((e) => {
            refusal.set((e && e.message) ? e.message : 'Could not disconnect ' + (d.name || d.id) + '.');
          });
        }, 'small');
        return UI.row(d.name || d.id, hint, [UI.osIcon(d.os), picker, off]);
      }));
    };
  },
});
