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
    onHub: (fn) => {
      if (event && event.listen) {
        event.listen('hub-message', (e) => fn(e.payload));
      }
    },
    onCamPreview: (fn) => {
      if (event && event.listen) {
        event.listen('cam-preview', (e) => {
          const data = e.payload instanceof Uint8Array ? e.payload : new Uint8Array(e.payload);
          fn(data);
        });
      }
    },
    on: (evt, fn) => {
      if (event && event.listen) {
        event.listen(evt, (e) => fn(e.payload));
      }
    },
    send: (msg) => invoke('cam_send', { msg: msg || {} }),

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
    fixFirewall: () => invoke('fix_firewall'),
    netCancel: () => invoke('net_cancel'),
    netJoinHotspot: (ssid, key) => invoke('net_join_hotspot', { ssid: String(ssid || ''), key: String(key || '') }),
    netLocation: () => invoke('net_location'),
    netWifi: (arg) => invoke('net_wifi', { arg: arg || {} }),

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

    phoneAssets: () => invoke('phone_assets'),
    connectPhone: (id) => invoke('connect_phone', { id: id || '' }),
    disconnectPhone: () => invoke('disconnect_phone'),
    phoneScreen: (show) => invoke('phone_screen', { show: !!show }),
    phoneRetry: () => invoke('phone_retry'),
    phonePair: (opts) => invoke('phone_pair', { opts: opts || {} }),
    phoneCamera: (on, opts) => invoke('phone_camera', { on: !!on, opts: opts || {} }),
    hub: (msg) => invoke('hub_message', { msg: msg || {} }),

    camera: (arg) => invoke('camera', { arg: typeof arg === 'object' && arg !== null ? arg : { on: !!arg } }),
    cancelDrop: () => invoke('cancel_drop'),
    checkFirewall: () => invoke('check_firewall'),
    chooseReceivedFolder: () => invoke('choose_received_folder'),
    chooseShareFolder: () => invoke('choose_share_folder'),
    clearNotifHistory: () => invoke('clear_notif_history'),
    installVdd: () => invoke('install_vdd'),
    launchApp: (name) => invoke('launch_app', { name: String(name || '') }),
    listApps: () => invoke('list_apps'),
    muteNotifApp: (arg) => {
      const app = typeof arg === 'object' && arg ? (arg.app || arg.appName || '') : String(arg || '');
      return invoke('mute_notif_app', { app: app, app_name: app });
    },
    openBluetoothSettings: () => invoke('open_bluetooth_settings'),
    openMacDrive: () => invoke('open_mac_drive'),
    openMonitor: () => invoke('open_monitor'),
    openNotifiedApp: (arg) => {
      const id = typeof arg === 'object' && arg ? (arg.app || arg.appName || arg.id || '') : String(arg || '');
      return invoke('open_notified_app', { id: String(id) });
    },
    openReceivedFolder: () => invoke('open_received_folder'),
    openTerminal: () => invoke('open_terminal'),
    phoneAppMenu: () => invoke('phone_app_menu'),
    phoneBtPick: () => invoke('phone_bt_pick'),
    phoneCloseView: () => invoke('phone_close_view'),
    phoneInstallFull: () => invoke('phone_install_full'),
    phoneMedia: (action) => {
      const act = typeof action === 'object' && action ? (action.a || JSON.stringify(action)) : String(action || '');
      return invoke('phone_media', { action: act });
    },
    phoneMount: () => invoke('phone_mount'),
    phoneOpenApp: (arg) => {
      const pkg = typeof arg === 'object' && arg ? (arg.id || arg.name || '') : String(arg || '');
      return invoke('phone_open_app', { pkg });
    },
    phoneUnmount: () => invoke('phone_unmount'),
    pinApp: (name) => invoke('pin_app', { name: String(name || '') }),
    remoteScreen: () => invoke('remote_screen'),
    removeVdd: () => invoke('remove_vdd'),
    resetReceivedFolder: () => invoke('reset_received_folder'),
    setMacApps: (partial) => invoke('set_mac_apps', { partial: partial || {} }),
    setMount: (partial) => invoke('set_mount', { partial: partial || {} }),
    setPcScreen: (mode) => invoke('set_pc_screen', { mode: typeof mode === 'string' ? mode : ((mode && mode.mode) || '') }),
    setPhone: (partial) => invoke('set_phone', { partial: partial || {} }),
    setPhoneRemote: (on) => invoke('set_phone_remote', { on: !!on }),
    setRemote: (partial) => invoke('set_remote', { partial: partial || {} }),
    setStartup: (on) => invoke('set_startup', { on: !!on }),
    setWake: (on) => invoke('set_wake', { on: !!on }),
    setupSshd: () => invoke('setup_sshd'),

    pathsOf: (files) => [...files].map((f) => f.path || f.name).filter(Boolean),
  };

  window.macshare = api;

  window.cam = {
    on: (fn) => {
      if (event && event.listen) {
        event.listen('cam-in', (e) => fn(e.payload));
      }
    },
    send: (m) => invoke('cam_send', { msg: m }),
  };

  window.camout = {
    hello: () => invoke('vcam_hello'),
    frame: (buf) => invoke('vcam_frame', { frame: Array.from(buf instanceof Uint8Array ? buf : new Uint8Array(buf)) }),
    preview: (buf) => invoke('vcam_preview', { preview: Array.from(buf instanceof Uint8Array ? buf : new Uint8Array(buf)) }),
    on: (fn) => {
      if (event && event.listen) {
        event.listen('vcam-set', (e) => fn(e.payload));
      }
    },
  };

  console.log('[GlideDesk] Running on Tauri v2 native bridge (low-memory runtime)');
})();
