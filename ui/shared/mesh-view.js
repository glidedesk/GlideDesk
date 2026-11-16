// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// What the Multi-device page shows, worked out from the state the app pushes.
//
// Pure: no DOM, no sockets, so the rules are tested (shared/test/ui-multi.js) and the page only
// renders what is computed here. Two devices are what makes the mesh worth a page at all: with one,
// the app is the two-device app and the tab must not exist (specs/001-multi-device-mesh, FR-003).
//
// The wording mirrors `refusal_text` in src-tauri/src/mesh/mod.rs. The app sends a sentence over the
// wire for the device that was refused; this file words the reasons the page reads from state. If
// one changes, change the other: a person must not read two different explanations of one refusal.
(function (root) {
  'use strict';

  const ROLES = ['idle', 'driving', 'driven'];
  const OS = { mac: 'Mac', win: 'Windows', linux: 'Linux', android: 'Phone', ios: 'Phone' };

  // The sessions as the page needs them. A state from an app that predates the mesh, or a state
  // pushed before the first one arrives, falls back to the device list so the page never blanks.
  function sessions(state) {
    const mesh = state && state.mesh;
    let list;
    if (mesh && Array.isArray(mesh.sessions) && mesh.sessions.length) {
      list = mesh.sessions;
    } else {
      const clients = state && Array.isArray(state.clients) ? state.clients : [];
      list = clients.map((c) => ({
        id: c.id, name: c.name, os: c.os, address: c.address, role: 'idle', mesh: false, pool: true,
      }));
    }
    return list
      .map((s) => ({
        id: String(s.id || ''),
        name: String(s.name || s.id || ''),
        os: String(s.os || ''),
        address: String(s.address || ''),
        role: ROLES.includes(s.role) ? s.role : 'idle',
        mesh: !!s.mesh,
        pool: s.pool !== false,
      }))
      .filter((s) => s.id);
  }

  // The tab and the mesh only exist with two or more devices connected (FR-003).
  function visible(state) {
    return sessions(state).length >= 2;
  }

  function active(state) {
    if (state && state.mesh && typeof state.mesh.active === 'boolean') return state.mesh.active;
    return visible(state);
  }

  // One line per device, as a person reads it: who has the pointer, not a wire enum.
  function roleText(role) {
    if (role === 'driving') return 'This computer drives it';
    if (role === 'driven') return 'It drives this computer';
    return 'Idle';
  }

  function kind(os) {
    return OS[String(os || '').toLowerCase()] || String(os || '');
  }

  function summary(state) {
    const n = sessions(state).length;
    if (n === 0) return 'Waiting';
    if (n === 1) return 'One device';
    return n + ' devices';
  }

  // Why a crossing or a copy did not happen, in words (`FR-023`).
  function refusalText(refusal) {
    if (!refusal || !refusal.why) return '';
    const who = String(refusal.device || 'The other device');
    switch (refusal.why) {
      case 'limit':
        return who + ' was not added: this desk already has ten devices connected. Disconnect one to make room.';
      case 'not_allowed':
        return who + ' is not accepting the pointer right now.';
      case 'not_connected':
        return who + ' is not connected.';
      case 'busy':
        return who + ' is already driving or being driven.';
      case 'not_ready':
        return who + ' is not ready for the pointer yet.';
      case 'too_large':
        return 'That copy is larger than the clipboard limit.';
      case 'concealed':
        return 'That copy is concealed, so it was not shared.';
      default:
        return who + ' did not answer.';
    }
  }

  const api = { sessions, visible, active, roleText, kind, summary, refusalText, OS };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.MeshView = api;
})(typeof self !== 'undefined' ? self : this);
