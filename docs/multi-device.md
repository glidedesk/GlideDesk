# Multi-device

GlideDesk is designed with full multi-device mesh continuity: many computers
and phones connected at once, one clipboard and one notification pool for all of them, each screen edge
leading to a different device, a phone and a computer connected at the same time.

It is off by default, and while it is off TwinDesk runs exactly the code it runs today.

## Rules

1. **Off means today's app.** With Multi-device off nothing of the mesh is loaded: no module, no
   listener, no extra field on the wire. The one-to-one connection code (`server.js`, `link.js`,
   `peer.js`, `hostpair.js`/`pairing.js`, `kvm.js`, `clip.js`/`clipboard.js`, `notifs.js`, `phone*.js`,
   `wired.js`, …) is not changed for it. The one-to-one app only gains the switch: one line in
   `main.js`, `multi.js`, and the Multi-device tab.
2. **One switch, read at start.** Multi-device tab › *Use several devices at once* › Turn on / Turn off.
   It is `on` in `multi.json`; `main.js` reads it once, right after it sets the user data folder, and with
   it on hands over to `mesh/main.js`. Changing it restarts TwinDesk (as Quit, then start again), so the
   two never run side by side; the window comes back on the Multi-device tab.
3. **Everything for several devices is in the Multi-device tab**: the layout of the screens (which device
   each edge leads to) and *Join every device automatically*. The device list is the sidebar's. The
   other tabs stay as they are.
4. **Storage of its own.** The one-to-one app's files are read by the mesh, never written:

   | | one-to-one app | mesh |
   | --- | --- | --- |
   | hosts this computer dials | `pairing.json` (Windows), `hosts.json` (Mac) | `pairings.json`, `hosts-mesh.json` |
   | devices that dial this computer | `paired.json` | `paired-mesh.json` |
   | settings only the mesh has | – | `multi.json` (`layout`, `mesh`, Windows `kvm.edges`) |

   A pairing of the one-to-one app joins the mesh's list once (`multi.json` › `imported`): one forgotten
   in the mesh stays forgotten, one paired again in the one-to-one app joins again. Settings both have
   (speed, clipboard, …) stay in `settings.json`, the same choices in both; `kvm.pcSide` is kept.
   Switching off gives back exactly what was there.
5. **Older devices and devices with it off** are plain one-to-one peers to a multi-device one: no
   roster, no relayed items. Their `kvm.layout` places them where they say, as the one-to-one app does;
   between mesh devices the shared layout is the one map.
6. **Native helpers: two builds of the same sources.** What Multi-device adds (several watched sides,
   the side in the edge and return events) is inside `#if MULTI`: `windows-helper/src/CaptureMulti.cs`,
   `Program.cs`, `Json.cs`; `mac-helper`'s `Capture.swift` and `main.swift`, where the `#else` parts are
   today's code as it was. `macshare-winhelper.exe`, `macshare-helper` and `macshare-linuxhelper` are
   built without it and are today's helpers; the `-multi` builds have it, and only `mesh/helper.js`
   starts them. `linux-helper` reads the sides as `sideList` ("left,top"), as its JSON reader skips
   arrays. Without a `-multi` build the mesh starts the one-to-one helper,
   which watches the first side (the mesh sends it as `side` too).
7. **Android, first release: unchanged.** A multi-device computer sees today's phone app as a plain
   one-to-one device. A phone with several computers at once comes later, behind its own switch.
8. **Linux** has Multi-device too (`linux/mesh/`), with less in it, to stay clear of what Bluetooth,
   the desktops and the GPU drivers make fragile there:
   - with it on, other computers connect to the Linux one over the same network only, never Bluetooth
     (no `BleGuest`; `BleHost` and the Bluetooth policy take phones only; General's *Bluetooth with
     computers* is left as the one-to-one app has it). Phones connect as before, Bluetooth included.
   - screen sharing goes to the selected device, as today (no screen per device, no virtual display
     per device);
   - several watched edges on X11 (`macshare-linuxhelper-multi`); the Wayland backend watches one;
   - Windows and Mac keep Bluetooth with each other in Multi-device.
   A Linux computer with Multi-device off is a plain one-to-one peer to the others, with its Bluetooth
   as today.
