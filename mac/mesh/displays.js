// TwinDesk — Copyright (C) 2026 inukaasith, Erdoğan Yeşil. GPL-3.0, see LICENSE.
// ../displays.js and what screen sharing per device needs of it (streams.js, shared/screen-mesh.js).
'use strict';

const { Displays } = require('../displays');

class MeshDisplays extends Displays {
  byId(id) {
    return this.list().find((d) => d.id === Number(id)) || null;
  }

  virtualTarget() {
    return this.list().find((d) => d.virtual) || null;
  }

  // This Mac's displays as the screen-mesh model (shared/screen-mesh.js): physical pixels, positions
  // relative to the main display (negative x and y are normal).
  model() {
    const all = this.list();
    const main = all.find((d) => d.primary) || all[0];
    if (!main) return [];
    return all.map((d) => {
      const k = d.width ? d.pixelWidth / d.width : 1;
      return { id: String(d.id), name: d.label, x: Math.round((d.bounds.x - main.bounds.x) * k), y: Math.round((d.bounds.y - main.bounds.y) * k), w: d.pixelWidth, h: d.pixelHeight, scale: k, primary: d.primary };
    });
  }
}

module.exports = { Displays: MeshDisplays };
