// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// The arrangement control on the Multi-device page: where each device sits around this screen.
//
// Pure, so the rules are tested (shared/test/edges-ui.js) and the page only renders them. The rules
// are the ones `src-tauri/src/mesh/edges.rs` enforces, and `ui/shared/kvm-edges.js` defines: one
// device per side, one side per device, and a side that already leads to another device refuses a
// new one rather than silently replacing it.
//
// The page draws a grid with this computer in the middle; the four cells next to it are the sides.
// This file says what is in each cell and what a click on it would ask the app to do; it does not
// decide anything the app does not. `assign_edge` is the command, and its refusal is the sentence
// shown when a choice cannot be taken.
(function (root) {
  'use strict';

  const SIDES = ['left', 'right', 'top', 'bottom'];
  const LABELS = { left: 'Left', right: 'Right', top: 'Top', bottom: 'Bottom' };
  // The 3×3 grid, read left to right and top to bottom: the four sides touch the centre cell, the
  // corners are empty, and the centre is this computer.
  const GRID = [null, 'top', null, 'left', 'here', 'right', null, 'bottom', null];

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

  // What the grid shows, cell by cell: the corners are gaps, the centre is this computer, and each
  // side cell names the device that leads off that edge (or nothing).
  function cells(edges, names) {
    const e = normalize(edges);
    return GRID.map((slot) => {
      if (slot === null) return { kind: 'gap' };
      if (slot === 'here') return { kind: 'here' };
      const id = e[slot];
      return {
        kind: 'side',
        side: slot,
        label: LABELS[slot],
        device: id,
        name: id ? (names && names[id]) || id : '',
      };
    });
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

  // What a click on a side of the grid means for the device that is being arranged.
  //
  //   { call }        the command to send: put the device there, or (its own side) take it off
  //   { held }        the side already leads to another device; the page says which and changes
  //                   nothing, because the app would refuse it and the click would look broken
  //   null            nothing to do (no device, or not a side)
  function attempt(edges, id, side, names) {
    if (!id || !SIDES.includes(side)) return null;
    const holder = deviceOn(edges, side);
    if (!holder) return { call: { side, deviceId: String(id) } };
    if (holder.toLowerCase() === String(id).toLowerCase()) {
      // Its own side: the same click takes it off, so a device is never stuck on an edge.
      return { call: change(edges, id, '') };
    }
    return { held: holder, name: (names && names[holder]) || holder };
  }

  // The side names in a sentence, for the note under the list. With a name for each device it says
  // who is there rather than repeating an id the person never saw.
  function describe(edges, names) {
    const e = normalize(edges);
    const who = (id) => (names && names[id]) || id;
    const parts = SIDES.filter((s) => e[s]).map((s) => LABELS[s].toLowerCase() + ': ' + who(e[s]));
    return parts.length ? parts.join(', ') : 'No sides assigned yet';
  }

  const api = { SIDES, LABELS, GRID, normalize, sideOf, deviceOn, cells, change, attempt, describe };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.EdgesUI = api;
})(typeof self !== 'undefined' ? self : this);
