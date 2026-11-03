// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Clipboard tab: directions, when to sync, privacy and limits, Recent list.
'use strict';

UI.tab({
  id: 'clipboard',
  group: 'Share',
  order: 40,
  title: 'Clipboard',
  desc: 'Copy on one computer, paste on the other.',
  color: 'linear-gradient(#64d2ff, #30b0c7)',
  master: { get: (s) => s.clipboard && s.clipboard.enabled, set: (v) => UI.api.setClipboard({ enabled: v }) },
  badge: (s) => (s.clipboard && s.clipboard.enabled ? 'on' : ''),
  build(body) {
    const { el, api } = UI;
    const set = (partial) => api.setClipboard(partial);

    const peerNote = UI.note('warn', '');
    const skipNote = UI.note('info', '');
    const send = UI.switch((v) => set({ send: v }), { label: 'Send' });
    const receive = UI.switch((v) => set({ receive: v }), { label: 'Receive' });
    const mode = UI.segmented([
      { value: 'switch', label: 'When you switch' },
      { value: 'instant', label: 'Instantly' },
    ], (v) => set({ mode: v }));
    const modeRow = UI.row('Sync', '', [mode]);
    const concealed = UI.switch((v) => set({ skipConcealed: v }), { label: 'Skip passwords' });
    const maxKB = UI.select([
      { value: 16, label: '16 KB' }, { value: 64, label: '64 KB' }, { value: 256, label: '256 KB' },
    ], (v) => set({ maxKB: Number(v) }));
    // Pictures (clipimage.js): screenshots put on the clipboard among them. Not in Multi-device yet.
    const pictures = UI.switch((v) => set({ images: v }), { label: 'Pictures' });
    const picturesRow = UI.row('Pictures', '', [pictures]);
    // Text only: pictures have their own limit (10 MB).
    const maxKBRow = UI.row('Largest text', '', [maxKB]);
    const historySize = UI.select([
      { value: 0, label: 'Off' }, { value: 10, label: '10 items' }, { value: 20, label: '20 items' },
      { value: 50, label: '50 items' }, { value: 100, label: '100 items' },
    ], (v) => set({ historySize: Number(v) }));

    const peerRow = (label, hint, controls) => {
      const r = UI.row(label, hint, controls);
      r.label.dataset.peer = label;
      return r;
    };
    const list = el('div', { class: 'history' });
    const clearBtn = UI.button('Clear', () => api.clearHistory(), 'small');

    body.append(
      peerNote,
      UI.pair(
        UI.dir('m2w', [peerRow('Copy on {me}, paste on {name}', '', [send])]),
        UI.dir('w2m', [peerRow('Copy on {name}, paste on {me}', '', [receive])]),
      ),
      UI.card('Options', [
        modeRow,
        picturesRow,
        UI.row('Skip passwords', 'From password managers', [concealed]),
        maxKBRow,
        UI.row('Recent items', 'Never saved to disk', [historySize]),
      ]),
      skipNote,
      UI.card('Recent', [list], { actions: [clearBtn] }),
    );

    let lastKey = '';
    return (s) => {
      const c = s.clipboard;
      if (!c) return;
      send.set(c.send);
      receive.set(c.receive);
      mode.set(c.mode);
      modeRow.setHint(c.mode === 'instant' ? 'As soon as you copy' : 'When you move to the other computer');
      picturesRow.hidden = !c.imageSupport;
      pictures.set(!!c.images);
      // ⇧⌘4 saves a screenshot to a file; with ⌃ it goes on the clipboard, and over.
      picturesRow.setHint(!c.images ? 'Pictures you copy and screenshots taken with ⌃⇧⌘4, up to 10 MB, both ways'
        : s.client && !c.peerImages ? `Turn on Pictures on ${s.client.name} too (TwinDesk 2.3 or later there)`
          : 'Pictures you copy and screenshots taken with ⌃⇧⌘4, up to 10 MB, both ways. Not over Bluetooth');
      concealed.set(c.skipConcealed);
      maxKB.set(c.maxKB);
      maxKBRow.setHint(c.imageSupport ? 'Longer text isn’t sent. Pictures: up to 10 MB' : 'Longer text isn’t sent');
      historySize.set(c.historySize);
      peerNote.set(s.client && !c.peerAccepts ? `Turned off on ${s.client.name}.` : '');
      skipNote.set(c.enabled && c.lastSkip ? `Last copy not sent: ${c.lastSkip}.` : '');
      clearBtn.disabled = !c.history.length;

      // Rebuild the list only when it changed (or every 15 s for the relative times).
      const key = JSON.stringify(c.history.map((h) => [h.at, h.dir, !!(h.image && h.image.copy)])) + Math.floor(Date.now() / 15000);
      if (key === lastKey) return;
      lastKey = key;
      list.replaceChildren(...(c.history.length
        ? c.history.map((h, i) => el('div', { class: 'hist-item' }, [
          el('span', { class: `hist-dir ${h.dir}`, title: UI.fill(h.dir === 'out' ? 'Sent to {name}' : 'From {name}') }, [UI.icon(h.dir === 'out' ? 'up' : 'down')]),
          h.image && h.image.thumb ? el('img', { class: 'hist-thumb', src: h.image.thumb, alt: '' }) : null,
          el('div', { class: 'hist-text' }, [
            h.image
              ? el('div', { class: 'hist-preview', text: `${h.image.shot ? 'Screenshot' : 'Picture'} · ${h.image.w} × ${h.image.h}` })
              : el('div', { class: 'hist-preview selectable', text: h.text.replace(/\s+/g, ' ').trim() || '(whitespace)' }),
            el('div', { class: 'hint', text: `${UI.fill(h.dir === 'out' ? 'To {name}' : 'From {name}')} · ${UI.ago(h.at)} · ${UI.bytes(h.size)}` }),
          ]),
          !h.image || h.image.copy ? UI.button('Copy', () => api.copyHistory(i), 'small') : null,
        ]))
        : [UI.row(c.historySize ? 'Nothing yet' : 'Recent items are off', '')]));
    };
  },
});