9. **Join every device automatically** is on by default, so every computer the mouse goes through ends up
   connected to the others (a chain needs the computer whose mouse is used to be connected to each device
   of it). With it on, a paired device can add another device to this one without a pairing code; off,
   each pair of devices is paired by hand, and the layout shows in red a device the pointer stops before
   (in the mesh, not connected here) and greys the ones past it. Switched off, multi.json keeps
   `mesh: { autoJoin: false, chosen: true }`; a `false` without `chosen` was written by a build before
   2.2.0, where off was the default and every save wrote it, so it reads as on.

## Where things are

- `shared/mesh.js`, `intro.js`, `layout.js`, `kvm-edges.js`, `screen-mesh.js`, `sidebar-model.js`,
  `notif-guard.js`: the mesh core, plain JS for every platform. Tests in `shared/test/`.
- `windows/multi.js`, `mac/multi.js`: the switch. `ui-multi.js`: the tab with it off.
- `windows/mesh/`: the Windows app with Multi-device on. `main.js`, `ipc.js`, `preload-app.js`; the link
  layer (`server`, `link`, `hostpair`, `links`, `session`, `hub`); the features (`clip`, `notifs`, `kvm`,
  `layout-sync`, `autojoin`, `phones`, `phone`, `phone-link`, `phone-bridge`, `wired`, `pcscreen`,
  `pcscreenset`, `sender`); `settings` and `store` (its files, rule 4); the window (`app.html` with
  `<base href="../">`, so everything it doesn't have loads from `windows/`, its `app.js`, pages and css,
  `ui-multi.js`).
- `mac/mesh/`: the same for the Mac (`services`, `pairing`, `clipboard`, `stream`/`streams`, `displays`,
  `control.html`, …).
- `linux/mesh/`: the same for Linux, made from `windows/mesh/` with `linux/`'s own parts (notifications
  read with dbus-monitor, sleep that ends every session and invites each device back: `sleepwake.js`).
- A file in `mesh/` named like one a folder up is its mesh version: when a fix there applies to both,
  make it in both.
- The design: [mesh.md](mesh.md) and [screen-mesh.md](screen-mesh.md).

Tests: `node <file>` for each of `shared/test/*.js`, `windows/mesh/test/*.js`, `mac/mesh/test/*.js` and
`linux/mesh/test/*.js`,
with `NODE_PATH` set to `windows/node_modules`. `switch.test.js` checks rule 1 (off: nothing of the
mesh is loaded) and the hand-over; `storage.test.js` checks rule 4. The installers leave `test` folders
out.

## Steps

1. Import the mesh as new files, its tests passing. *(done)*
2. Windows and Mac: the switch and the Multi-device tab, the mesh app in `mesh/`, its own storage,
   `kvm.layout` from older peers. *(done)*
3. Helpers: several watched sides in `-multi` builds of the same sources (rule 6); the fork's extra
   edge logging left out. *(done; the Windows helper built without `MULTI` has the same types,
   methods and IL as before, compared method by method; the Mac `-multi` build is still to be compiled
   on a Mac; the Linux helper built without `MULTI` compiles to the same object files as before)*
4. Real devices: two computers and a phone with the switch off (must match today), then on.
5. Android: several computers at once, behind its own switch.
6. Linux. *(done: computers over the network only, the screen to the selected device, several edges on X11)*

## Not part of this

- The clipboard rework in the mesh fork (images, files, the latest copy wins, a Recent list) changes
  today's clipboard: a pull request of its own.
- Pull request #28 (a phone and a computer at once, several edges) is what Multi-device does; here it
  lives behind the switch.
- The mesh fork's log file (`shared/logfile.js`).

## Architecture

The multi-device mesh runs natively in GlideDesk with zero-copy async networking and low-latency input streaming.
