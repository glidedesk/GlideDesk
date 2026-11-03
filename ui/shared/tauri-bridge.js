// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 Polyfill Bridge:
// When running inside Tauri v2, maps the Electron preload window.macshare IPC interface
// 1:1 to Tauri's native core.invoke() and event.listen() systems.
// When running in standard Electron, does nothing and leaves window.macshare intact.
(function () {
  'use strict';

  const isTauri = typeof window !== 'undefined' && (!!window.__TAURI__ || !!window.__TAURI_INTERNALS__);
  if (!isTauri) return;

  const tauri = window.__TAURI__ || {};
  const core = tauri.core || (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke ? window.__TAURI_INTERNALS__ : null);
  const event = tauri.event || null;

  if (!core || typeof core.invoke !== 'function') return;

  const invoke = (cmd, args = {}) => core.invoke(cmd, args);

  const api = {
    getState: () => invoke('get_state'),
    onState: (fn) => {
      if (event && event.listen) {
        event.listen('state-change', (e) => fn(e.payload));
      }
    },
    onShowUpdate: (fn) => {
      if (event && event.listen) {
        event.listen('show-update', () => fn());
      }
    },
    onGo: (fn) => {
      if (event && event.listen) {
        event.listen('go', (e) => fn(e.payload));
      }
    },
    onWindowHandoffOffer: (fn) => {
      if (event && event.listen) {
        event.listen('window-handoff-received', (e) => fn(e.payload));
      }
    },
    onWindowHandoffReturn: (fn) => {
      if (event && event.listen) {
        event.listen('window-handoff-returned', (e) => fn(e.payload));
      }
    },
    onProjectionSetup: (fn) => {
      if (event && event.listen) {
        event.listen('projection-setup', (e) => fn(e.payload));
      }
    },
    onAudioReceived: (fn) => {
      if (event && event.listen) {
        event.listen('media-audio-received', (e) => fn(e.payload));
      }
    },
    onScreenReceived: (fn) => {
      if (event && event.listen) {
        event.listen('media-screen-received', (e) => fn(e.payload));
      }
    },
    onPortConflict: (fn) => {
      if (event && event.listen) {
        event.listen('port-conflict', (e) => fn(e.payload));
      }
    },
    onHub: () => {},
    onCamPreview: () => {},

    setKvm: (partial) => invoke('set_kvm', { partial }),
    setLayout: (side) => invoke('set_layout', { side }),
    setKeys: (partial) => invoke('set_keys', { partial }),
    setApp: (partial) => invoke('set_app', { partial }),
    setUpdates: (partial) => invoke('set_updates', { partial }),
    skipUpdate: (version) => invoke('skip_update', { version }),
    unskipUpdate: () => invoke('unskip_update'),
    checkUpdates: () => invoke('check_updates'),
    openUpdate: () => invoke('open_update'),
    installUpdate: () => invoke('install_update'),
    openLink: (key) => invoke('open_link', { key }),
    setMonitor: (partial) => invoke('set_monitor', { partial }),
    setClipboard: (partial) => invoke('set_clipboard', { partial }),
    copyHistory: (index) => invoke('copy_history', { index }),
    clearHistory: () => invoke('clear_history'),
    setTab: (tab) => invoke('set_tab', { tab }),
    setMulti: (on) => invoke('set_multi', { on }),
    setAudio: (partial) => invoke('set_app', { partial }).catch(() => {}),
    setFiles: (partial) => invoke('set_app', { partial }).catch(() => {}),
    setNotifs: (partial) => invoke('set_app', { partial }).catch(() => {}),
    setTerminal: (partial) => invoke('set_app', { partial }).catch(() => {}),
    setPcApps: (allow) => invoke('set_app', { partial: { allow } }).catch(() => {}),
    setPcScreenMode: (mode) => invoke('set_app', { partial: { mode } }).catch(() => {}),
    setNet: (partial) => invoke('set_app', { partial }).catch(() => {}),
    setShareOn: (on) => invoke('set_app', { partial: { on } }).catch(() => {}),
    fixFirewall: () => Promise.resolve(),
    netCancel: () => Promise.resolve(),
    netJoinHotspot: () => Promise.resolve(),
    netLocation: () => Promise.resolve(),
    netWifi: () => Promise.resolve(),

    pair: (host) => invoke('pair', { host }),
    pairRespond: (allowed) => invoke('pair_respond', { allowed }),
    respondPair: (allowed) => invoke('pair_respond', { allowed }),
    connectGuest: (id) => invoke('pair', { host: id }),
    forgetGuest: (id) => invoke('forget', { id }),
    disconnect: () => invoke('disconnect'),
    connectAgain: () => invoke('connect_again'),
    connectAddress: (addr) => invoke('connect_address', { addr }),
    refreshFound: () => invoke('refresh_found'),
    copyText: (text) => invoke('copy_text', { text }),
    skipSetup: () => invoke('skip_setup'),
    confirmCode: () => invoke('confirm_code'),
    cancelPair: () => invoke('cancel_pair'),
    forget: (id) => invoke('forget', { id }),
    wakePeer: (id) => invoke('wake_peer', { id }),
    sendFiles: (files, target = null) => invoke('send_files', { files, target }),
    chooseSendFiles: () => invoke('choose_send_files'),
    sendWireMessage: (pkt) => invoke('send_wire_message', { pkt }),
    handoffWindow: (id, title, x, y, width, height) => invoke('handoff_window', { id, title, x, y, width, height }),
    handoffWindowAtPoint: (x, y) => invoke('handoff_window_at_point', { x, y }),
    returnWindow: (id) => invoke('return_window', { id }),
    getParkedWindows: () => invoke('get_parked_windows'),
    showProjectionWindow: (id, title, width, height) => invoke('show_projection_window', { id, title, width, height }),
    hideProjectionWindow: () => invoke('hide_projection_window'),
    startAudioCapture: () => invoke('start_audio_capture'),
    stopAudioCapture: () => invoke('stop_audio_capture'),
    startScreenStreaming: (fps) => invoke('start_screen_streaming', { fps }),
    stopScreenStreaming: () => invoke('stop_screen_streaming'),
    emulateGamepadEvent: (button, pressed) => invoke('emulate_gamepad_event', { button, pressed }),
    createVirtualDisplay: (width, height) => invoke('create_virtual_display', { width, height }),

    phoneAssets: () => Promise.resolve({ apps: [], wallpaper: '' }),
    connectPhone: (id) => invoke('connect_phone', { id }).catch(() => {}),
    disconnectPhone: () => invoke('disconnect_phone').catch(() => {}),
    phoneScreen: (show) => invoke('phone_screen', { show }).catch(() => {}),
    phoneRetry: () => invoke('phone_retry').catch(() => {}),
    phonePair: (opts) => invoke('phone_pair', opts).catch(() => {}),
    phoneCamera: (on, opts) => invoke('phone_camera', { on, opts }).catch(() => {}),
    hub: (msg) => invoke('hub_message', { msg }).catch(() => {}),

    pathsOf: (files) => [...files].map((f) => f.path || f.name).filter(Boolean),
  };

  window.macshare = api;
  console.log('[GlideDesk] Running on Tauri v2 native bridge (low-memory runtime)');
})();
