// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
'use strict';

const assert = require('assert');
const { Session, TYPE } = require('../../windows/scrcpy');

// 1. scrcpy 5.0.1's ControlMessage codes (TYPE_CAMERA_SET_TORCH 18, ZOOM_IN 19, ZOOM_OUT 20)
assert.strictEqual(TYPE.CAMERA_SET_TORCH, 18, 'CAMERA_SET_TORCH should be 18');
assert.strictEqual(TYPE.CAMERA_ZOOM_IN, 19, 'CAMERA_ZOOM_IN should be 19');
assert.strictEqual(TYPE.CAMERA_ZOOM_OUT, 20, 'CAMERA_ZOOM_OUT should be 20');

// 2. Verify Session.parseCameras
// As scrcpy-server 5.0.1 prints it (LogUtils.buildCameraListMessage).
const sampleOutput = `
[server] INFO: Device: Google Pixel 8 (Android 14)
[server] INFO: List of cameras:
    --camera-id=0    (back, 4000x3000, fps=[15, 30])
    --camera-id=1    (front, 2560x1440, fps=[15, 30, 60])
    --camera-id=2    (back, 1920x1080, fps=[30])
`;

const cameras = Session.parseCameras(sampleOutput);
assert.strictEqual(cameras.length, 3, 'Should discover 3 cameras');
assert.strictEqual(cameras[0].facing, 'back');
assert.strictEqual(cameras[0].id, '0');
assert.deepStrictEqual(cameras[0].sizes, ['4000x3000']);

assert.strictEqual(cameras[1].facing, 'front');
assert.strictEqual(cameras[1].id, '1');
assert.deepStrictEqual(cameras[1].sizes, ['2560x1440']);

assert.strictEqual(cameras[2].facing, 'back');
assert.strictEqual(cameras[2].id, '2');
assert.deepStrictEqual(cameras[2].sizes, ['1920x1080']);

// 3. Verify Session control message buffers
const s = new Session({}, 'mock_serial', {
  videoSource: 'camera',
  cameraFacing: 'front',
  cameraSize: '1920x1080',
  cameraFps: 60,
});

assert.strictEqual(s.opts.videoSource, 'camera');
assert.strictEqual(s.opts.cameraFacing, 'front');
assert.strictEqual(s.opts.cameraSize, '1920x1080');
assert.strictEqual(s.opts.cameraFps, 60);

const sent = [];
s._send = (buf) => sent.push(buf);

s.setTorch(true);
assert.strictEqual(sent.length, 1);
assert.deepStrictEqual(sent[0], Buffer.from([18, 1]));

s.setTorch(false);
assert.strictEqual(sent.length, 2);
assert.deepStrictEqual(sent[1], Buffer.from([18, 0]));

s.zoomIn();
assert.strictEqual(sent.length, 3);
assert.deepStrictEqual(sent[2], Buffer.from([19]));

s.zoomOut();
assert.strictEqual(sent.length, 4);
assert.deepStrictEqual(sent[3], Buffer.from([20]));

console.log('scrcpy-camera tests: passed (all assertions verified)');
