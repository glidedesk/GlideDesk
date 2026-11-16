// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// The edge picker on the Multi-device page: which device sits behind each side of this screen.
//
// Pure, so the rules are tested (shared/test/edges-ui.js) and the page only renders them. The rules
// are the ones `src-tauri/src/mesh/edges.rs` enforces, and `ui/shared/kvm-edges.js` defines: one
// device per side, one side per device, and a side that already leads to another device refuses a
// new one rather than silently replacing it.
//
// This file words the picker; it does not decide anything the app does not. `assign_edge` is the
// command, and its refusal is the sentence shown when a choice cannot be taken.
(function (root) {
  'use strict';

  const SIDES = ['left', 'right', 'top', 'bottom'];
  const LABELS = { left: 'Left', right: 'Right', top: 'Top', bottom: 'Bottom' };

  function normalize(edges) {
    const out = { left: null, right: null, top: null, bottom: null };
    if (!edges || typeof edges !== 'object') return out;
    const seen = [];
    for (const side of SIDES) {
      const id = edges[side];
      if (typeof id !== 'string') continue;
      const clean = id.trim();
      if (!clean || clean.length > 64) continue;
      if (seen.some((s) => s.toLowerCase() === clean.toLowerCase())) continue;
      seen.push(clean);
      out[side] = clean;
    }
    return out;
  }

  // Which side of this screen a device is on, or null.
  function sideOf(edges, id) {
    if (!id) return null;
    const e = normalize(edges);
    for (const side of SIDES) {
      if (e[side] && e[side].toLowerCase() === String(id).toLowerCase()) return side;
    }
    return null;
  }

  // The device on a side, or null.
  function deviceOn(edges, side) {
    return normalize(edges)[side] || null;
  }

  // The picker's choices for one device. The current side is selected; a side that leads to a
  // different device is marked, because choosing it is what the app will refuse.
  function options(edges, id, names) {
    const current = sideOf(edges, id);
    const list = [{ value: '', label: 'No side', selected: current === null, disabled: false }];
    for (const side of SIDES) {
      const holder = deviceOn(edges, side);
      const taken = holder !== null && holder.toLowerCase() !== String(id).toLowerCase();
      list.push({
        value: side,
        label: LABELS[side],
        selected: current === side,
        // Shown but not offered: the app refuses it, and a picker that offers a refusal is a
        // control that does nothing (spec Clarifications).
        disabled: taken,
        holder: taken ? (names && names[holder]) || holder : null,
      });
    }
    return list;
  }

  // What to call the `assign_edge` call for a chosen value.
  // `''` clears the device's side (the command takes the side it is on now and a null device).
  function change(edges, id, value) {
    if (!id) return null;
    if (value === '' || value === null || value === undefined) {
      const side = sideOf(edges, id);
      return side ? { side, deviceId: null } : null;
    }
    if (!SIDES.includes(value)) return null;
    return { side: value, deviceId: String(id) };
  }

  // The side names in a sentence, for the note under the list.
  function describe(edges) {
    const e = normalize(edges);
    const parts = SIDES.filter((s) => e[s]).map((s) => LABELS[s].toLowerCase() + ': ' + e[s]);
    return parts.length ? parts.join(', ') : 'No sides assigned yet';
  }

  const api = { SIDES, LABELS, normalize, sideOf, deviceOn, options, change, describe };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.EdgesUI = api;
})(typeof self !== 'undefined' ? self : this);
