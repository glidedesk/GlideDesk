// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Terminal tab: shells both ways as two direction cards, with any computer. The other computer →
// here: it opens a shell here (Windows' OpenSSH Server, sshd.js). Here → the other computer: a
// shell there (terminal.js), also as `ssh glidedesk` in any terminal.
'use strict';

UI.tab({
  id: 'terminal',
  wifiOnly: true, // over Bluetooth it can't work and doesn't move the session onto Wi-Fi: says so, with Connect with Wi-Fi Direct
  hidden: (s) => UI.isPhone(s), // the Phone page has the phone's screen, sound and apps
  order: 60,
  title: 'Terminal',
  color: 'linear-gradient(#3a3a3c, #1c1c1e)',
  desc: 'Open a terminal on the other computer.',
  // The runtime status and the SSH server block only exist once the terminal feature is wired;
  // until then they are missing from the state and this must still answer (a throwing badge used
  // to abort the whole render, hiding the pairing banner and its Allow / Codes match buttons).
  badge: (s) => {
    const t = (s && s.terminal) || {};
    const srv = t.server || {};
    if (t.status === 'unavailable' || t.status === 'error') return 'warn';
    if (t.status === 'ready' || !!(srv.info && srv.info.enabled)) return 'on';
    return '';
  },
  build(body) {
    const { el, api } = UI;
    // Mac → PC (server here)
    const srvPill = el('span', { class: 'pill' });
    const setupBtn = UI.button('Set up', () => api.setupSshd(), 'small primary');
    const srvRow = UI.row('SSH server', '', [srvPill, setupBtn]);
    const macRow = UI.row('', '');
    // PC → Mac (client here)
    const pill = el('span', { class: 'pill' });
    const openBtn = UI.button('Open Terminal', () => api.openTerminal(), 'small primary');
    const status = UI.row('Shell on {name}', '', [pill, openBtn]);
    const cmdRow = UI.row('In any terminal', '', [el('code', { class: 'mono', text: 'ssh glidedesk' })]);
    const pair = UI.dirCards(
      { on: { get: (s) => s.terminal.allowMac, set: (v) => api.setTerminal({ allowMac: v }) }, label: 'Let {name} open a terminal on {me}', rows: [
        srvRow,
        macRow,
      ] },
      { on: { get: (s) => s.terminal.enabled, set: (v) => api.setTerminal({ enabled: v }) }, label: 'Open a terminal on {name} from {me}', rows: [
        status,
        cmdRow,
      ] },
    );
    body.append(pair.el);
    const TEXT = { off: 'Off', no_mac: 'Waiting for {name}', waiting: 'Asking {name}…', ready: 'Ready', unavailable: 'Not available', error: 'Problem' };
    const SRV = { running: 'Running', stopped: 'Stopped', missing: 'Not installed', unknown: '' };
    return (s) => {
      const t = s.terminal;
      const v = t.server || {}; // absent until the terminal feature is wired
      pair.update(s);
      srvPill.textContent = v.busy ? 'Setting up…' : SRV[v.service] || v.service || '';
      srvPill.className = 'pill ' + (v.service === 'running' ? 'good' : v.busy ? 'muted' : 'bad');
      setupBtn.hidden = v.service === 'running' || v.busy || !t.allowMac;
      srvRow.setHint(v.message || (v.service === 'running' ? '' : 'Asks for administrator rights once'));
      macRow.hidden = !t.allowMac;
      macRow.label.textContent = v.info && v.info.enabled ? 'Ready' : v.macAsked ? 'Not ready yet' : 'Waiting for {name}';
      macRow.setHint(v.macAsked && !(v.info && v.info.enabled) && v.service !== 'running' ? 'Set up the SSH server first' : '');

      pill.textContent = TEXT[t.status] || t.status || '';
      pill.className = 'pill ' + (t.status === 'ready' ? 'good' : t.status === 'unavailable' || t.status === 'error' ? 'bad' : 'muted');
      openBtn.disabled = t.status !== 'ready';
      status.setHint(t.message || (t.status === 'ready' ? 'As ' + t.user : t.status === 'unavailable' ? (UI.peer(s).os === 'mac' ? 'Turn on Remote Login on {name}' : 'Set up the SSH server on {name}') : ''));
      cmdRow.hidden = t.status !== 'ready';
    };
  },
});
