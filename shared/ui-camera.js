// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Camera tab (Mac, PC and Linux): its switch at the top turns Camera on (off: nothing of it is
// loaded or runs, and nothing is shared); Start runs the other device's camera here. A
// phone's, back or front, through wireless debugging (phoneview.js openCamera, Android 12 or later;
// the phone's own Camera page starts it too); the other computer's, which it shares only with
// "Let … see this camera" on there (camshare.js). Its picture shows on the page (turned with
// Rotate), in a window with Window (or a click on the picture), and with Virtual camera on, in a
// camera every app here can pick ("TwinDesk Camera", shared/vcam.js), installed the first time
// it's turned on. This computer's own camera goes to the other one the same way.
'use strict';

(() => {
  let cur = null; // the last state, for the switch at the top
  let facing = 'back'; // a phone's camera to start with

  const phoneCam = (s) => ((s.phone && s.phone.bridge && s.phone.bridge.windows) || []).find((w) => w.kind === 'camera') || null;
  const running = (s) => (UI.isPhone(s) ? !!phoneCam(s) : !!(s.camera && s.camera.viewing));

  // Why it can't start now ('' when it can).
  function blocked(s) {
    const c = s.camera || {};
    if (UI.isPhone(s)) {
      const ph = s.phone || {};
      if (!ph.connected) return 'Not connected';
      if (!ph.bridge || ph.bridge.state !== 'ready') return 'Needs wireless debugging: set it up on the Phone page';
      return '';
    }
    if (!c.connected) return 'Not connected';
    if (c.ble) return 'Through Bluetooth a camera can’t go across: connect over Wi-Fi or a cable';
    if (!c.peerShares && !c.viewing) return 'Turn on “Let {me} see this camera” on {name} first';
    return '';
  }

  function start(on, show = false) {
    const s = cur;
    if (!s) return;
    if (UI.isPhone(s)) UI.api.phoneCamera(on, on ? { facing, show } : undefined);
    else UI.api.camera({ do: on ? 'view' : 'close', show });
  }

  UI.tab({
    id: 'camera',
    group: 'More',
    order: 75,
    title: 'Camera',
    icon: 'camera',
    color: 'linear-gradient(#bf5af2, #8944ab)',
    desc: 'The other device’s camera here, and as a camera other apps can pick.',
    badge: (s) => (s.camera && s.camera.sending ? 'warn' : running(s) ? 'on' : ''),
    master: {
      get: (s) => !!(s.camera && s.camera.on),
      set: (v) => UI.api.camera({ do: 'on', on: v }),
    },
    build(body) {
      const { api, el } = UI;
      const cam = (m) => api.camera(m);

      // The picture: what the camera's window sends (a JPEG, about 12 a second). A click opens the window.
      const img = el('img', { alt: '' });
      Object.assign(img.style, { width: '100%', height: '100%', objectFit: 'contain', display: 'block' });
      const said = el('div');
      Object.assign(said.style, { position: 'absolute', inset: '0', display: 'grid', placeItems: 'center', textAlign: 'center', padding: '0 24px', color: '#a1a1a6' });
      const box = el('div', { title: 'Open it in a window' }, [img, said]);
      Object.assign(box.style, { position: 'relative', aspectRatio: '16 / 9', background: '#000', borderRadius: '8px', overflow: 'hidden', cursor: 'pointer', margin: '12px' });
      box.onclick = () => cur && running(cur) && start(true, true);
      let url = '';
      if (api.onCamPreview) {
        api.onCamPreview((buf) => {
          if (!cur || !running(cur)) return;
          const next = URL.createObjectURL(new Blob([buf], { type: 'image/jpeg' }));
          img.onload = () => { if (url && url !== next) URL.revokeObjectURL(url); url = next; };
          img.src = next;
        });
      }

      const facingSeg = UI.segmented([{ value: 'back', label: 'Back' }, { value: 'front', label: 'Front' }], (v) => {
        facing = v;
        facingSeg.set(v);
        if (cur && running(cur)) api.phoneCamera(true, { facing: v }); // switches in the same window
      });
      facingSeg.set(facing);
      const rotate = UI.button('Rotate', () => cam({ do: 'rotate' }));
      rotate.title = 'A quarter turn clockwise: here, in its window and in the virtual camera';
      const windowBtn = UI.button('Window', () => start(true, true));
      windowBtn.title = 'Open it in a window';
      const startBtn = UI.button('Start', () => cur && start(!running(cur)));
      const controls = UI.row('{name}’s camera', '', [startBtn, facingSeg, rotate, windowBtn]);
      const picCard = UI.card('', [box, controls]);

      // Virtual camera: the picture also goes to a camera other apps can pick.
      const virtual = UI.switch((v) => cam({ do: 'virtual', on: v }), { label: 'Virtual camera' });
      const vRemove = UI.button('Remove', () => cam({ do: 'vcamRemove' }));
      const vRow = UI.row('Virtual camera', '', [vRemove, virtual]);
      // Which camera it feeds, where there's a choice (the Mac: TwinDesk Camera or OBS Virtual Camera).
      const vTarget = UI.select([], (t) => cam({ do: 'vcamTarget', target: t }));
      const vTargetRow = UI.row('Send to', 'The camera other apps pick', [vTarget]);
      const vCard = UI.card('Virtual camera', [vTargetRow, vRow]);

      // This computer's camera there.
      const share = UI.switch((v) => cam({ do: 'share', on: v }), { label: 'Share this camera' });
      const shareRow = UI.row('Let {name} see this camera', '', [share]);
      const device = UI.select([{ value: '', label: 'The default camera' }], (v) => cam({ do: 'device', id: v }));
      const deviceRow = UI.row('Camera', 'Which of {me}’s cameras it shows', [device]);
      const stop = UI.button('Stop', () => cam({ do: 'stop' }), 'danger');
      const liveRow = UI.row('{name} is viewing it now', 'The camera is on while it’s running there', [stop]);
      const ourCard = UI.card('{me}’s camera', [shareRow, deviceRow, liveRow]);

      const note = UI.note('info', '');
      body.append(picCard, vCard, ourCard, note);

      let listed = false;
      return (s) => {
        cur = s;
        const c = s.camera || {};
        const v = s.vcam || {};
        const phone = UI.isPhone(s);
        const on = running(s);
        const vName = `“${v.name || 'TwinDesk Camera'}”`;
        const w = phone ? phoneCam(s) : null;
        if (w && w.facing && w.facing !== facing) {
          facing = w.facing; // started or switched elsewhere (the phone's Camera page)
          facingSeg.set(facing);
        }

        // The picture, or what it's waiting for.
        const why = blocked(s);
        let text = '';
        if (!c.on) text = 'Off: turn on Camera at the top';
        else if (!on) text = UI.fill(why || 'Press Start to run {name}’s camera here');
        else if (phone && w && w.error) text = `It stopped: ${w.error}`;
        else if (!phone && c.viewStatus === 'refused') text = UI.fill('Turn on “Let {me} see this camera” on {name}');
        else if (!phone && c.viewStatus === 'failed') text = `It didn’t start: ${c.viewError || 'unknown'}`;
        else if (!phone && c.viewStatus === 'ended') text = UI.fill('{name} stopped its camera');
        else if (!v.picture) text = phone ? 'Starting the phone’s camera…' : UI.fill('Asking {name} for its camera…');
        said.textContent = text;
        said.hidden = !text;
        img.hidden = !on || !!text;
        if (!on && url) {
          URL.revokeObjectURL(url);
          url = '';
          img.removeAttribute('src');
        }
        startBtn.textContent = on ? 'Stop' : 'Start';
        startBtn.disabled = !c.on || (!on && !!why);
        startBtn.title = !on && why ? UI.fill(why) : '';
        facingSeg.el.hidden = !phone;
        rotate.disabled = !on;
        windowBtn.disabled = !on;
        windowBtn.textContent = (phone ? w && w.shown : c.viewShown) ? 'Window ✓' : 'Window';
        controls.setHint(on ? `Turned ${v.rotate || 0}°. Click the picture or Window for a window of its own` : '');

        // Virtual camera.
        virtual.set(!!v.on);
        virtual.disable(v.supported === false || !!v.busy);
        vRemove.hidden = !v.installed || !!v.on || !!v.busy || v.removable === false;
        vTargetRow.hidden = !(v.targets && v.targets.length > 1);
        if (!vTargetRow.hidden) {
          vTarget.options(v.targets);
          vTarget.set(v.target || v.targets[0].value);
          vTarget.el.disabled = !!v.busy;
        }
        vRow.setHint(!v.checked ? 'Checking…'
          : v.supported === false ? v.message
            : v.busy === 'install' ? (v.message || 'Installing…')
              : v.busy === 'remove' ? 'Removing…'
                : v.ready && v.live ? `Sharing now: in Zoom, Teams, the browser…, choose only ${vName} as the camera`
                  : v.ready ? `Ready: the camera running here goes to ${vName}. Choose only that one in the app you call from`
                    : v.on ? (v.message || 'Not installed')
                      : v.message || `Off: the picture stays in TwinDesk. On: it’s also ${vName}, a camera other apps can pick`);

        // This computer's own camera (not with a phone).
        ourCard.hidden = phone;
        if (!phone) {
          share.set(!!c.share);
          shareRow.setHint(c.shareError || (c.share ? '{name} can run it there. A popup says so here when it does' : 'Off: nobody sees it'));
          device.options([{ value: '', label: 'The default camera' }, ...(c.devices || []).map((d) => ({ value: d.id, label: d.label }))]);
          device.set(c.device || '');
          deviceRow.hidden = !c.share;
          liveRow.hidden = !c.sending;
          if (c.share && !listed) {
            listed = true;
            cam({ do: 'devices' });
          }
        }

        note.set(v.ready
          ? `Apps here see it as ${vName}: pick that one, not {me}’s own camera.` + (phone ? '' : ' The picture goes straight between the two computers.')
          : 'Turn on Virtual camera to use it as a real camera in other apps (Zoom, Teams, the browser…).' + (phone ? '' : ' The picture goes straight between the two computers.'));
      };
    },
  });
})();
