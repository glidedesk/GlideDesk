// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// Mouse at 1000 Hz, between two computers (a benchmark, not part of the test run): the sender makes
// a pointer move every millisecond for 30 s and sums them the way kvm.js does (one k.mm per turn of
// the event loop), over a TLS WebSocket like the session's; the receiver hands them to a process
// through a pipe, as main does to the helper. The two clocks are lined up first (the ping with the
// shortest round trip), so the delay is from the move to its write into the pipe.
//
//   receiver: node shared/test/movebench.js serve [port] [--helper 'cmd …']   (default: cat > /dev/null)
//   sender:   node shared/test/movebench.js send host[:port] [seconds] [--hz 1000]
// Prints p50 / p95 / p99 / max delay, how many moves went per message, and the backlog (moves made
// but not yet written on the other side) at its largest and at the end. Goal: p95 ≤ 16 ms, no growth.
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const https = require('https');
const { execFileSync, spawn } = require('child_process');

const WebSocket = (() => {
  for (const app of ['linux', 'mac', 'windows']) {
    try { return require(path.join(__dirname, '..', '..', app, 'node_modules', 'ws')); } catch { /* next */ }
  }
  throw new Error('ws not found: npm install in linux/, mac/ or windows/');
})();
const { KvmMoves } = require('../kvm-moves.js');

const now = () => Number(process.hrtime.bigint()) / 1e6; // ms
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : def; };

function serve() {
  const port = Number(args[1]) || 47590;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'movebench-'));
  execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1', '-subj', '/CN=bench',
    '-keyout', path.join(dir, 'k.pem'), '-out', path.join(dir, 'c.pem')], { stdio: 'ignore' });
  const server = https.createServer({ key: fs.readFileSync(path.join(dir, 'k.pem')), cert: fs.readFileSync(path.join(dir, 'c.pem')) });
  const wss = new WebSocket.Server({ server });
  wss.on('connection', (ws) => {
    ws._socket.setNoDelay(true);
    const helper = spawn('sh', ['-c', opt('--helper', 'cat > /dev/null')], { stdio: ['pipe', 'ignore', 'inherit'] });
    // As main on the driven side: each k.mm summed with the ones that came in the same turn
    // (shared/kvm-moves.js), then a JSON line into the helper's pipe.
    let seqs = [];
    const moves = new KvmMoves({
      send: (m) => {
        helper.stdin.write(JSON.stringify({ cmd: 'inject', ev: { t: 'k.mm', dx: m.dx, dy: m.dy } }) + '\n');
        ws.send(JSON.stringify({ t: 'ack', seqs, at: now() }));
        seqs = [];
      },
    });
    ws.on('message', (data) => {
      const m = JSON.parse(data);
      if (m.t === 'ping') return ws.send(JSON.stringify({ t: 'pong', ts: m.ts, at: now() }));
      if (m.t === 'k.mm') {
        seqs.push(m.seq);
        moves.add(m.dx, m.dy);
      }
    });
    ws.on('close', () => helper.stdin.end());
  });
  server.listen(port, '0.0.0.0', () => console.log(`movebench: listening on ${port}`));
}

async function send() {
  const [host, p] = String(args[1] || '127.0.0.1').split(':');
  const seconds = Number(args[2]) || 30;
  const hz = Number(opt('--hz', 1000));
  const ws = new WebSocket(`wss://${host}:${p || 47590}`, { rejectUnauthorized: false });
  await new Promise((r, j) => { ws.once('open', r); ws.once('error', j); });
  ws._socket.setNoDelay(true);
  const waiters = [];
  const made = new Map(); // seq -> time of the oldest move in that k.mm
  const delays = [];
  let acked = 0, sent = 0, maxBacklog = 0, perMsg = 0, msgs = 0;
  let offset = 0;
  ws.on('message', (data) => {
    const m = JSON.parse(data);
    if (m.t === 'pong') return waiters.shift()(m);
    if (m.t === 'ack') {
      for (const s of m.seqs) {
        const t = made.get(s);
        made.delete(s);
        delays.push(m.at - offset - t.made);
        acked += t.n;
      }
    }
  });
  // Clock: their time = ours + offset, from the ping with the shortest round trip.
  let best = Infinity;
  for (let i = 0; i < 200; i++) {
    const t0 = now();
    ws.send(JSON.stringify({ t: 'ping', ts: t0 }));
    const m = await new Promise((r) => waiters.push(r));
    const t1 = now();
    if (t1 - t0 < best) { best = t1 - t0; offset = m.at - (t0 + t1) / 2; }
  }
  console.log(`round trip ${best.toFixed(2)} ms (clock lined up to ±${(best / 2).toFixed(2)} ms)`);

  // As main on the driving side: k.mm from the helper summed per turn (shared/kvm-moves.js).
  let seq = 0, oldest = 0, n = 0;
  const moves = new KvmMoves({
    send: (m) => {
      made.set(seq, { made: oldest, n });
      ws.send(JSON.stringify({ t: 'k.mm', dx: m.dx, dy: m.dy, seq: seq++ }));
      perMsg += n;
      msgs++;
      n = 0;
    },
    backlog: () => ws.bufferedAmount,
  });
  const start = now();
  const step = 1000 / hz;
  let next = start;
  const samples = [];
  await new Promise((done) => {
    const tick = () => {
      const t = now();
      while (next <= t) { // the helper's events, one every 1/hz s (late ones in a burst, as from a pipe)
        if (!n) oldest = next;
        n++;
        sent++;
        moves.add(1, 0);
        next += step;
      }
      const backlog = sent - acked;
      if (backlog > maxBacklog) maxBacklog = backlog;
      if (samples.length < Math.floor((t - start) / 1000)) samples.push(backlog);
      if (t - start < seconds * 1000) setImmediate(tick);
      else setTimeout(done, 500);
    };
    tick();
  });
  ws.close();
  delays.sort((a, b) => a - b);
  const q = (f) => delays[Math.min(delays.length - 1, Math.floor(f * delays.length))].toFixed(2);
  console.log(`${sent} moves in ${seconds} s, ${msgs} messages (${(perMsg / msgs).toFixed(2)} moves each)`);
  console.log(`delay ms: p50 ${q(0.5)}  p95 ${q(0.95)}  p99 ${q(0.99)}  max ${delays[delays.length - 1].toFixed(2)}`);
  console.log(`backlog (moves not yet written there): largest ${maxBacklog}, each second: ${samples.join(' ')}, at the end ${sent - acked}`);
  const ok = Number(q(0.95)) <= 16 && sent - acked === 0;
  console.log(ok ? 'movebench: ok (p95 ≤ 16 ms, nothing left behind)' : 'movebench: NOT ok');
  process.exit(ok ? 0 : 1);
}

if (args[0] === 'serve') serve();
else if (args[0] === 'send') send().catch((e) => { console.error(e.message); process.exit(1); });
else console.log('usage: movebench.js serve [port] [--helper cmd] | send host[:port] [seconds] [--hz 1000]');
