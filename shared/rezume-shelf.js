// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Where the other side's ReZume shows on a computer (shared/rezume.js), each as ReZume's settings
// say: a notification (silent, the last one only), and a button on the taskbar with the app's icon
// ("Continue … from {name}") that opens it when clicked. Windows and Linux: a small window of its
// own, minimized, so it sits on the taskbar until it's clicked (restored); the Mac passes its own
// (a Dock icon, mac/dockstub.js). The tray's item and the tabs read rezume.view.
// opts: rezume, cfg() → settings.rezume, peerName(), appIcon (path, when the offer has none),
//   appId (Windows: the button's own taskbar group), dock? {show(o, take, dismiss), hide()}.
'use strict';

const { BrowserWindow, Notification, nativeImage } = require('electron');

const esc = (s) => String(s || '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const what = (o) => (o.kind === 'web' ? hostOf(o.url) : o.kind === 'doc' ? o.name : o.appName || 'App');
const hostOf = (u) => { try { return new URL(u).host; } catch (_) { return ''; } };

class ReZumeShelf {
  constructor(opts) {
    Object.assign(this, opts);
    this.note = null;
    this.win = null;
    this.shown = null; // the offer the taskbar shows
    const h = this.rezume;
    h.on('offer', (o) => this._show(o));
    h.on('gone', () => this._hide());
    h.on('change', () => this._sync());
  }

  _icon(o) {
    try {
      if (o.icon) {
        const img = nativeImage.createFromDataURL(o.icon);
        if (!img.isEmpty()) return img;
      }
    } catch (_) { /* not an image */ }
    return this.appIcon ? nativeImage.createFromPath(this.appIcon) : undefined;
  }

  _show(o) {
    const c = this.cfg();
    const from = this.peerName();
    const title = o.title || what(o);
    if (c.notify && Notification.isSupported()) {
      if (this.note) this.note.close(); // the last one only: every tab there would leave one here
      this.note = new Notification({ title: 'Continue from ' + from, body: title + (what(o) && what(o) !== title ? '\n' + what(o) : ''), icon: this._icon(o), silent: true });
      this.note.on('click', () => this.rezume.incoming && this.rezume.incoming.id === o.id && this.rezume.take());
      this.note.show();
    }
    if (c.taskbar) this._bar(o, title, from);
  }

  _bar(o, title, from) {
    this.shown = o;
    const take = () => { if (this.rezume.incoming && this.rezume.incoming.id === o.id) this.rezume.take(); this._hide(); };
    const dismiss = () => { this.rezume.dismiss(o.id); this._hide(); };
    if (this.dock) return void this.dock.show(o, take, dismiss);
    const label = 'Continue ' + title + ' from ' + from;
    if (this.win && !this.win.isDestroyed()) this.win.destroy();
    const icon = this._icon(o);
    const win = new BrowserWindow({
      width: 360, height: 110, show: false, title: label, icon, resizable: false, maximizable: false, fullscreenable: false,
      skipTaskbar: false, autoHideMenuBar: true, backgroundColor: '#1f1f22',
      webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false, javascript: false },
    });
    this.win = win;
    if (process.platform === 'win32' && this.appId) win.setAppDetails({ appId: this.appId, relaunchDisplayName: 'ReZume' }); // its own button, not in TwinDesk's group
    const img = o.icon ? `<img src="${esc(o.icon)}">` : '';
    win.loadURL('data:text/html;charset=utf-8,' + encodeURIComponent(`<!doctype html><meta charset="utf-8"><title>${esc(label)}</title>
<style>body{margin:0;font:13px system-ui,sans-serif;background:#1f1f22;color:#eee;display:flex;align-items:center;gap:12px;padding:16px;height:78px;box-sizing:border-box}
img{width:44px;height:44px;border-radius:10px}b{display:block;font-size:14px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:270px}span{opacity:.7}</style>
${img}<div><b>${esc(title)}</b><span>From ${esc(from)} · ${esc(what(o))}</span></div>`));
    win.once('ready-to-show', () => !win.isDestroyed() && win.minimize()); // on the taskbar, not in front
    win.on('restore', () => { win.hide(); setImmediate(take); }); // its button clicked (not destroyed inside its own event)
    win.on('close', (e) => { // closed from the taskbar: dismissed
      if (this.win !== win) return;
      e.preventDefault();
      dismiss();
    });
  }

  _hide() {
    this.shown = null;
    if (this.dock) this.dock.hide();
    const w = this.win;
    this.win = null;
    if (w && !w.isDestroyed()) setImmediate(() => !w.isDestroyed() && w.destroy());
    if (this.note) { this.note.close(); this.note = null; }
  }

  // Settings changed: what's shown follows.
  _sync() {
    const c = this.cfg();
    const i = this.rezume.incoming;
    if (!i) return this.shown && this._hide();
    if (!c.taskbar && this.shown) {
      this.shown = null;
      if (this.dock) this.dock.hide();
      if (this.win && !this.win.isDestroyed()) { const w = this.win; this.win = null; w.destroy(); }
    } else if (c.taskbar && (!this.shown || this.shown.id !== i.id)) this._bar(i, i.title || what(i), this.peerName());
    if (!c.notify && this.note) { this.note.close(); this.note = null; }
  }

  shutdown() {
    this._hide();
  }
}

module.exports = { ReZumeShelf };
