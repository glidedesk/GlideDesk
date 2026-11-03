// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Tests for shared/keylayout.js: shortcuts by letter between different layouts.
// Run with: node shared/test/keylayout.js
'use strict';

const assert = require('assert');
const { KeyLayout } = require('../keylayout.js');

// What the helpers say (abridged): US, German (Y and Z swapped), French (AZERTY).
const US = { KeyA: 'a', KeyQ: 'q', KeyW: 'w', KeyY: 'y', KeyZ: 'z', KeyM: 'm', Semicolon: ';', Digit1: '1', KeyC: 'c' };
const DE = { KeyA: 'a', KeyQ: 'q', KeyW: 'w', KeyY: 'z', KeyZ: 'y', KeyM: 'm', Semicolon: 'ö', Digit1: '1', KeyC: 'c' };
const FR = { KeyA: 'q', KeyQ: 'a', KeyW: 'z', KeyZ: 'w', KeyY: 'y', KeyM: ',', Semicolon: 'm', Digit1: '&', KeyC: 'c' };
const RU = { KeyA: 'ф', KeyZ: 'я', KeyC: 'с' };

const layout = (map) => {
  const k = new KeyLayout(() => Promise.resolve({ layout: map }));
  k.set(map);
  return k;
};
const down = (k, code, ch) => k.code(true, code, ch);
const up = (k, code, ch) => k.code(false, code, ch);

