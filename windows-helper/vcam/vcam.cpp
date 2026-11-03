// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// twindesk-vcam.exe: the virtual camera's side in TwinDesk (windows/vcam.js).
//   status          {"camera":true|false}: is "TwinDesk Camera" among this PC's cameras
//   add             makes it (MFCreateVirtualCamera, for this user, kept until removed); needs
//                   twindesk-vcam.dll registered first (elevated, scripts/vcam-install.ps1)
//   remove          takes it away again
//   feed            frames on stdin, each VCAM_FRAME bytes of NV12, into the camera's shared memory
//   selftest <dll> [n]  loads the source from that file ("camera": the installed camera, through
//                   Frame Server) and reads n frames (15) from it
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <mfvirtualcamera.h>
#include <wrl/client.h>
#include <stdio.h>
#include <io.h>
#include <fcntl.h>
#include <initializer_list>
#include "vcam.h"

using Microsoft::WRL::ComPtr;

static int Fail(const char* what, HRESULT hr) {
  printf("{\"ok\":false,\"error\":\"%s (0x%08lX)\"}\n", what, (unsigned long)hr);
  return 1;
}

// "TwinDesk Camera" among this PC's cameras (Windows may add " (Windows Virtual Camera)").
static bool FindCamera(ComPtr<IMFActivate>* out = nullptr) {
  ComPtr<IMFAttributes> a;
  if (FAILED(MFCreateAttributes(&a, 1))) return false;
  a->SetGUID(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID);
  IMFActivate** list = nullptr;
  UINT32 n = 0;
  if (FAILED(MFEnumDeviceSources(a.Get(), &list, &n))) return false;
  bool found = false;
  for (UINT32 i = 0; i < n; i++) {
    wchar_t* name = nullptr;
    UINT32 len = 0;
    if (!found && SUCCEEDED(list[i]->GetAllocatedString(MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, &name, &len))) {
      if (!wcsncmp(name, VCAM_NAME, wcslen(VCAM_NAME))) {
        found = true;
        if (out) *out = list[i];
      }
      CoTaskMemFree(name);
    }
    list[i]->Release();
  }
  CoTaskMemFree(list);
  return found;
}

static HRESULT Camera(ComPtr<IMFVirtualCamera>& vc) {
  wchar_t clsid[64];
  StringFromGUID2(CLSID_TwinDeskVcam, clsid, 64);
  return MFCreateVirtualCamera(MFVirtualCameraType_SoftwareCameraSource, MFVirtualCameraLifetime_System,
    MFVirtualCameraAccess_CurrentUser, VCAM_NAME, clsid, nullptr, 0, &vc);
}

static int Add() {
  ComPtr<IMFVirtualCamera> vc;
  HRESULT hr = Camera(vc);
  if (FAILED(hr)) return Fail("MFCreateVirtualCamera", hr);
  hr = vc->Start(nullptr);
  if (FAILED(hr)) return Fail("Start", hr);
  vc->Shutdown();
  printf("{\"ok\":true}\n");
  return 0;
}

static int Remove() {
  ComPtr<IMFVirtualCamera> vc;
  HRESULT hr = Camera(vc);
  if (FAILED(hr)) return Fail("MFCreateVirtualCamera", hr);
  hr = vc->Remove();
  if (FAILED(hr)) return Fail("Remove", hr);
  vc->Shutdown();
  printf("{\"ok\":true}\n");
  return 0;
}

static bool ReadAll(HANDLE in, BYTE* p, DWORD n) {
  while (n) {
    DWORD got = 0;
    if (!ReadFile(in, p, n, &got, nullptr) || !got) return false;
    p += got;
    n -= got;
  }
  return true;
}

// The shared memory is the source's (it opens once an app opens the camera): until then, and
// after it's gone, frames are read and dropped and the mapping looked for again now and then.
static int Feed() {
  HANDLE in = GetStdHandle(STD_INPUT_HANDLE);
  BYTE* frame = new BYTE[VCAM_FRAME];
  HANDLE map = nullptr;
  BYTE* view = nullptr;
  ULONGLONG looked = 0;
  while (ReadAll(in, frame, VCAM_FRAME)) {
    ULONGLONG now = GetTickCount64();
    if (!view && now - looked > 1000) {
      looked = now;
      for (const wchar_t* ns : { L"Global\\", L"Local\\" }) {
        wchar_t name[96];
        swprintf_s(name, 96, L"%s%s", ns, VCAM_MAP);
        map = OpenFileMappingW(FILE_MAP_WRITE | FILE_MAP_READ, FALSE, name);
        if (map) break;
      }
      if (map) view = (BYTE*)MapViewOfFile(map, FILE_MAP_WRITE | FILE_MAP_READ, 0, 0, VCAM_MAP_BYTES);
      if (map && !view) { CloseHandle(map); map = nullptr; }
      if (view && ((VcamHeader*)view)->magic != VCAM_MAGIC) { UnmapViewOfFile(view); CloseHandle(map); view = nullptr; map = nullptr; }
      if (view) { fprintf(stderr, "feeding\n"); fflush(stderr); }
    }
    if (!view) continue;
    VcamHeader* h = (VcamHeader*)view;
    LONG64 s = h->seq;
    if (s & 1) s++;
    InterlockedExchange64(&h->seq, s + 1);
    memcpy(view + sizeof(VcamHeader), frame, VCAM_FRAME);
    InterlockedExchange64(&h->tick, (LONG64)now);
    InterlockedExchange64(&h->seq, s + 2);
  }
  return 0;
}

