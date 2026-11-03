// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Tests for linux/inputmethod.js: Fcitx 5 and IBus input methods by language, with fake command lines.
// Run with: node shared/test/inputmethod.js
'use strict';

const assert = require('assert');
const { InputMethods, wrap, langOf } = require('../../linux/inputmethod.js');

// A fake fcitx5-remote: engines enabled, the one in use and whether it composes.
function fcitx(enabled, engine, active) {
  const st = { engine, active, calls: [] };
  st.exec = async (cmd, args) => {
    st.calls.push([cmd, ...args].join(' '));
    if (cmd !== 'fcitx5-remote') return null;
    if (args[0] === '-n') return st.engine;
    if (args[0] === '-s') { if (enabled.includes(args[1])) st.engine = args[1]; return ''; }
    if (args[0] === '-o') { st.active = true; return ''; }
    if (args[0] === '-c') { st.active = false; return ''; }
    return st.active ? '2' : '1';
  };
  return st;
}

// A fake ibus with GNOME's input sources.
function ibus(sources, engine) {
  const st = { engine, calls: [] };
  st.exec = async (cmd, args) => {
    st.calls.push([cmd, ...args].join(' '));
    if (cmd === 'fcitx5-remote') return null;
    if (cmd === 'gsettings') return `[('xkb', 'us'), ${sources.map((s) => `('ibus', '${s}')`).join(', ')}]`;
    if (cmd === 'ibus' && args[0] === 'read-config') return 'preload-engines: []';
    if (cmd === 'ibus' && args[0] === 'engine') {
      if (!args[1]) return st.engine;
      if (!sources.includes(args[1]) && !args[1].startsWith('xkb:')) return null;
      st.engine = args[1];
      return '';
    }
    return null;
  };
  return st;
}

(async () => {
  assert.strictEqual(langOf('mozc-jp'), 'ja');
  assert.strictEqual(langOf('libpinyin'), 'zh-Hans');
  assert.strictEqual(langOf('chewing'), 'zh-Hant');
  assert.strictEqual(langOf('hangul'), 'ko');
  assert.strictEqual(langOf('keyboard-us'), null);
  assert.strictEqual(langOf('xkb:us::eng'), null);

  // Fcitx 5: Pinyin composing says zh-Hans, in its English state (1) nothing.
  {
    const f = fcitx(['keyboard-us', 'pinyin', 'mozc'], 'pinyin', true);
    assert.strictEqual(await new InputMethods(f.exec).current(), 'zh-Hans');
    f.active = false;
    assert.strictEqual(await new InputMethods(f.exec).current(), null);
  }

  // Fcitx 5: switches to Mozc for ja and back to what was in use (US, not composing).
  {
    const f = fcitx(['keyboard-us', 'mozc'], 'keyboard-us', false);
    const im = new InputMethods(f.exec);
    assert.strictEqual(await im.select('ja'), true);
    assert.deepStrictEqual([f.engine, f.active], ['mozc', true]);
    assert.strictEqual(await im.select('ko'), false, 'no Hangul enabled');
    await im.restore();
    assert.deepStrictEqual([f.engine, f.active], ['keyboard-us', false]);
  }

  // IBus on GNOME: the engines from its input sources.
  {
    const b = ibus(['hangul', 'libpinyin'], 'xkb:us::eng');
    const im = new InputMethods(b.exec);
    assert.strictEqual(await im.current(), null);
    assert.strictEqual(await im.select('zh-Hans'), true);
    assert.strictEqual(b.engine, 'libpinyin');
    assert.strictEqual(await im.current(), 'zh-Hans');
    assert.strictEqual(await im.select('ja'), false, 'none for Japanese');
    await im.restore();
    assert.strictEqual(b.engine, 'xkb:us::eng');
  }

  // Neither running: nothing, and KeyLayout hears selectInput: false.
  {
    const none = new InputMethods(async () => null);
    assert.strictEqual(await none.available(), false);
    const w = wrap(async () => ({ layout: { KeyA: 'a' }, type: true }), () => {}, none);
    assert.deepStrictEqual(await w.request('keyLayout', {}), { layout: { KeyA: 'a' }, type: true, selectInput: false });
  }

  // wrap: keyLayout with im; selectInput done here in order, the rest to the helper.
  {
    const f = fcitx(['keyboard-us', 'hangul'], 'hangul', true);
    const sent = [];
    const w = wrap(async () => ({ layout: {}, type: true }), (cmd, a) => sent.push(cmd), new InputMethods(f.exec));
    assert.deepStrictEqual(await w.request('keyLayout', {}), { layout: {}, type: true, selectInput: true, im: 'ko' });
    f.engine = 'keyboard-us';
    w.send('selectInput', { lang: 'ko' });
    w.send('type', { text: 'x' });
    w.send('selectInput', { restore: true });
    await new Promise((r) => setTimeout(r, 20));
    assert.deepStrictEqual(sent, ['type']);
    assert.strictEqual(f.engine, 'keyboard-us', 'switched to Hangul, then back');
    assert.ok(f.calls.includes('fcitx5-remote -s hangul'));
  }

  console.log('inputmethod: ok');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
