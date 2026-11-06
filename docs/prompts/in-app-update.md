# Prompt — Working In-App Download & Install (macOS / Windows / Linux)

Copy everything below the line into your coding agent.

---

## Task

Make GlideDesk's built-in updater **download and install new releases from inside the app, on macOS, Windows, and Linux**, with real progress, real error reporting, and a safe fallback. Today "Update now" spawns a detached shell/PowerShell script, returns `Ok(())` immediately, never reports success or failure, and leaves the UI stuck on "Checking...".

Repository: `glidedesk/GlideDesk` (Tauri v2 + Rust, GPL-3.0). Work on `main`, one focused commit per fix.

## Current architecture (read this before changing anything)

- The updater is **custom**, not `tauri-plugin-updater`. There is no `plugins.updater` in `src-tauri/tauri.conf.json`; Cargo only has `tauri-plugin-shell`.
- `src-tauri/src/commands.rs`
  - `check_updates` — GETs `https://api.github.com/repos/glidedesk/GlideDesk/releases?per_page=5`, picks a candidate with `pick_candidate` (skips drafts, prefers stable over pre-release), compares with `is_version_newer`, stores `UpdateDownload` via `pick_asset` / `pick_asset_for` (platform-parameterized).
  - `install_update` — reads `state.updates.download.url`, spawns `tauri::async_runtime::spawn` with three `#[cfg(target_os = ...)]` blocks that build `bash`/`powershell` one-liners and `.spawn()` them **detached**. It sets `checking = true`, calls `push_state`, and returns `Ok(())` with no follow-up.
  - `open_update` / `open_link` — open URLs in the browser via `app.shell().open`.
- `src-tauri/src/state.rs`
  - `UpdatesView` (fields: `current`, `latest`, `available`, `checking`, `error`, `checked_at`, `release_url`, `release_notes`, `published_at`, `skipped`, `skipped_version`, `prompt_update`, `auto`, `download: Option<UpdateDownload>`, `no_releases`).
  - `UpdateDownload { name, url, size }`.
  - `UpdatesView::default()` uses `env!("CARGO_PKG_VERSION")` — do **not** reintroduce a hard-coded version.
- `ui/ui-about.js` — About tab. `Check now` → `api.checkUpdates()`; `Update now` → `api.installUpdate()` (falls back to `api.openUpdate()`); state rendering at the bottom of the `build()` return.
- `ui/shared/tauri-bridge.js` — bridge: `setUpdates`, `checkUpdates`, `openUpdate`, `installUpdate`.
- `ui/shared/updates.js` is **not loaded by the Tauri app** (Electron-era, test-only). Do not wire UI through it.
- `src-tauri/capabilities/default.json` — currently `core:default`, `shell:allow-open`, `core:event:default`, `core:window:default`.

## Known defects to fix (verify each against the code, don't trust this list)

1. **Version reporting** — `UpdatesView::default().current` must be `env!("CARGO_PKG_VERSION")`; a stale literal made every install report a phantom update. (Already fixed — keep the regression test `state::tests::test_updates_default_current_matches_crate_version`.)
2. **Asset selection diverges** — the macOS/Linux bash and Windows PowerShell scripts **re-select the download URL themselves** with regex/`-like` matching instead of using the already-correct `pick_asset_for`. The Windows fallback `-like "*.exe"` can still hand users `GlideDesk-portable-x64.exe`, which ignores `/SILENT` and does not update. **The scripts must use the URL chosen by Rust; the embedded re-selection is dead weight and a bug source.**
3. **No result** — after `install_update`, nothing sets `checking = false`, and success/failure is never surfaced. The About tab shows "Checking..." forever.
4. **No download feedback** — `install_update` gives the user no progress, no cancel, and no way to know whether the bytes landed.
5. **Divergent success semantics per OS** — macOS copies a DMG into `/Applications` and re-signs ad-hoc; Windows runs the NSIS setup with `/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS`; Linux drops an AppImage in `/tmp` and runs it. Each path can fail silently (read-only `/Applications`, unsigned build blocked by Gatekeeper, `/tmp` noexec, process name mismatch in `pkill -ix glidedesk`).
6. **No verification** — no checksum/signature check on the downloaded artifact.
7. **No fallback** — when the download or install fails, the user gets nothing; there is no "open the release page instead" path.

## Required behavior

Implement a single coherent flow, identical in the UI on all three OSes:

