// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Where the video playing in a browser here is, for ReZume's video handoff (shared/rezume.js): the
// other device opens the same YouTube video at the same second. From the system's media controls:
// Windows' media sessions (GlobalSystemMediaTransportControls, through PowerShell), Linux's MPRIS
// (playerctl). The Mac has nothing an app may read: null there.
//   position() → Promise<seconds | null>
'use strict';

const { execFile } = require('child_process');

const run = (cmd, args, timeout = 6000) => new Promise((resolve) => {
  execFile(cmd, args, { timeout, windowsHide: true }, (err, out) => resolve(err ? '' : String(out || '').trim()));
});

const WIN = `
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$await = ([System.WindowsRuntimeSystemExtensions].GetMethods() | ? { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation\`1' })[0]
function Await($op, $t) { $task = $await.MakeGenericMethod($t).Invoke($null, @($op)); $task.Wait(-1) | Out-Null; $task.Result }
[Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager,Windows.Media.Control,ContentType=WindowsRuntime] | Out-Null
$m = Await ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager]::RequestAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager])
$s = $m.GetSessions() | ? { $_.SourceAppUserModelId -match 'chrome|msedge|firefox|brave|opera|vivaldi|arc' } | Select-Object -First 1
if ($s) { $t = $s.GetTimelineProperties(); $p = $s.GetPlaybackInfo(); $pos = $t.Position.TotalSeconds; if ($p.PlaybackStatus -eq 'Playing') { $pos += ((Get-Date).ToUniversalTime() - $t.LastUpdatedTime.UtcDateTime).TotalSeconds }; [math]::Floor($pos) }`;

async function position() {
  let out = '';
  if (process.platform === 'win32') out = await run('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', WIN], 8000);
  else if (process.platform === 'linux') out = await run('playerctl', ['--player=chromium,chrome,firefox,brave,%any', 'position']);
  const n = Math.floor(Number(String(out).split(/\r?\n/).pop()));
  return Number.isFinite(n) && n >= 0 ? n : null;
}

// A YouTube video's link with its time (t=<s>s); anything else as it is.
function atTime(url, sec) {
  if (!sec || sec < 3) return url;
  try {
    const u = new URL(url);
    const yt = /(^|\.)youtube\.com$/.test(u.hostname) && (u.pathname === '/watch' || u.pathname.startsWith('/shorts/') || u.pathname.startsWith('/live/'));
    if (!yt && u.hostname !== 'youtu.be') return url;
    u.searchParams.set('t', `${sec}s`);
    return u.href;
  } catch (_) { return url; }
}

const isVideo = (url) => /^https?:\/\/((www|m)\.)?(youtube\.com\/(watch|shorts\/|live\/)|youtu\.be\/)/i.test(String(url || ''));

module.exports = { position, atTime, isVideo };
