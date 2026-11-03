// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tests for cursor tab layout resolution, cell placement, opposite layout mapping, and hint text.
// Run with: node ui/shared/test/ui-cursor-layout.js
'use strict';

const assert = require('assert');
const path = require('path');
const {
  MAC_SIDE,
  CELL,
  SIDE_OF,
  resolveSide,
  calculateOppositeSide,
  generateLayoutHint,
} = require(path.resolve(__dirname, '../../ui-cursor.js'));

function getGridCells(side, peerName) {
  return [0, 1, 2, 3, 4, 5, 6, 7, 8].map((i) => {
    if (i === 4) return { i, role: 'me', text: '{me}', isPeer: false };
    const here = side && CELL[side] === i;
    const isLayoutCell = Object.values(CELL).includes(i);
    return {
      i,
      role: isLayoutCell ? SIDE_OF[i] : 'empty',
      className: isLayoutCell ? 'cell' + (here ? ' mac-here' : '') : '',
      text: here ? peerName : '',
      isPeer: !!here,
    };
  });
}

// 1. Test MAC_SIDE opposite mapping
{
  assert.strictEqual(MAC_SIDE['left'], 'right');
  assert.strictEqual(MAC_SIDE['right'], 'left');
  assert.strictEqual(MAC_SIDE['top'], 'bottom');
  assert.strictEqual(MAC_SIDE['bottom'], 'top');
  assert.strictEqual(MAC_SIDE['unknown'], undefined);

  assert.strictEqual(calculateOppositeSide('right'), 'left');
  assert.strictEqual(calculateOppositeSide('left'), 'right');
  assert.strictEqual(calculateOppositeSide('top'), 'bottom');
  assert.strictEqual(calculateOppositeSide('bottom'), 'top');
  assert.strictEqual(calculateOppositeSide('invalid'), 'left');
}

// 2. Test resolveSide logic under all state combinations
{
  // Normal state with layout.side
  assert.strictEqual(resolveSide({ layout: { side: 'right' } }), 'right');
  assert.strictEqual(resolveSide({ layout: { side: 'left' } }), 'left');
  assert.strictEqual(resolveSide({ layout: { side: 'top' } }), 'top');
  assert.strictEqual(resolveSide({ layout: { side: 'bottom' } }), 'bottom');

  // Fallback to legacy pcSide
  assert.strictEqual(resolveSide({ kvm: { pcSide: 'left' } }), 'right');
  assert.strictEqual(resolveSide({ kvm: { pcSide: 'right' } }), 'left');

  // Fallback default when layout and kvm are empty
  assert.strictEqual(resolveSide({}), 'right');
  assert.strictEqual(resolveSide(null), 'right');
  assert.strictEqual(resolveSide(undefined), 'right');
  assert.strictEqual(resolveSide({ layout: { side: '' } }), 'right');
  assert.strictEqual(resolveSide({ layout: null, kvm: { pcSide: null } }), 'right');
  assert.strictEqual(resolveSide({ layout: null, kvm: { pcSide: 'invalid_side' } }), 'right');
}

// 3. Test grid cell generation and peer placement
{
  const rightGrid = getGridCells('right', 'VASANTH');
  assert.strictEqual(rightGrid[5].className, 'cell mac-here');
  assert.strictEqual(rightGrid[5].text, 'VASANTH');
  assert.strictEqual(rightGrid[5].isPeer, true);
  assert.strictEqual(rightGrid[3].className, 'cell');
  assert.strictEqual(rightGrid[3].text, '');
  assert.strictEqual(rightGrid[3].isPeer, false);

  const leftGrid = getGridCells('left', 'VASANTH');
  assert.strictEqual(leftGrid[3].className, 'cell mac-here');
  assert.strictEqual(leftGrid[3].text, 'VASANTH');
  assert.strictEqual(leftGrid[3].isPeer, true);
  assert.strictEqual(leftGrid[5].className, 'cell');
  assert.strictEqual(leftGrid[5].text, '');
  assert.strictEqual(leftGrid[5].isPeer, false);

  const topGrid = getGridCells('top', 'VASANTH');
  assert.strictEqual(topGrid[1].className, 'cell mac-here');
  assert.strictEqual(topGrid[1].text, 'VASANTH');
  assert.strictEqual(topGrid[1].isPeer, true);

  const bottomGrid = getGridCells('bottom', 'VASANTH');
  assert.strictEqual(bottomGrid[7].className, 'cell mac-here');
  assert.strictEqual(bottomGrid[7].text, 'VASANTH');
  assert.strictEqual(bottomGrid[7].isPeer, true);
}

// 4. Test layout hint generation
{
  assert.strictEqual(
    generateLayoutHint('right', 'VASANTH'),
    'VASANTH is to the right of {me}. Click a side to move it.'
  );
  assert.strictEqual(
    generateLayoutHint('left', 'VASANTH'),
    'VASANTH is to the left of {me}. Click a side to move it.'
  );
  assert.strictEqual(
    generateLayoutHint('top', 'Developer’s Mac mini'),
    'Developer’s Mac mini is above {me}. Click a side to move it.'
  );
  assert.strictEqual(
    generateLayoutHint('bottom', 'Developer’s Mac mini'),
    'Developer’s Mac mini is below {me}. Click a side to move it.'
  );
}

console.log('ui-cursor-layout: passed');