// dll "camera": the installed camera, as an app opens it (through Frame Server).
static int SelfTest(const wchar_t* dll, int count) {
  ComPtr<IMFActivate> act;
  HRESULT hr;
  if (!wcscmp(dll, L"camera")) {
    if (!FindCamera(&act)) return Fail("no TwinDesk Camera", E_FAIL);
  } else {
    HMODULE m = LoadLibraryW(dll);
    if (!m) return Fail("LoadLibrary", HRESULT_FROM_WIN32(GetLastError()));
    typedef HRESULT(__stdcall * GetClass)(REFCLSID, REFIID, void**);
    GetClass get = (GetClass)GetProcAddress(m, "DllGetClassObject");
    if (!get) return Fail("DllGetClassObject", E_NOINTERFACE);
    ComPtr<IClassFactory> f;
    hr = get(CLSID_TwinDeskVcam, IID_PPV_ARGS(&f));
    if (FAILED(hr)) return Fail("class", hr);
    hr = f->CreateInstance(nullptr, IID_PPV_ARGS(&act));
    if (FAILED(hr)) return Fail("CreateInstance", hr);
  }
  ComPtr<IMFMediaSource> src;
  hr = act->ActivateObject(IID_PPV_ARGS(&src));
  if (FAILED(hr)) return Fail("ActivateObject", hr);
  ComPtr<IMFSourceReader> reader;
  hr = MFCreateSourceReaderFromMediaSource(src.Get(), nullptr, &reader);
  if (FAILED(hr)) return Fail("MFCreateSourceReaderFromMediaSource", hr);
  ComPtr<IMFMediaType> type;
  hr = reader->GetCurrentMediaType((DWORD)MF_SOURCE_READER_FIRST_VIDEO_STREAM, &type);
  if (FAILED(hr)) return Fail("GetCurrentMediaType", hr);
  UINT32 w = 0, hgt = 0;
  MFGetAttributeSize(type.Get(), MF_MT_FRAME_SIZE, &w, &hgt);
  ULONGLONG t0 = GetTickCount64();
  int frames = 0;
  BYTE y0 = 0;
  for (int i = 0; i < count; i++) {
    DWORD stream = 0, flags = 0;
    LONGLONG ts = 0;
    ComPtr<IMFSample> s;
    hr = reader->ReadSample((DWORD)MF_SOURCE_READER_FIRST_VIDEO_STREAM, 0, &stream, &flags, &ts, &s);
    if (FAILED(hr)) return Fail("ReadSample", hr);
    if (!s) continue;
    ComPtr<IMFMediaBuffer> b;
    if (SUCCEEDED(s->ConvertToContiguousBuffer(&b))) {
      BYTE* p = nullptr;
      DWORD len = 0;
      if (SUCCEEDED(b->Lock(&p, nullptr, &len))) {
        if (len == VCAM_FRAME) { frames++; y0 = p[0]; }
        b->Unlock();
      }
    }
  }
  ULONGLONG ms = GetTickCount64() - t0;
  reader.Reset();
  act->ShutdownObject();
  printf("{\"ok\":%s,\"width\":%u,\"height\":%u,\"frames\":%d,\"ms\":%llu,\"y0\":%u}\n", frames >= count * 9 / 10 ? "true" : "false", w, hgt, frames, ms, y0);
  return frames >= count * 9 / 10 ? 0 : 1;
}

int wmain(int argc, wchar_t** argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: twindesk-vcam status|add|remove|feed|selftest <dll>\n");
    return 2;
  }
  CoInitializeEx(nullptr, COINIT_MULTITHREADED);
  MFStartup(MF_VERSION, MFSTARTUP_FULL);
  const wchar_t* cmd = argv[1];
  int r = 2;
  if (!wcscmp(cmd, L"status")) { printf("{\"ok\":true,\"camera\":%s}\n", FindCamera() ? "true" : "false"); r = 0; }
  else if (!wcscmp(cmd, L"add")) r = Add();
  else if (!wcscmp(cmd, L"remove")) r = Remove();
  else if (!wcscmp(cmd, L"feed")) r = Feed();
  else if (!wcscmp(cmd, L"selftest") && argc > 2) r = SelfTest(argv[2], argc > 3 ? _wtoi(argv[3]) : 15);
  fflush(stdout);
  MFShutdown();
  CoUninitialize();
  return r;
}
