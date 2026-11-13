// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Files tab: Send files (drop zone + Choose…) and copy/paste files with shortcuts,
// then the two direction cards, then the Folders card — open the other computer's
// shared folder and save from it, or share one of yours and browse it — then
// transfers. Drags across the edge, Monitor 2 drops, network drives in one switch
// each.
'use strict';

UI.tab({
  id: 'files',
  order: 50,
  title: 'Files',
  color: 'linear-gradient(#34d399, #059669)',
  desc: 'Drag files between your computers, or share a folder.',
  badge: (s) => (s.files.dropToMac || s.files.acceptFromMac ? 'on' : ''),
  build(body) {
    const { el, api } = UI;
    const sw = (label, set) => UI.switch(set, { label });
    const mount = sw('{Name}’s folder here', (v) => api.setFiles({ mountMac: v }));
    // What the other computer shares is not a drive: it is a folder this page can
    // list, and each file can be saved into the same place received files go.
    const peerPill = el('span', { class: 'pill muted' });
    const openBtn = UI.button('Open', () => toggleBrowse(), 'small');
    const macDrive = UI.row('Open {name}’s shared folder', '', [peerPill, openBtn, mount.el]);
    const share = sw('{me}’s folder on {name}', (v) => api.setShareOn(v));
    const chooseBtn = UI.button('Choose…', () => api.chooseShareFolder(), 'small');
    const sharePill = el('span', { class: 'pill' });
    const pcShare = UI.row('Share a folder', '', [sharePill, chooseBtn, share.el]);
    // The two folder rows need the full width: inside the direction cards their
    // labels and controls were squeezed onto three lines and overlapped.
    const foldersCard = UI.card('Folders', [macDrive, pcShare]);

    // The other computer's folder, one level at a time.
    const browse = { open: false, path: '', entries: [], error: '', hint: '' };
    let lastListKey = '';
    const list = el('div', { class: 'history' });
    const upBtn = UI.button('Up', () => load(parentPath(browse.path)), 'small');
    const browserBtn = UI.button('In browser', () => api.openMacDrive(), 'small');
    const listHead = UI.row('', '', [upBtn, browserBtn]);
    const folderCard = UI.card('{Name}’s shared folder', [listHead, list]);
    folderCard.hidden = true;

    const joinPath = (a, b) => (a ? a + '/' + b : b);
    const parentPath = (p) => (p.includes('/') ? p.slice(0, p.lastIndexOf('/')) : '');
    const updateHead = () => {
      upBtn.disabled = !browse.path;
      listHead.setHint(browse.hint || browse.path || 'Top of the folder');
      listHead.hint.classList.toggle('bad', !!browse.error);
    };
    const rowFor = (entry) => {
      const path = joinPath(browse.path, entry.name);
      const row = el('div', { class: 'hist-item' }, [
        el('span', { class: 'hist-dir ' + (entry.dir ? 'out' : 'in') }, [UI.icon(entry.dir ? 'folder' : 'doc')]),
        el('div', { class: 'hist-text' }, [
          el('div', { class: 'hist-preview selectable', text: entry.name }),
          el('div', { class: 'hint', text: entry.dir ? 'Folder' : UI.bytes(entry.size) }),
        ]),
      ]);
      row.append(entry.dir
        ? UI.button('Open', () => load(path), 'small')
        : UI.button('Save', () => saveFile(path, entry), 'small'));
      return row;
    };
    const drawList = () => {
      const key = JSON.stringify([browse.path, browse.error, browse.entries.map((e) => [e.name, e.size, e.dir])]);
      if (key !== lastListKey) {
        lastListKey = key;
        if (browse.error) list.replaceChildren(UI.row(browse.error, ''));
        else if (!browse.entries.length) list.replaceChildren(UI.row('This folder is empty', ''));
        else list.replaceChildren(...browse.entries.map(rowFor));
      }
      updateHead();
    };
    const load = async (path) => {
      browse.hint = 'Reading…';
      drawList();
      try {
        const answer = await api.listPeerFolder(path);
        browse.path = answer.path || path;
        browse.entries = Array.isArray(answer.entries) ? answer.entries : [];
        browse.error = '';
        browse.hint = '';
      } catch (e) {
        browse.entries = [];
        browse.error = String((e && e.message) || e);
        browse.hint = '';
      }
      browse.open = true;
      drawList();
      // Small windows put the list below the fold: show it and bring it into view.
      // (A hidden card cannot be scrolled to, so this must happen before the scroll.)
      folderCard.hidden = false;
      folderCard.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    };
    const toggleBrowse = () => {
      if (browse.open) {
        browse.open = false;
        lastListKey = '';
        folderCard.hidden = true;
      } else {
        browse.hint = '';
        browse.open = true;
        folderCard.hidden = false;
        load('');
      }
    };
    const saveFile = async (path, entry) => {
      browse.hint = 'Saving ' + entry.name + '…';
      drawList();
      try {
        const saved = await api.downloadPeerFile(path);
        browse.hint = 'Saved to ' + saved;
      } catch (e) {
        browse.hint = String((e && e.message) || e);
      }
      drawList();
    };

    const pair = UI.dirCards(
      { on: { get: (s) => s.files.acceptFromMac, set: (v) => api.setFiles({ acceptFromMac: v }) }, label: 'Take files from {name} on {me}' },
      { on: { get: (s) => s.files.dropToMac, set: (v) => api.setFiles({ dropToMac: v }) }, label: 'Send files from {me} to {name}' },
    );
    // Send files: drop zone + Choose…
    const zoneText = el('div', { class: 'big', text: 'Drop files or folders here' });
    const zoneHint = el('div', { class: 'hint', text: '' });
    const zone = el('div', { class: 'dropzone' }, [el('div', {}, [zoneText, zoneHint])]);
    // Choose… picks files with the system dialog and sends them right away; the
    // picked paths are only useful to the person sending, so they are not shown.
    const baseName = (p) => String(p || '').split(/[\\/]/).pop();
    const sayNotSent = (path, e) => {
      const why = String((e && e.message) || e || 'could not send');
      api.setFiles({ last: { ok: false, name: baseName(path) || 'files', text: why, at: Date.now() } })
        .catch(() => {});
      console.warn('[Files] Send failed', why);
    };
    const sendBtn = UI.button('Choose…', async () => {
      if (!canSend) return;
      let picked = [];
      try {
        picked = await api.chooseSendFiles();
      } catch (e) {
        sayNotSent('', e);
        return;
      }
      if (!Array.isArray(picked) || !picked.length) return; // cancelled
      try {
        await api.sendFiles(picked);
      } catch (e) {
        sayNotSent(picked[0], e);
      }
    }, 'small');
    const sendRow = UI.row('Send files', '', [sendBtn]);
    const sendBar = el('div', { class: 'bar' }, [el('span')]);
    sendBar.hidden = true;
    sendRow.querySelector('.text').append(sendBar);
    let canSend = false;
    zone.ondragover = (e) => { e.preventDefault(); if (canSend) zone.classList.add('over'); };
    zone.ondragleave = () => zone.classList.remove('over');
    zone.ondrop = (e) => {
      e.preventDefault();
      zone.classList.remove('over');
      // As an array: a FileList reaches the preload as a plain object, which can't be iterated.
      if (canSend) api.sendFiles(api.pathsOf([...e.dataTransfer.files]));
    };
    // Dropping anywhere else in the window must not open the file.
    window.addEventListener('dragover', (e) => e.preventDefault());
    window.addEventListener('drop', (e) => e.preventDefault());

    // Copy and paste files with shortcuts. There is nothing to configure: the
    // gesture is Ctrl+C here and Ctrl+Alt+V on the other computer, read by the
    // side that has the pointer (fileclip.rs). The stored copyKeys/pasteKeys are
    // kept for older files.json files but the hook uses the fixed gesture.
    const keysRow = UI.row('Copy and paste files', '');
    const cancelBtn = UI.button('Cancel', () => api.cancelDrop(), 'small');
    const outRow = UI.row('', '', [cancelBtn]);
    const inRow = UI.row('', '');
    body.append(
      UI.card('', [sendRow, zone], { cls: 'keep-on' }),
      UI.card('', [keysRow], { cls: 'keep-on' }),
      pair.el,
      foldersCard,
      folderCard,
      UI.card('Recent', [outRow, inRow]),
    );
    const mb = (n) => (n / 1048576).toFixed(1) + ' MB';
    return (s) => {
      const f = s.files;
      const m = f.smb;
      const p = UI.peer(s);
      pair.update(s);
      // Send files
      canSend = s.link.state === 'connected' || (s.clients && s.clients.length > 0);
      const multiCount = (s.clients && s.clients.length) || (s.link.state === 'connected' ? 1 : 0);
      zone.classList.toggle('off', !canSend);
      zoneText.textContent = canSend ? (multiCount > 1 ? `Drop files to broadcast to all ${multiCount} connected devices` : 'Drop files or folders here') : 'Connect to send files';
      zoneHint.textContent = !canSend ? '' : (multiCount > 1 ? `Broadcasts files simultaneously to all ${multiCount} connected mesh peers` : (p.phone ? 'They go to ' + p.the + '’s Downloads'
        : 'They go to ' + p.the + '’s Downloads, or the folder chosen there for received files'));
      sendBtn.disabled = !canSend;
      const b = f.batch;
      sendBar.hidden = !b;
      if (b) sendBar.firstChild.style.width = Math.floor((100 * b.sent) / Math.max(1, b.bytes)) + '%';
      sendRow.setHint(b ? (b.kind === 'paste' ? 'Pasting ' : 'Sending ') + b.name + ' to ' + (multiCount > 1 ? `all ${multiCount} devices` : p.the) + (b.files > 1 ? ' · ' + (b.index + 1) + ' of ' + b.files : '') : canSend ? (multiCount > 1 ? `To all ${multiCount} devices` : 'To ' + p.the) : '');
      // Shortcuts
      const ck = f.clip;
      const paste = UI.keysLabel(f.pasteKeys || 'Control+Alt+V');
      keysRow.setHint(ck.error
        || (ck.pasting ? 'Pasting…'
          : ck.incoming ? p.them + ' copied ' + (ck.incoming.name || ck.incoming.items + ' items') + ' · press ' + paste
            : 'Select files and copy them here, then press ' + paste + ' on ' + p.the));
      keysRow.hint.classList.toggle('bad', !!ck.error);
      // ---- the other computer's folder ----
      mount.set(f.mountMac);
      const other = m.peer || {};
      const otherCount = Number(other.count || 0);
      peerPill.hidden = !other.available;
      peerPill.textContent = other.available
        ? (otherCount ? otherCount + (otherCount === 1 ? ' item' : ' items') : 'Empty')
        : '';
      peerPill.className = 'pill ' + (other.available ? 'good' : 'muted');
      // Browsing reads the other computer's folder over the share link; it does not
      // need the folder mounted as a drive here. Requiring the mount switch made
      // Open do nothing at all unless that separate feature was turned on.
      openBtn.disabled = !other.available;
      if (browse.open && !other.available) browse.open = false;
      folderCard.hidden = !(browse.open && other.available);
      macDrive.setHint(m.mountMessage || (!f.mountMac || other.available ? ''
        : m.drive ? 'Drive ' + m.drive
          : m.busy === 'mount' ? 'Connecting…'
            : m.wifi ? 'Over Bluetooth: Open connects it with Wi-Fi Direct'
              : 'Waiting for {name} to share one'));
      // ---- the folder this computer offers ----
      share.set(f.shareOn);
      const busy = m.busy === 'setup' ? 'Setting up…' : m.busy === 'remove' ? 'Removing…' : '';
      sharePill.hidden = !f.shareOn && !busy;
      const sharingCount = Number(m.count || 0);
      sharePill.textContent = busy
        || (!f.shareOn ? 'Off'
          : m.sharing ? 'Sharing' + (sharingCount ? ' ' + sharingCount + (sharingCount === 1 ? ' item' : ' items') : '')
            : 'Not sharing');
      sharePill.className = 'pill ' + (busy ? 'muted' : f.shareOn && m.sharing ? 'good' : f.shareOn ? 'bad' : 'muted');
      chooseBtn.disabled = !!m.busy;
      pcShare.setHint(m.shareMessage
        || (!f.shareOn ? (m.folder ? 'Turned off · ' + m.folder : 'Turned off')
          : m.folder || (m.sharing ? '' : 'Choose… picks the folder to share')));
      if (browse.open) drawList();

      cancelBtn.hidden = !f.current;
      if (f.current) {
        const c = f.current;
        outRow.label.textContent = 'Sending ' + c.name + (c.count > 1 ? ' (' + (c.index + 1) + ' of ' + c.count + ')' : '');
        outRow.setHint(mb(c.sent) + ' of ' + mb(c.size) + (f.queued ? ' · ' + f.queued + ' more waiting' : ''));
      } else {
        outRow.label.textContent = f.last ? (f.last.ok ? 'Sent ' : 'Not sent: ') + (f.last.name || '') : 'Nothing sent';
        outRow.setHint(f.last ? f.last.text + ' · ' + UI.ago(f.last.at) : '');
      }
      const inc = f.incoming;
      const dr = f.drag;
      if (dr.state) {
        inRow.label.textContent = (dr.state === 'copying' ? 'Copying ' : 'Dragging ') + dr.name;
        inRow.setHint(dr.state === 'copying' ? '' : 'Esc cancels');
        return;
      }
      if (dr.last && (!inc.last || dr.last.at > inc.last.at)) {
        inRow.label.textContent = dr.last.ok ? 'Received from {name}' : 'Not received';
        inRow.setHint(dr.last.text + ' · ' + UI.ago(dr.last.at));
        return;
      }
      inRow.label.textContent = inc.receiving ? 'Receiving ' + inc.receiving + ' file' + (inc.receiving > 1 ? 's' : '') + ' from {name}'
        : inc.last ? (inc.last.ok ? 'Received ' : 'Not received: ') + inc.last.name : 'Nothing received';
      inRow.setHint(inc.receiving ? '' : inc.last ? inc.last.text + ' · ' + UI.ago(inc.last.at) : '');
    };
  },
});
