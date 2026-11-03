// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// ReZume tab (Mac, PC and Linux; with a computer or a phone, shared/rezume.js): go on here with
// the tab, document or phone app in use on the other device, and the other way round. Its switch at
// the top turns it on (off until then: nothing is looked at, no permission asked). What goes
// (pages, documents, phone apps, opened here in a window with Mirror apps), which ways, and where
// the other side's shows: a notification, the taskbar (the Dock on the Mac), the tray (menu bar),
// this tab and the Apps tab / Phone page (UI.rezumeCard, the card they show too).
'use strict';

(function () {
  const mac = /Mac/.test(navigator.platform);
  const linux = /Linux/.test(navigator.platform);
  const kindWord = (o) => (o.kind === 'web' ? 'Page' : o.kind === 'doc' ? 'Document' : 'App');
  const hostOf = (u) => { try { return new URL(u).host; } catch (_) { return ''; } };
  const size = (n) => (n >= 1048576 ? (n / 1048576).toFixed(1) + ' MB' : Math.max(1, Math.round(n / 1024)) + ' KB');

  // The other device's offer: its icon, what it is, Open and Dismiss. update(s, always): shown while
  // there is one (always: on the ReZume tab; elsewhere only with "On the Apps and Phone pages").
  UI.rezumeCard = function () {
    const { el, api } = UI;
    const icon = el('span', { class: 'rezume-icon' });
    const open = UI.button('Open', () => api.rezumeDo({ a: 'take' }), 'small primary');
    const dismiss = UI.button('Dismiss', () => api.rezumeDo({ a: 'dismiss' }), 'small');
    const row = UI.row('', '', [open, dismiss]);
    row.prepend(icon);
    const wrap = UI.card('Continue from {name}', [row]);
    wrap.hidden = true;
    let key = '';
    return {
      el: wrap,
      update(s, always) {
        const h = s.rezume;
        const i = h && h.on && h.receive && h.incoming;
        wrap.hidden = !i || (!always && !h.tab);
        if (!i) return;
        const k = JSON.stringify([i.id, i.icon ? i.icon.length : 0]);
        if (k !== key) {
          key = k;
          icon.replaceChildren(UI.appIcon(i.icon));
        }
        row.label.textContent = i.title || i.name || i.appName;
        row.setHint(kindWord(i) + ' · ' + (i.kind === 'web' ? hostOf(i.url) : i.kind === 'doc' ? i.name + (i.size ? ', ' + size(i.size) : '') : (i.appName || i.app) + (h.mirrorReady ? ', in a window here' : ''))
          + (i.appName && i.kind !== 'app' ? ' · ' + i.appName : ''));
      },
    };
  };

  UI.tab({
    id: 'rezume',
    order: 85,
    title: 'ReZume',
    icon: 'rezume',
    color: 'linear-gradient(#64d2ff, #0a84ff)',
    desc: 'Like Handoff on Apple devices: go on here with what you were doing on the other device, and the other way round.',
    badge: (s) => (s.rezume && s.rezume.on && s.rezume.incoming ? '1' : ''),
    master: {
      get: (s) => !!(s.rezume && s.rezume.on),
      set: (v) => UI.api.setReZume({ on: v }),
      disabled: (s) => !s.rezume,
    },
    build(body) {
      const { api } = UI;
      const set = (k) => UI.switch((v) => api.setReZume({ [k]: v }), { label: k });
      const note = UI.note('warn', '');
      const info = UI.note('info', '');
      const offer = UI.rezumeCard();

      const status = UI.row('{name}', '');
      const out = UI.row('Offered to {name}', '');
      const push = UI.button('Send to {name}', () => api.rezumeDo({ a: 'push' }), 'small');
      const outRow = UI.row('Send to {name}', 'Opens there now what was last in front here', [push]);
      const now = UI.card('Now', [status, out, outRow]);

      const sw = {};
      for (const k of ['web', 'video', 'docs', 'apps', 'mirror', 'send', 'receive', 'push', 'notify', 'taskbar', 'tray', 'tab']) sw[k] = set(k);
      const webRow = UI.row('Web pages', 'The tab in front in Chrome, Edge, Firefox, Safari, Brave, Vivaldi, Opera or Arc', [sw.web.el]);
      const videoRow = UI.row('Videos', 'A YouTube video playing here opens there at the same second', [sw.video.el]);
      const docRow = UI.row('Documents', 'The file open in front (Word, Pages, Preview, a text editor…): it comes across and opens here', [sw.docs.el]);
      const appRow = UI.row('Phone apps', 'The app in front on the phone', [sw.apps.el]);
      const mirrorRow = UI.row('Mirror apps', 'A phone app opens here in a window of its own, as it was (wireless debugging)', [sw.mirror.el]);
      const what = UI.card('What goes', [webRow, videoRow, docRow, appRow, mirrorRow]);

      const sendRow = UI.row('Offer what’s in use on {me}', '{name} can go on with it', [sw.send.el]);
      const recvRow = UI.row('Take {name}’s', 'What’s in use there shows here', [sw.receive.el]);
      const pushRow = UI.row('Open what’s sent at once', 'Send to on {name} (its tray, the share sheet’s Open on computer) opens it here without asking', [sw.push.el]);
      const ways = UI.card('Which ways', [sendRow, recvRow, pushRow]);

      const notifyRow = UI.row('Notification', 'A quiet one, replaced by the next', [sw.notify.el]);
      const barRow = UI.row(mac ? 'Dock' : linux ? 'Taskbar or dock' : 'Taskbar', 'A button with the app’s icon: click it to go on', [sw.taskbar.el]);
      const trayRow = UI.row(mac ? 'Menu bar' : 'Tray', 'Continue and Send to in its menu', [sw.tray.el]);
      const tabRow = UI.row('Apps tab and Phone page', 'The card above, there too', [sw.tab.el]);
      const where = UI.card('Where {name}’s shows', [notifyRow, barRow, trayRow, tabRow]);

      body.append(note, offer.el, now, info, what, ways, where);

      return (s) => {
        const h = s.rezume || {};
        for (const k of Object.keys(sw)) {
          sw[k].set(h[k]);
          sw[k].disable(!h.on);
        }
        sw.mirror.disable(!h.on || !h.apps);
        offer.update(s, true);
        const phone = !!h.phone;
        appRow.hidden = mirrorRow.hidden = !phone;
        docRow.setHint(phone ? 'The file open in front here goes to the phone and opens there; the phone has none to send'
          : 'The file open in front (Word, Pages, Preview, a text editor…): it comes across and opens on the other side');
        mirrorRow.setHint(!h.mirrorReady ? 'Needs wireless debugging (Phone page): until then phone apps aren’t offered here'
          : 'A phone app opens here in a window of its own, as it was');
        status.setHint(!h.connected ? 'Not connected'
          : !h.on ? 'Off here'
            : !h.peerKnows ? 'No ReZume there yet: update TwinDesk on {name}'
              : !h.peerOn ? 'Off on {name}: turn it on there too'
                : phone && !h.peerDetects ? (h.mirrorReady ? 'Ready: the phone’s apps through wireless debugging (its pages need TwinDesk’s Accessibility there)' : 'Ready for pages and documents. For what’s in use on the phone, turn on TwinDesk’s Accessibility there, or wireless debugging')
                  : h.incoming ? 'Ready: something to go on with, above' : 'Ready');
        out.hidden = !h.out;
        if (h.out) {
          out.setHint(h.out.title + (h.out.kind === 'web' ? ' · ' + hostOf(h.out.url) : h.out.kind === 'doc' ? ' · ' + h.out.name : ''));
        }
        outRow.hidden = !h.on || !h.connected;
        push.disabled = !h.last;
        outRow.setHint(h.last ? 'Opens there now: ' + h.last.title : 'Nothing in front here to send yet');
        note.set(h.on ? h.why : '');
        info.set(h.on ? (h.waiting ? 'Getting ' + h.waiting + '…' : h.message) : '');
      };
    },
  });
})();
