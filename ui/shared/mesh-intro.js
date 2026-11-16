// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Multi-device (Mesh mode, beta) in the one-to-one app: the picture of a mesh on its tab
// (docs/multi-device.md).
//   MeshIntro.art()                 -> an element: devices, each one linked to every other
//
// There is no "turn it on" popup any more: the mesh is always loaded and stays dormant until a
// second device connects, so there is nothing to confirm and nothing to restart.
(function () {
  // Each device: [kind, x, y]. Lines join every pair, as the mesh does.
  const NODES = [['monitor', 110, 78], ['laptop', 320, 46], ['monitor', 530, 84], ['phone', 214, 196], ['laptop', 430, 200]];

  const device = (kind, x, y) => FeatureArt.device(kind, x, y); // shared/feature-art.js, loaded first

  function art() {
    const lines = [];
    const dots = [];
    for (let i = 0; i < NODES.length; i++) {
      for (let j = i + 1; j < NODES.length; j++) {
        const [, x1, y1] = NODES[i];
        const [, x2, y2] = NODES[j];
        lines.push(`<line class="mi-link" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`);
        // A copy or a pointer on its way: a dot running along some of the links.
        if ((i + j) % 2) {
          const dur = 2.4 + ((i * 3 + j) % 4) * 0.5;
          dots.push(`<circle class="mi-dot" r="3.5"><animateMotion dur="${dur}s" repeatCount="indefinite" path="M${x1} ${y1}L${x2} ${y2}"/></circle>`);
        }
      }
    }
    const wrap = document.createElement('div');
    wrap.className = 'mesh-art';
    wrap.setAttribute('aria-hidden', 'true');
    wrap.innerHTML = `<svg viewBox="0 0 640 250" role="img">${lines.join('')}${dots.join('')}${NODES.map((n) => device(...n)).join('')}</svg>`
      + '<p>Every device linked to every other: one clipboard and one set of notifications for all, and the mouse and keyboard across all of them.</p>';
    return wrap;
  }

  window.MeshIntro = { art };
})();
