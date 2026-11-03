// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
'use strict';

const crypto = require('crypto');
const WebSocket = require('ws');

const DEFAULT_URL = process.env.GLIDEDESK_WS_URL || 'ws://127.0.0.1:47800';

function sha256Hex(str) {
  return crypto.createHash('sha256').update(str).digest('hex');
}

function commitOf(nonce) {
  return sha256Hex(nonce);
}

function sasCode(salt, a, b) {
  const hash = crypto.createHash('sha256').update(salt + a + b).digest('hex');
  const num = parseInt(hash.substring(0, 8), 16) % 1000000;
  return num.toString().padStart(6, '0');
}

function randomHex(bytes = 16) {
  return crypto.randomBytes(bytes).toString('hex');
}

class E2EClient {
  constructor(url = DEFAULT_URL) {
    this.url = url;
    this.ws = null;
    this.messages = [];
    this.waiters = [];
  }

  async connect(timeoutMs = 3000) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.ws) this.ws.terminate();
        reject(new Error(`Connection to ${this.url} timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      this.ws = new WebSocket(this.url);

      this.ws.on('open', () => {
        clearTimeout(timer);
        resolve();
      });

      this.ws.on('message', (data) => {
        try {
          const pkt = JSON.parse(data.toString());
          this.messages.push(pkt);

          // Check any active waiters
          for (let i = this.waiters.length - 1; i >= 0; i--) {
            const w = this.waiters[i];
            if (w.predicate(pkt)) {
              this.waiters.splice(i, 1);
              clearTimeout(w.timer);
              w.resolve(pkt);
            }
          }
        } catch (e) {
          // ignore non-json
        }
      });

      this.ws.on('error', (err) => {
        clearTimeout(timer);
        reject(err);
      });
    });
  }

  send(pkt) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket is not open');
    }
    this.ws.send(JSON.stringify(pkt));
  }

  async waitFor(predicate, timeoutMs = 2500) {
    // Check if message already arrived
    const existing = this.messages.find(predicate);
    if (existing) {
      return existing;
    }

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const idx = this.waiters.findIndex((w) => w.timer === timer);
        if (idx !== -1) this.waiters.splice(idx, 1);
        reject(new Error(`waitFor condition timed out after ${timeoutMs}ms (received ${this.messages.length} messages)`));
      }, timeoutMs);

      this.waiters.push({ predicate, resolve, timer });
    });
  }

  async collect(durationMs = 500) {
    const beforeCount = this.messages.length;
    await new Promise((r) => setTimeout(r, durationMs));
    return this.messages.slice(beforeCount);
  }

  close() {
    for (const w of this.waiters) {
      clearTimeout(w.timer);
    }
    this.waiters = [];
    if (this.ws) {
      try {
        this.ws.close();
      } catch (_) {}
      this.ws = null;
    }
  }
}

module.exports = {
  DEFAULT_URL,
  sha256Hex,
  commitOf,
  sasCode,
  randomHex,
  E2EClient,
};