(async () => {
  // Sending: the letter a key types here.
  {
    const de = layout(DE);
    assert.deepStrictEqual(de.tag({ t: 'k.kd', code: 'KeyY' }), { t: 'k.kd', code: 'KeyY', ch: 'z' });
    assert.deepStrictEqual(de.tag({ t: 'k.kd', code: 'Semicolon' }), { t: 'k.kd', code: 'Semicolon' }, 'ö: not a–z');
    assert.deepStrictEqual(de.tag({ t: 'k.kd', code: 'ShiftLeft' }), { t: 'k.kd', code: 'ShiftLeft' });
    const ru = layout(RU);
    assert.deepStrictEqual(ru.tag({ t: 'k.kd', code: 'KeyC' }), { t: 'k.kd', code: 'KeyC', tx: 'с' }, 'Cyrillic: no ch, its letter as tx');
    const k = new KeyLayout(() => Promise.reject(new Error('unknown command')));
    await k.refresh();
    assert.deepStrictEqual(k.tag({ t: 'k.kd', code: 'KeyZ' }), { t: 'k.kd', code: 'KeyZ' }, 'an older helper: no letters');
  }

  // A German Mac's Cmd+Z (the Y place, ch z) on a US PC: Ctrl + the Z place.
  {
    const us = layout(US);
    down(us, 'ControlLeft');
    assert.strictEqual(down(us, 'KeyY', 'z'), 'KeyZ', 'undo, not redo');
    assert.strictEqual(down(us, 'KeyY', 'z'), 'KeyZ', 'a repeat: the same key');
    up(us, 'ControlLeft'); // Ctrl up before the letter
    assert.strictEqual(up(us, 'KeyY', 'z'), 'KeyZ', 'its key-up goes to the key that went down');
    // Without Ctrl/Cmd: typing stays in place (the PC's layout types it).
    assert.strictEqual(down(us, 'KeyY', 'z'), 'KeyY');
    assert.strictEqual(up(us, 'KeyY', 'z'), 'KeyY');
  }

  // A US PC's Ctrl+A / Ctrl+Z / Ctrl+W on a French Mac (after the swap: Cmd).
  {
    const fr = layout(FR);
    down(fr, 'MetaLeft');
    assert.strictEqual(down(fr, 'KeyA', 'a'), 'KeyQ', 'select all: the key that types a on AZERTY');
    assert.strictEqual(up(fr, 'KeyA', 'a'), 'KeyQ');
    assert.strictEqual(down(fr, 'KeyZ', 'z'), 'KeyW');
    assert.strictEqual(up(fr, 'KeyZ', 'z'), 'KeyW');
    assert.strictEqual(down(fr, 'KeyM', 'm'), 'Semicolon', 'm is in the ; place');
    up(fr, 'KeyM', 'm');
    assert.strictEqual(down(fr, 'KeyC', 'c'), 'KeyC', 'the same place: unchanged');
    up(fr, 'KeyC', 'c');
    assert.strictEqual(down(fr, 'Digit1'), 'Digit1', 'no letter: unchanged');
    up(fr, 'Digit1');
    up(fr, 'MetaLeft');
  }

  // Same layout both sides: nothing moves. A layout without that letter (Russian): in place.
  {
    const de = layout(DE);
    down(de, 'ControlLeft');
    assert.strictEqual(down(de, 'KeyY', 'z'), 'KeyY');
    up(de, 'KeyY', 'z');
    const ru = layout(RU);
    down(ru, 'ControlLeft');
    assert.strictEqual(down(ru, 'KeyZ', 'z'), 'KeyZ', 'no key types z there: the place');
    up(ru, 'KeyZ', 'z');
    const old = layout(US);
    down(old, 'ControlLeft');
    assert.strictEqual(down(old, 'KeyY'), 'KeyY', 'from an older app (no ch): the place');
  }

  // Another alphabet: a Russian PC (ж in the Semicolon place, ф in the A place) on an English Mac.
  {
    const ru = layout({ ...RU, Semicolon: 'ж', Backquote: 'ё', Period: 'ю', Slash: '.' });
    assert.deepStrictEqual(ru.tag({ t: 'k.kd', code: 'KeyA' }), { t: 'k.kd', code: 'KeyA', tx: 'ф' });
    assert.deepStrictEqual(ru.tag({ t: 'k.ku', code: 'KeyA' }), { t: 'k.ku', code: 'KeyA' }, 'its key-up: nothing more');
    assert.deepStrictEqual(ru.tag({ t: 'k.kd', code: 'Slash' }), { t: 'k.kd', code: 'Slash', tx: '.' }, 'its punctuation too');
    ru.tag({ t: 'k.kd', code: 'ShiftLeft' });
    assert.deepStrictEqual(ru.tag({ t: 'k.kd', code: 'KeyA' }), { t: 'k.kd', code: 'KeyA', tx: 'Ф' }, 'with Shift: capital');
    ru.tag({ t: 'k.ku', code: 'ShiftLeft' });
    ru.tag({ t: 'k.kd', code: 'ControlLeft' });
    assert.deepStrictEqual(ru.tag({ t: 'k.kd', code: 'KeyA' }), { t: 'k.kd', code: 'KeyA' }, 'Ctrl held: a shortcut, the key');
    ru.tag({ t: 'k.ku', code: 'ControlLeft' });

    const typed = [];
    const us = new KeyLayout(() => Promise.resolve({ layout: US, type: true }), (cmd, a) => typed.push([cmd, a.text]));
    await us.refresh();
    assert.strictEqual(down(us, 'KeyA', undefined), 'KeyA');
    up(us, 'KeyA');
    assert.strictEqual(us.code(true, 'KeyA', undefined, 'ф'), null, 'typed, no key');
    assert.strictEqual(us.code(true, 'KeyA', undefined, 'ф'), null, 'a repeat: typed again');
    assert.strictEqual(us.code(false, 'KeyA'), null, 'its key-up: nothing');
    assert.deepStrictEqual(typed, [['type', 'ф'], ['type', 'ф']]);
    assert.strictEqual(us.code(true, 'Semicolon', undefined, 'ж'), null, 'a key typing ; here: typed too');
    up(us, 'Semicolon');
    down(us, 'MetaLeft');
    assert.strictEqual(us.code(true, 'KeyA', undefined, 'ф'), 'KeyA', 'Cmd held: the key');
    up(us, 'KeyA');
    up(us, 'MetaLeft');

    const both = new KeyLayout(() => Promise.resolve({ layout: RU, type: true }), (cmd, a) => typed.push(a.text));
    await both.refresh();
    assert.strictEqual(both.code(true, 'KeyA', undefined, 'ф'), 'KeyA', 'Russian here too: its own layout types it');
    const old = new KeyLayout(() => Promise.resolve({ layout: US }), (cmd, a) => typed.push(a.text));
    await old.refresh();
    assert.strictEqual(old.code(true, 'KeyA', undefined, 'ф'), 'KeyA', 'a helper that can\'t type: the key');
    assert.strictEqual(typed.length, 3);
    us.reset();
  }

  // Sinhala (Wijesekara: ක on KeyU, ඛ with Shift; the vowel sign ා on KeyD; ෂ on the ; key; a Wij 9
  // ligature) on an English Mac, and Shift's own map.
  {
    const SI = { KeyU: 'ක', KeyD: '්', KeyF: 'ා', Semicolon: 'ස', KeyX: 'ං', Slash: 'ෘ', Digit1: '1' };
    const SI_SHIFT = { KeyU: 'ඛ', KeyD: 'ෟ', KeyF: 'ෑ', Semicolon: ':', KeyX: 'ක්‍ෂ', Digit1: '!' };
    const si = new KeyLayout(() => Promise.resolve({ layout: SI, shift: SI_SHIFT, type: true }));
    await si.refresh();
    assert.deepStrictEqual(si.tag({ t: 'k.kd', code: 'KeyU' }), { t: 'k.kd', code: 'KeyU', tx: 'ක' });
    assert.deepStrictEqual(si.tag({ t: 'k.kd', code: 'KeyD' }), { t: 'k.kd', code: 'KeyD', tx: '්' }, 'al-lakuna: a mark');
    assert.deepStrictEqual(si.tag({ t: 'k.kd', code: 'KeyF' }), { t: 'k.kd', code: 'KeyF', tx: 'ා' }, 'a vowel sign');
    assert.deepStrictEqual(si.tag({ t: 'k.kd', code: 'Digit1' }), { t: 'k.kd', code: 'Digit1', tx: '1' }, 'a digit: says it too');
    si.tag({ t: 'k.kd', code: 'ShiftLeft' });
    assert.deepStrictEqual(si.tag({ t: 'k.kd', code: 'KeyU' }), { t: 'k.kd', code: 'KeyU', tx: 'ඛ' }, 'Shift: its own map, no case');
    assert.deepStrictEqual(si.tag({ t: 'k.kd', code: 'KeyX' }), { t: 'k.kd', code: 'KeyX', tx: 'ක්‍ෂ' }, 'a ligature');
    assert.deepStrictEqual(si.tag({ t: 'k.kd', code: 'Semicolon' }), { t: 'k.kd', code: 'Semicolon', tx: ':' }, 'Shift on a Sinhala key: what it types');
    si.tag({ t: 'k.ku', code: 'ShiftLeft' });

    const typed = [];
    const us = new KeyLayout(() => Promise.resolve({ layout: US, shift: { KeyA: 'A', Semicolon: ':', Digit1: '!', Quote: '"' }, type: true }), (cmd, a) => typed.push(a.text));
    await us.refresh();
    assert.strictEqual(us.code(true, 'KeyU', undefined, 'ක'), null);
    us.code(false, 'KeyU');
    assert.strictEqual(us.code(true, 'Semicolon', undefined, 'ස'), null, 'in the ; place');
    us.code(false, 'Semicolon');
    us.code(true, 'ShiftLeft');
    assert.strictEqual(us.code(true, 'Semicolon', undefined, ':'), 'Semicolon', 'Shift+; types : here too: the key');
    us.code(false, 'Semicolon');
    assert.strictEqual(us.code(true, 'KeyX', undefined, 'ක්‍ෂ'), null);
    us.code(false, 'KeyX');
    assert.strictEqual(us.code(true, 'Digit1', undefined, '!'), 'Digit1', 'the same here: the key');
    us.code(false, 'Digit1');
    assert.strictEqual(us.code(true, 'Quote', undefined, ','), null, 'a , in the \' place: typed');
    us.code(false, 'Quote');
    assert.strictEqual(us.code(true, 'KeyX', undefined, 'ක්‍ෂ'), null);
    us.code(false, 'KeyX');
    us.code(false, 'ShiftLeft');
    assert.deepStrictEqual(typed, ['ක', 'ස', 'ක්‍ෂ', ',', 'ක්‍ෂ']);

    const same = new KeyLayout(() => Promise.resolve({ layout: SI, shift: SI_SHIFT, type: true }), (cmd, a) => typed.push(a.text));
    await same.refresh();
    assert.strictEqual(same.code(true, 'KeyU', undefined, 'ක'), 'KeyU', 'Sinhala here too: its own layout types it');
    same.code(false, 'KeyU');
    same.code(true, 'ShiftLeft');
    assert.strictEqual(same.code(true, 'KeyU', undefined, 'ඛ'), 'KeyU', 'and with Shift');
  }

  // Any other script the same way, nothing per language: Hindi (Devanagari-QWERTY: the vowel sign ु
  // on U, ू with Shift) and Arabic (ش on A, ِ with Shift) on an English PC.
  {
    for (const [layout, shift, code, alone, shifted] of [
      [{ KeyU: 'ु', KeyK: 'क' }, { KeyU: 'ू', KeyK: 'ख' }, 'KeyU', 'ु', 'ू'],
      [{ KeyA: 'ش', KeyS: 'س' }, { KeyA: 'ِ', KeyS: 'ٍ' }, 'KeyA', 'ش', 'ِ'],
    ]) {
      const k = new KeyLayout(() => Promise.resolve({ layout, shift, type: true }));
      await k.refresh();
      assert.strictEqual(k.tag({ t: 'k.kd', code }).tx, alone);
      k.tag({ t: 'k.kd', code: 'ShiftRight' });
      assert.strictEqual(k.tag({ t: 'k.kd', code }).tx, shifted);
      const typed = [];
      const us = new KeyLayout(() => Promise.resolve({ layout: US, type: true }), (cmd, a) => typed.push(a.text));
      await us.refresh();
      assert.strictEqual(us.code(true, code, undefined, alone), null);
      us.code(false, code);
      assert.deepStrictEqual(typed, [alone]);
    }
  }

  // Input methods (Japanese, Pinyin, Korean): the keys say the language, the other side switches to
  // its own input method of it while driven and back after.
  {
    const ja = new KeyLayout(() => Promise.resolve({ layout: US, im: 'ja', type: true, selectInput: true }));
    await ja.refresh();
    assert.deepStrictEqual(ja.tag({ t: 'k.kd', code: 'KeyK' }), { t: 'k.kd', code: 'KeyK' }, 'not a typing key here: as it is');
    assert.deepStrictEqual(ja.tag({ t: 'k.kd', code: 'KeyA' }), { t: 'k.kd', code: 'KeyA', im: 'ja', ch: 'a' });
    assert.deepStrictEqual(ja.tag({ t: 'k.ku', code: 'KeyA' }), { t: 'k.ku', code: 'KeyA', ch: 'a' }, 'key-up: no im');
    const bad = new KeyLayout(() => Promise.resolve({ layout: US, im: 'x"; rm', type: true }));
    await bad.refresh();
    assert.strictEqual(bad.tag({ t: 'k.kd', code: 'KeyA' }).im, undefined, 'not a language: none');

    const sent = [];
    const us = new KeyLayout(() => Promise.resolve({ layout: US, type: true, selectInput: true }), (cmd, a) => sent.push([cmd, a]));
    await us.refresh();
    assert.strictEqual(us.code(true, 'KeyA', 'a', undefined, 'zh-Hans'), 'KeyA', 'the key goes: the input method composes it');
    us.code(false, 'KeyA', 'a');
    us.code(true, 'KeyC', 'c', undefined, 'zh-Hans');
    us.code(false, 'KeyC', 'c');
    assert.deepStrictEqual(sent, [['selectInput', { lang: 'zh-Hans' }]], 'switched once');
    us.code(true, 'ControlLeft');
    us.code(true, 'KeyC', 'c');
    us.code(false, 'KeyC', 'c');
    us.code(false, 'ControlLeft');
    assert.strictEqual(sent.length, 1, 'a shortcut (no im): stays');
    us.code(true, 'KeyA', 'a', undefined, 'ko');
    us.code(true, 'KeyA', 'a');
    us.reset();
    us.reset();
    assert.deepStrictEqual(sent.slice(1), [['selectInput', { lang: 'ko' }], ['selectInput', { restore: true }]],
      'another language, then a layout again: back to ours; reset after: nothing more');
    us.code(true, 'KeyA', 'a', undefined, 'ja');
    us.reset();
    assert.deepStrictEqual(sent.slice(3), [['selectInput', { lang: 'ja' }], ['selectInput', { restore: true }]], 'reset: back');

    const old = new KeyLayout(() => Promise.resolve({ layout: US, type: true }), (cmd, a) => sent.push([cmd, a]));
    await old.refresh();
    old.code(true, 'KeyA', 'a', undefined, 'ja');
    old.reset();
    assert.strictEqual(sent.length, 5, 'a helper that can\'t select: nothing');
  }

  // reset: nothing held.
  {
    const us = layout(US);
    down(us, 'ControlLeft');
    down(us, 'KeyY', 'z');
    us.reset();
    assert.strictEqual(down(us, 'KeyY', 'z'), 'KeyY', 'Ctrl forgotten');
  }

  console.log('keylayout: ok');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
