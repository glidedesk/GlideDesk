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

    // The arrangement, drawn the same way as the Cursor tab's "Where the other computer is": this
    // computer in the middle, the four sides around it, and each side showing the device that leads
    // off that edge. One control for the whole desk rather than a menu per device, so the shape of
    // the arrangement is the thing you look at.
    const grid = el('div', { class: 'layout-grid arrange' });
    const cells = [];
    for (let i = 0; i < 9; i++) {
      const c = el('div', {});
      cells.push(c);
      grid.append(c);
    }
    const layoutHint = el('div', { class: 'layout-hint' });
    // Why a side could not be given to the device being arranged, in this page's words: a side that
    // already leads somewhere is said to belong to that device, so the click is never a mystery.
    const edgeWarn = UI.note('warn', '');
    edgeWarn.hidden = true;
    const layoutSide = el('div', { class: 'layout-side' }, [layoutHint, edgeWarn]);

    body.append(
      UI.card('Mesh Networking', [meshRow, refusal]),
      UI.card('Connected now', [list, empty]),
      UI.card('Also on this desk', [knownList, knownEmpty]),
      UI.card('Screen edges', [edgeNote, el('div', { class: 'layout' }, [grid, layoutSide])]),
      MeshIntro.art(),
    );

    let key = '';
    // The device the grid is arranging. Picking a row chooses it; a device that leaves the desk
    // cannot stay chosen, so the next one takes its place and the grid always has a subject.
    let picked = '';
    let state = null;

    const paint = (s) => {
      if (s) state = s;
      s = state || {};
      const devices = MeshView.sessions(s);
      const edges = (s && s.layout && s.layout.edges) ? s.layout.edges : null;
      const names = {};
      for (const d of devices) names[d.id] = d.name || d.id;
      if (!devices.some((d) => d.id === picked)) picked = devices.length ? devices[0].id : '';
      const pickedName = names[picked] || picked;
      const pickedSide = picked ? EdgesUI.sideOf(edges, picked) : null;

      const n = devices.length;
      statusPill.textContent = MeshView.summary(s);
      statusPill.className = 'pill ' + (n >= 2 ? 'good' : 'muted');
      empty.hidden = n > 0;
      refusal.set(MeshView.refusalText(s && s.mesh && s.mesh.lastRefusal));
      edgeNote.set(EdgesUI.describe(edges, names));

      const shape = EdgesUI.cells(edges, names);
      cells.forEach((c, i) => {
        const cell = shape[i];
        if (cell.kind === 'gap') {
          c.className = 'gap';
          c.textContent = '';
          c.title = '';
          c.onclick = null;
          return;
        }
        if (cell.kind === 'here') {
          c.className = 'screen pc-screen';
          c.textContent = UI.fill('{me}');
          c.title = 'This computer';
          c.onclick = null;
          return;
        }
        const mine = cell.device && picked && cell.device.toLowerCase() === picked.toLowerCase();
        c.className = 'cell' + (cell.device ? (mine ? ' mine' : ' taken') : '');
        c.textContent = cell.device ? cell.name : cell.label;
        c.title = cell.device
          ? cell.label + ': ' + cell.name + (mine ? ' — click to take it off this side' : '')
          : 'Put ' + (pickedName || 'a device') + ' on the ' + cell.label.toLowerCase() + ' of this screen';
        c.onclick = () => {
          if (!UI.api || !UI.api.assignEdge) return;
          const ask = EdgesUI.attempt(edges, picked, cell.side, names);
          if (!ask) return;
          if (ask.held) {
            edgeWarn.set('The ' + cell.label.toLowerCase() + ' edge already leads to ' + ask.name +
              '. Choose ' + ask.name + ' above to move it, or take it off this side first.');
            return;
          }
          edgeWarn.set('');
          UI.api.assignEdge(ask.call.side, ask.call.deviceId).catch((e) => {
            edgeWarn.set((e && e.message) ? e.message : 'That side could not be assigned.');
          });
        };
      });
      layoutHint.textContent = n === 0
        ? 'No other device to arrange yet.'
        : !picked
          ? 'Choose a device above, then click the side of this screen it sits on.'
          : pickedSide
            ? pickedName + ' is on the ' + EdgesUI.LABELS[pickedSide].toLowerCase() +
              ' of this screen. Click another side to move it, or the same one to take it off.'
            : 'Click a side of this screen to put ' + pickedName + ' there.';

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
        picked,
      ]);
      if (next === key) return;
      key = next;

      list.replaceChildren(...devices.map((d) => {
        const side = EdgesUI.sideOf(edges, d.id);
        const hint = [MeshView.kind(d.os), d.address, MeshView.roleText(d.role),
          side ? EdgesUI.LABELS[side] + ' of this screen' : 'No side yet']
          .filter(Boolean)
          .join(' · ');
        // One link ends; the other devices stay connected.
        const off = UI.button('Disconnect', () => {
          if (!UI.api || !UI.api.disconnectDevice) return;
          UI.api.disconnectDevice(d.id).catch((e) => {
            refusal.set((e && e.message) ? e.message : 'Could not disconnect ' + (d.name || d.id) + '.');
          });
        }, 'small');
        // Choosing a row is choosing the device the grid arranges: the grid shows whose side a
        // click would set, and nothing is sent until a side is clicked.
        const row = UI.row(d.name || d.id, hint, [UI.osIcon(d.os), off], {
          onClick: () => {
            picked = d.id;
            edgeWarn.set('');
            paint();
          },
        });
        row.classList.toggle('picked', d.id === picked);
        if (d.id === picked) row.setAttribute('aria-current', 'true');
        else row.removeAttribute('aria-current');
        return row;
      }));
    };

    return paint;
  },
});
