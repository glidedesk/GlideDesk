// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Window Edge-Handoff: Client-side projection renderer & low-latency input forwarder.

(function () {
  'use strict';

  let currentWindowId = null;
  let currentTitle = 'Remote Window';
  let targetWidth = 1024;
  let targetHeight = 768;

  const titleEl = document.getElementById('projection-title');
  const btnReturn = document.getElementById('btn-return');
  const btnClose = document.getElementById('btn-close');
  const canvas = document.getElementById('projection-canvas');
  const overlay = document.getElementById('projection-overlay');
  const ctx = canvas.getContext('2d');

  let lastX = 0;
  let lastY = 0;
  let isFocused = false;

  function updateProjection(data) {
    if (!data) return;
    currentWindowId = data.id || currentWindowId;
    currentTitle = data.title || currentTitle;
    targetWidth = data.width || targetWidth;
    targetHeight = data.height || targetHeight;

    if (titleEl) titleEl.textContent = currentTitle;
    document.title = `GlideDesk — ${currentTitle}`;

    canvas.width = targetWidth;
    canvas.height = targetHeight;

    renderCanvasPlaceholder();
    if (overlay) {
      overlay.classList.add('hidden');
    }
  }

  function renderCanvasPlaceholder() {
    if (!ctx) return;
    // Dark background with subtle grid
    ctx.fillStyle = '#0f172a';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    ctx.strokeStyle = '#1e293b';
    ctx.lineWidth = 1;
    const step = 40;
    for (let x = 0; x < canvas.width; x += step) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, canvas.height);
      ctx.stroke();
    }
    for (let y = 0; y < canvas.height; y += step) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(canvas.width, y);
      ctx.stroke();
    }

    // App title card in center
    ctx.fillStyle = '#f8fafc';
    ctx.font = 'bold 24px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText(currentTitle, canvas.width / 2, canvas.height / 2 - 20);

    ctx.fillStyle = '#94a3b8';
    ctx.font = '14px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    ctx.fillText(`${targetWidth} × ${targetHeight} • Active Single-Window Handoff`, canvas.width / 2, canvas.height / 2 + 15);
    ctx.fillText('Mouse and keyboard inputs forwarded to source machine', canvas.width / 2, canvas.height / 2 + 40);
  }

  function returnToSource() {
    if (currentWindowId && window.macshare && window.macshare.returnWindow) {
      window.macshare.returnWindow(currentWindowId).catch(console.error);
    }
    if (window.macshare && window.macshare.hideProjectionWindow) {
      window.macshare.hideProjectionWindow().catch(console.error);
    }
  }

  if (btnReturn) btnReturn.addEventListener('click', returnToSource);
  if (btnClose) btnClose.addEventListener('click', returnToSource);

  // Setup input forwarding
  canvas.addEventListener('mouseenter', (e) => {
    isFocused = true;
    lastX = e.clientX;
    lastY = e.clientY;
  });

  canvas.addEventListener('mouseleave', () => {
    isFocused = false;
  });

  canvas.addEventListener('mousemove', (e) => {
    if (!isFocused || !window.macshare || !window.macshare.sendWireMessage) return;
    const dx = e.clientX - lastX;
    const dy = e.clientY - lastY;
    lastX = e.clientX;
    lastY = e.clientY;

    if (dx !== 0 || dy !== 0) {
      window.macshare.sendWireMessage({
        t: 'k.mm',
        dx: dx,
        dy: dy,
      }).catch(() => {});
    }
  });

  canvas.addEventListener('mousedown', (e) => {
    if (!window.macshare || !window.macshare.sendWireMessage) return;
    const btn = e.button === 0 ? 0 : e.button === 2 ? 1 : 2;
    window.macshare.sendWireMessage({
      t: 'k.md',
      b: btn,
    }).catch(() => {});
  });

  canvas.addEventListener('mouseup', (e) => {
    if (!window.macshare || !window.macshare.sendWireMessage) return;
    const btn = e.button === 0 ? 0 : e.button === 2 ? 1 : 2;
    window.macshare.sendWireMessage({
      t: 'k.mu',
      b: btn,
    }).catch(() => {});
  });

  canvas.addEventListener('wheel', (e) => {
    if (!window.macshare || !window.macshare.sendWireMessage) return;
    e.preventDefault();
    window.macshare.sendWireMessage({
      t: 'k.wh',
      dx: -e.deltaX,
      dy: -e.deltaY,
    }).catch(() => {});
  }, { passive: false });

  canvas.addEventListener('contextmenu', (e) => {
    e.preventDefault();
  });

  window.addEventListener('keydown', (e) => {
    if (!isFocused || !window.macshare || !window.macshare.sendWireMessage) return;
    window.macshare.sendWireMessage({
      t: 'k.kd',
      vk: e.keyCode,
    }).catch(() => {});
  });

  window.addEventListener('keyup', (e) => {
    if (!isFocused || !window.macshare || !window.macshare.sendWireMessage) return;
    window.macshare.sendWireMessage({
      t: 'k.ku',
      vk: e.keyCode,
    }).catch(() => {});
  });

  // URL query fallback initialization
  const params = new URLSearchParams(window.location.search);
  if (params.has('id')) {
    updateProjection({
      id: params.get('id'),
      title: params.get('title') || 'Remote Window',
      width: parseInt(params.get('w') || '1024', 10),
      height: parseInt(params.get('h') || '768', 10),
    });
  }

  // Register Tauri event listeners
  if (window.macshare) {
    if (window.macshare.onProjectionSetup) {
      window.macshare.onProjectionSetup((payload) => updateProjection(payload));
    }
    if (window.macshare.onWindowHandoffOffer) {
      window.macshare.onWindowHandoffOffer((payload) => updateProjection(payload));
    }
    if (window.macshare.onWindowHandoffReturn) {
      window.macshare.onWindowHandoffReturn((payload) => {
        if (!payload || payload.id === currentWindowId) {
          if (window.macshare.hideProjectionWindow) {
            window.macshare.hideProjectionWindow();
          }
        }
      });
    }
  }
})();
