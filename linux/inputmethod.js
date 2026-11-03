// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Input methods across computers on Linux (shared/keylayout.js): keys typed with one (Japanese,
// Pinyin, Korean) type a–z and the input method composes them, so the language goes along with the
// keys (keyLayout's im) and the other side switches to its own input method of that language while
// driven (selectInput), back to what it had after. Here they are Fcitx 5's or IBus's, through their own
// command line (fcitx5-remote, ibus engine), on X11 and Wayland alike; the helper doesn't see them.
//   wrap(request, send) -> {request, send} for KeyLayout: the helper's keyLayout with im and
//                         selectInput added, selectInput done here
'use strict';

const { execFile } = require('child_process');

// Engines by language (Fcitx 5 and IBus names): the first one enabled here is taken.
const ENGINES = [
  ['ja', /^(mozc(-jp)?|anthy|kkc|skk)$/],
  ['zh-Hant', /^(chewing|new-chewing|cangjie\d*|table:cangjie\d*|quick\d*|table:quick\d*|zhuyin|bopomofo|mcbopomofo)$/],
  ['zh-Hans', /^(pinyin|libpinyin|shuangpin|sunpinyin|googlepinyin|rime|wbx|wubi\w*|table:wubi\w*)$/],
  ['ko', /^hangul$/],
];
const langOf = (engine) => {
  const e = ENGINES.find(([, re]) => re.test(engine || ''));
  return e ? e[0] : null;
};

// A switch may wait for the engine to load the first time (libpinyin: over a second).
function run(cmd, args) {
  return new Promise((resolve) => {
    execFile(cmd, args, { timeout: args.length > 1 ? 4000 : 1000 }, (err, out) => resolve(err ? null : String(out).trim()));
  });
}

class InputMethods {
  constructor(exec = run) {
    this.exec = exec;
    this.kind = undefined; // 'fcitx5' | 'ibus' | null (none running), found on first use
    this.before = null; // {engine, active}: in use before select(), for restore()
  }

  async _kind() {
    if (this.kind !== undefined) return this.kind;
    if (await this.exec('fcitx5-remote', ['-n'])) this.kind = 'fcitx5';
    else if (await this.exec('ibus', ['engine'])) this.kind = 'ibus';
    else this.kind = null;
    return this.kind;
  }

  // {engine, active}: the engine in use and whether it composes (Fcitx 5's 2; IBus: always).
  async _now() {
    const kind = await this._kind();
    if (kind === 'fcitx5') {
      const engine = await this.exec('fcitx5-remote', ['-n']);
      return { engine, active: (await this.exec('fcitx5-remote', [])) === '2' };
    }
    if (kind === 'ibus') return { engine: await this.exec('ibus', ['engine']), active: true };
    return null;
  }

  async available() {
    return !!(await this._kind());
  }

  // The language of the input method typed with here, or null.
  async current() {
    const now = await this._now();
    return now && now.active ? langOf(now.engine) : null;
  }

  // The engines enabled here, in their order.
  async _enabled() {
    if (this.kind === 'ibus') {
      // GNOME keeps them in its input sources ([('xkb', 'us'), ('ibus', 'mozc-jp')]), elsewhere IBus's own.
      const gnome = (await this.exec('gsettings', ['get', 'org.gnome.desktop.input-sources', 'sources'])) || '';
      const names = [...gnome.matchAll(/\('ibus',\s*'([^']+)'\)/g)].map((m) => m[1]);
      const cfg = await this.exec('ibus', ['read-config']);
      const m = cfg && /preload-engines:\s*\[([^\]]*)\]/.exec(cfg);
      if (m) names.push(...m[1].split(',').map((s) => s.trim().replace(/^'|'$/g, '')).filter(Boolean));
      return [...new Set([...names, 'mozc-jp', 'anthy', 'libpinyin', 'pinyin', 'chewing', 'hangul'])];
    }
    // Fcitx 5's enabled ones are in its profile; trying each candidate name is simpler and as good.
    return ['mozc', 'anthy', 'kkc', 'skk', 'chewing', 'cangjie', 'quick', 'zhuyin', 'pinyin', 'shuangpin', 'rime', 'wbx', 'hangul'];
  }

  async select(lang) {
    const now = await this._now();
    if (!now) return false;
    if (now.active && langOf(now.engine) === lang) return true;
    for (const engine of await this._enabled()) {
      if (langOf(engine) !== lang) continue;
      if (this.kind === 'fcitx5') {
        await this.exec('fcitx5-remote', ['-s', engine]);
        if ((await this.exec('fcitx5-remote', ['-n'])) !== engine) continue; // not enabled here
        await this.exec('fcitx5-remote', ['-o']);
      } else {
        await this.exec('ibus', ['engine', engine]);
        if ((await this.exec('ibus', ['engine'])) !== engine) continue; // not installed here
      }
      if (!this.before) this.before = now;
      return true;
    }
    return false;
  }

  async restore() {
    const was = this.before;
    this.before = null;
    if (!was || !was.engine) return;
    if (this.kind === 'fcitx5') {
      await this.exec('fcitx5-remote', ['-s', was.engine]);
      if (!was.active) await this.exec('fcitx5-remote', ['-c']);
    } else await this.exec('ibus', ['engine', was.engine]);
  }
}

// KeyLayout's request and send with the input methods here added to the helper's.
function wrap(request, send, im = new InputMethods()) {
  let queue = Promise.resolve(); // one switch at a time, in order
  return {
    request: (cmd, args) => {
      if (cmd !== 'keyLayout') return request(cmd, args);
      return Promise.all([request(cmd, args), im.available(), im.current()]).then(([r, can, lang]) => {
        const out = { ...(r || {}), selectInput: can };
        if (lang) out.im = lang;
        return out;
      });
    },
    send: (cmd, args) => {
      if (cmd !== 'selectInput') return send(cmd, args);
      queue = queue.then(() => (args && args.restore ? im.restore() : im.select(args && args.lang))).catch(() => {});
    },
  };
}

module.exports = { InputMethods, wrap, langOf };
