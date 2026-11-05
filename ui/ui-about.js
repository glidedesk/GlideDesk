// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// About tab (same cards on all three apps): the app icon and version, update checks, this
// computer, the app's build, the maker and the licence. Facts come from shared/about.js.
'use strict';

function formatUpdateStatus(u) {
  if (!u) return { statusText: '', hint: '', checkDisabled: false, getHidden: true, getDisabled: false, releaseHidden: true };
  let statusText = '';
  let hint = '';
  let checkDisabled = false;
  let getHidden = !u.available;
  let getDisabled = false;
  let releaseHidden = !u.error;

  if (u.status === 'downloading') {
    statusText = u.statusText || ('Downloading… ' + Math.round(u.progress || 0) + '%');
    hint = 'Downloading ' + (u.download ? u.download.name : 'update') + '…';
    checkDisabled = true;
    getDisabled = true;
    getHidden = false;
  } else if (u.status === 'verifying') {
    statusText = 'Verifying…';
    hint = 'Verifying downloaded update integrity…';
    checkDisabled = true;
    getDisabled = true;
    getHidden = false;
  } else if (u.status === 'installing') {
    statusText = 'Installing…';
    hint = 'Applying update to your system…';
    checkDisabled = true;
    getDisabled = true;
    getHidden = false;
  } else if (u.status === 'restarting') {
    statusText = 'Update installed — restarting';
    hint = 'Relaunching GlideDesk with the new version…';
    checkDisabled = true;
    getHidden = true;
  } else if (u.error) {
    statusText = 'Update failed';
    hint = 'Error: ' + u.error;
    checkDisabled = false;
    getDisabled = false;
    releaseHidden = false;
  } else if (u.checking) {
    statusText = 'Checking…';
    hint = 'Checking GitHub for new releases…';
    checkDisabled = true;
  } else if (u.available) {
    statusText = u.latest + ' is available';
    hint = 'You have ' + u.current + '. ' + (u.download ? 'Download ' + u.download.name : 'Opens the release page');
    getHidden = false;
  } else if (u.noReleases) {
    statusText = '';
    hint = 'Version ' + u.current + '. No releases published yet';
  } else if (u.checkedAt) {
    statusText = 'Up to date';
    hint = 'Version ' + u.current + (u.checkedAt ? ' · checked ' + new Date(u.checkedAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : '');
  }

  return { statusText, hint, checkDisabled, getHidden, getDisabled, releaseHidden };
}

if (typeof UI !== 'undefined' && UI.tab) {
UI.tab({
  id: 'about',
  order: 110,
  title: 'About',
  color: 'linear-gradient(#34d399, #059669)',
  desc: '',
  bottom: true,
  badge: (s) => (s.updates && s.updates.available ? '1' : ''),
  build(body) {
    const { el, api } = UI;
    const link = (label, key) => UI.button(label, () => api.openLink(key), 'small');
    // A card of label → value rows; set({label: text}) fills them and hides the empty ones.
    const facts = (title, labels) => {
      const vals = {};
      const rows = labels.map((label) => {
        vals[label] = el('span', { class: 'value selectable' });
        return UI.row(label, '', [vals[label]]);
      });
      const card = UI.card(title, rows, { cls: 'keep-on' });
      card.set = (map) => labels.forEach((label, i) => {
        const t = map[label];
        vals[label].textContent = t == null ? '' : String(t);
        rows[i].hidden = t == null || t === '';
      });
      return card;
    };
    const nameEl = el('div', { class: 'about-name', text: 'GlideDesk' });
    const verEl = el('div', { class: 'about-ver' });
    const hero = el('div', { class: 'card about-hero keep-on' }, [
      el('div', { class: 'about-icon', attrs: { role: 'img', 'aria-label': 'GlideDesk icon' } }),
      nameEl,
      verEl,
      el('div', { class: 'about-tag', text: 'Use multiple computers as one desk' }),
    ]);

    // Updates: new releases of GlideDesk on GitHub.
    const updStatus = el('span', { class: 'value' });
    const updCheck = UI.button('Check now', () => api.checkUpdates(), 'small');
    const updGet = UI.button('Update now', () => (api.installUpdate ? api.installUpdate() : api.openUpdate()), 'small primary');
    const updRelease = UI.button('Release page', () => api.openUpdate(), 'small');
    updRelease.hidden = true;
    const updRow = UI.row('GlideDesk', '', [updStatus, updGet, updRelease, updCheck]);
    const updAuto = UI.switch((v) => api.setUpdates({ auto: v }), { label: 'Check for updates automatically' });

    const pc = facts('This computer', ['Computer name', 'System', 'Kernel', 'Processor', 'Memory', 'Architecture']);
    const build = facts('This app', ['Version', 'Built for', 'Runtime']);
    const thanksRow = UI.row('Thanks', '');

    body.append(
      hero,
      UI.card('Updates', [
        updRow,
        UI.row('Check automatically', 'Looks for new releases on GitHub every few hours', [updAuto]),
      ], { cls: 'keep-on' }),
      pc,
      build,
      UI.card('Made by', [
        UI.row('GlideDesk Contributors', 'github.com/glidedesk', [link('Profile', 'portfolio')]),
        UI.row('Source code', 'github.com/glidedesk/GlideDesk', [link('GitHub', 'github')]),
        UI.row('All releases', 'Downloads for macOS, Windows and Linux', [link('Releases', 'releases')]),
      ], { cls: 'keep-on' }),
      UI.card('Licence', [
        UI.row('GNU General Public License v3', 'Free and open source. Copyright © 2026 GlideDesk Contributors', [link('Read', 'licence')]),
        thanksRow,
      ], { cls: 'keep-on' }),
    );

    return (s) => {
      const a = s.about || { app: {}, pc: {} };
      nameEl.textContent = a.app.name || 'GlideDesk';
      verEl.textContent = 'Version ' + (a.app.version || s.version || '');
      thanksRow.setHint(a.app.thanks || '');
      thanksRow.hidden = !a.app.thanks;
      pc.set({
        'Computer name': a.pc.host,
        System: a.pc.os,
        Kernel: a.pc.kernel,
        Processor: a.pc.cpu ? a.pc.cpu + (a.pc.cores ? ' · ' + a.pc.cores + ' cores' : '') : '',
        Memory: a.pc.memory,
        Architecture: a.pc.arch,
      });
      build.set({ Version: a.app.version, 'Built for': a.app.arch, Runtime: a.app.runtime || 'Tauri v2 + Rust' });
      const u = s.updates;
      if (u) {
        updAuto.set(u.auto);
        const st = formatUpdateStatus(u);
        updStatus.textContent = st.statusText;
        updRow.setHint(st.hint);
        updCheck.disabled = st.checkDisabled;
        updGet.hidden = st.getHidden;
        updGet.disabled = st.getDisabled;
        updRelease.hidden = st.releaseHidden;
      }
    };
  },
});
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = { formatUpdateStatus };
}