1. **Detect** — `check_updates` reports `available`, `latest`, release notes, and the chosen asset (name, size, URL) for the current platform.
2. **Download in-app** — on "Update now", download the chosen asset to a per-user temp dir with visible progress (percent and/or bytes), and update `UpdatesView` as it goes. Stream the body; do not buffer a whole 200 MB DMG into memory.
3. **Verify** — confirm size (and checksum if a checksum asset exists); abort with a clear error if it doesn't match.
4. **Install per OS** (no browser):
   - **macOS**: mount the DMG, replace `/Applications/GlideDesk.app`, remove quarantine (`xattr -cr`), then **quit the running instance first, pause, then `open` the new bundle** (opening before `pkill` was a real bug). If `/Applications` isn't writable, fail with an actionable message ("move GlideDesk to /Applications"), not a silent no-op. Note the current build is **unsigned/not notarized** — document the Gatekeeper caveat.
   - **Windows**: download the **NSIS `-setup.exe`** (never the portable exe), run it with `/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS`, and detect a real failure (e.g. non-zero exit or UAC dismissal) instead of assuming success.
   - **Linux**: download the `.AppImage` to a user-writable dir that is **not** assumed executable (`$XDG_DATA_HOME`/`~/.local/bin`, not hard-coded `/tmp`), `chmod +x`, relaunch, then quit the old instance. Handle AppImage process naming when terminating the old instance.
5. **Report** — on completion or failure, set `checking = false` and surface a status the About tab renders: downloading (%), installing, "Update installed — restarting", or an error string. The user must never be left on "Checking...".
6. **Fall back** — any failure offers "Open the release page" (`open_update`), which already exists.
7. **Autorun stays opt-in-safe** — respect `updates.auto`; never install without a click.

## Constraints

- Do **not** bundle, copy, or repackage Beam or LinkMyMac. Inspiration only.
- Keep GPL-3.0 headers and the existing code style (Rust: platform-parameterized, unit-tested; JS: plain browser JS, no new dependencies).
- **No hard-coded version strings** — use `env!("CARGO_PKG_VERSION")`.
- Do not add `tauri-plugin-updater` unless you also add signing keys and a `plugins.updater` config **and** explain the key-distribution plan. Prefer extending the existing custom updater.
- Android companion is **de-scoped** — desktop OSes only.
- Any new capability permission must be added to `src-tauri/capabilities/default.json` (e.g. if you stop shelling out and use a plugin).
- Do not claim a fix works unless you verified it. If a step needs a real device, label it "awaiting real-device observation".

## Tests (required)

- **Rust**: unit-test version comparison, candidate selection, and asset selection for **all three platforms on every host** via `pick_asset_for("macos"|"windows"|"linux", &assets)` (the Windows branch must be covered on macOS CI). Add tests for the new download/verify/install decision logic (e.g. a `verify_download(name, size, bytes)` helper and a per-OS `install_plan(platform)` that is pure and testable).
- **JS**: extend `ui/shared/test/updates.js` style tests only if you touch `updates.js`. Add a UI-level test for the status strings if the harness supports it.
- Keep `cargo test` green and `cargo check` warning-clean.

## Acceptance criteria

- [ ] `cargo test` passes; count only goes **up**; no dead-code warnings.
- [ ] `pick_asset_for` covers macOS (universal → arch → any dmg), Windows (`-setup.exe` → `.msi` → non-portable `.exe`), Linux (`.AppImage` → `.deb`), verified by unit tests on one host.
- [ ] `install_update` never leaves `checking = true`; it always ends in a success or error state visible in the About tab.
- [ ] No install script re-selects the asset URL — the URL comes from Rust.
- [ ] On failure, the UI offers the release page.
- [ ] Windows path can never select the portable exe.
- [ ] macOS install quits before relaunching.
- [ ] README/CHANGELOG note the updater behavior and the unsigned-build/Gatekeeper caveat.

## Verification to run and paste back

```sh
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
node scripts/test-all.js --js
npm run test:e2e        # live localhost suite (needs the compiled binary)
```

Then state plainly what was verified by code/unit tests versus what still needs a real install on each OS. Do not mark anything "fixed" that you only read.

## Suggested commit sequence

1. `fix(updater): use the Rust-selected asset in install scripts`
2. `feat(updater): stream the download with progress and verify it`
3. `feat(updater): report install result to the UI on all OSes`
4. `fix(updater): per-OS install hardening (macOS quit-first, Windows setup-only, Linux user dir)`
5. `docs(updater): document in-app update behavior and caveats`
