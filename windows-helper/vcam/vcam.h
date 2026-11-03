// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// The virtual camera (Windows 11, MFCreateVirtualCamera): what twindesk-vcam.dll (the camera's
// media source, run by Windows' Frame Server) and twindesk-vcam.exe (adds it, feeds it) share.
// The picture goes through shared memory: the source makes it (Frame Server is a service, so it
// may make a Global\ one), the exe writes each frame TwinDesk sends it there, and the source hands
// apps the newest one 30 times a second, or a black picture while none comes.
#pragma once
#include <windows.h>

#define VCAM_W 1280
#define VCAM_H 720
#define VCAM_FPS 30
#define VCAM_FRAME (VCAM_W * VCAM_H * 3 / 2) // NV12
#define VCAM_NAME L"TwinDesk Camera"
#define VCAM_MAP L"TwinDeskVirtualCamera" // under Global\ (Frame Server) or Local\ (the self test)
#define VCAM_MAGIC 0x43564454 // 'TDVC'
#define VCAM_STALE_MS 1500 // no new frame for this long: black

// {36D4122A-6D38-4C6A-B622-E9DD756F6C5D}
static const GUID CLSID_TwinDeskVcam = { 0x36d4122a, 0x6d38, 0x4c6a, { 0xb6, 0x22, 0xe9, 0xdd, 0x75, 0x6f, 0x6c, 0x5d } };

// A seqlock: the writer makes seq odd, copies the frame, makes it even again; a reader that saw
// it odd, or changed by the end of its copy, keeps the frame it had.
struct VcamHeader {
  UINT32 magic;
  UINT32 width, height, frameBytes;
  volatile LONG64 seq;
  volatile LONG64 tick; // GetTickCount64 of the last frame
  UINT32 reserved[8];
};
#define VCAM_MAP_BYTES (sizeof(VcamHeader) + VCAM_FRAME)
