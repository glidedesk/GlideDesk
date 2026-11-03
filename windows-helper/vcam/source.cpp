// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// twindesk-vcam.dll: the virtual camera's media source. Windows' Frame Server makes it through
// its COM class (CLSID_TwinDeskVcam, an IMFActivate) and hands its pictures to every app that
// picks "TwinDesk Camera": one stream, NV12 1280×720 at 30 fps, made from the newest frame
// TwinDesk put in shared memory (vcam.h), black while there's none.
// Registered once, elevated (regsvr32, windows/scripts/vcam-install.ps1): HKLM, because Frame
// Server runs as Local Service and doesn't see the user's own classes.
#include <windows.h>
#include <sddl.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mferror.h>
#include <ks.h>
#include <ksproxy.h>
#include <ksmedia.h>
#include <wrl/client.h>
#include <deque>
#include <mutex>
#include <condition_variable>
#include <thread>
#include <new>
#include <stdio.h>
#include "vcam.h"

using Microsoft::WRL::ComPtr;

static HMODULE g_module;
static volatile LONG g_objects;

// PINNAME_VIDEO_CAPTURE (= PINNAME_CAPTURE)
static const GUID PIN_CAPTURE = { 0xfb6c4281, 0x0353, 0x11d1, { 0x90, 0x5f, 0x00, 0x00, 0xc0, 0xcc, 0x16, 0xba } };

// ---- the frames TwinDesk writes ----

class FrameMap {
public:
  FrameMap() {
    // Frame Server may make a Global\ one, for TwinDesk in the user's session; elsewhere (the self
    // test in one process) a Local\ one.
    SECURITY_ATTRIBUTES sa = { sizeof(sa), nullptr, FALSE };
    PSECURITY_DESCRIPTOR sd = nullptr;
    // SYSTEM, administrators, Local Service (Frame Server) and the signed-in user, at any integrity.
    if (ConvertStringSecurityDescriptorToSecurityDescriptorW(L"D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;LS)(A;;GA;;;IU)S:(ML;;NW;;;LW)", SDDL_REVISION_1, &sd, nullptr))
      sa.lpSecurityDescriptor = sd;
    for (const wchar_t* ns : { L"Global\\", L"Local\\" }) {
      wchar_t name[96];
      swprintf_s(name, 96, L"%s%s", ns, VCAM_MAP);
      map_ = CreateFileMappingW(INVALID_HANDLE_VALUE, &sa, PAGE_READWRITE, 0, (DWORD)VCAM_MAP_BYTES, name);
      if (map_) break;
    }
    if (sd) LocalFree(sd);
    if (map_) view_ = (BYTE*)MapViewOfFile(map_, FILE_MAP_ALL_ACCESS, 0, 0, VCAM_MAP_BYTES);
    if (view_) {
      VcamHeader* h = (VcamHeader*)view_;
      h->magic = VCAM_MAGIC;
      h->width = VCAM_W;
      h->height = VCAM_H;
      h->frameBytes = VCAM_FRAME;
    }
  }
  ~FrameMap() {
    if (view_) UnmapViewOfFile(view_);
    if (map_) CloseHandle(map_);
  }
  // The newest frame into out (VCAM_FRAME bytes), or black while none is fresh. last: what out had.
  void Read(BYTE* out, LONG64& last) {
    VcamHeader* h = view_ ? (VcamHeader*)view_ : nullptr;
    if (h && GetTickCount64() - (ULONGLONG)h->tick < VCAM_STALE_MS) {
      LONG64 s = h->seq;
      if (s == last) return; // the same frame: out already has it
      if (!(s & 1)) {
        memcpy(out, view_ + sizeof(VcamHeader), VCAM_FRAME);
        MemoryBarrier();
        if (h->seq == s) { last = s; return; }
      }
      if (last > 0) return; // torn: the last good one stays
    }
    if (last != -1) {
      memset(out, 16, VCAM_W * VCAM_H);
      memset(out + VCAM_W * VCAM_H, 128, VCAM_W * VCAM_H / 2);
      last = -1;
    }
  }
private:
  HANDLE map_ = nullptr;
  BYTE* view_ = nullptr;
};

class Source;

// ---- the stream ----

class Stream : public IMFMediaStream2 {
public:
  Stream(Source* source, IMFStreamDescriptor* sd) : source_(source), sd_(sd) {
    InterlockedIncrement(&g_objects);
    MFCreateEventQueue(&queue_);
    frame_ = new (std::nothrow) BYTE[VCAM_FRAME];
    worker_ = std::thread([this] { Run(); });
  }
  virtual ~Stream() {
    InterlockedDecrement(&g_objects);
    delete[] frame_;
  }

  // IUnknown
  STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
    if (!ppv) return E_POINTER;
    if (riid == __uuidof(IUnknown) || riid == __uuidof(IMFMediaEventGenerator) || riid == __uuidof(IMFMediaStream) || riid == __uuidof(IMFMediaStream2))
      *ppv = static_cast<IMFMediaStream2*>(this);
    else { *ppv = nullptr; return E_NOINTERFACE; }
    AddRef();
    return S_OK;
  }
  STDMETHODIMP_(ULONG) AddRef() override { return InterlockedIncrement(&refs_); }
  STDMETHODIMP_(ULONG) Release() override {
    ULONG n = InterlockedDecrement(&refs_);
    if (!n) delete this;
    return n;
  }

  // IMFMediaEventGenerator
  STDMETHODIMP GetEvent(DWORD flags, IMFMediaEvent** e) override {
    ComPtr<IMFMediaEventQueue> q;
    { std::lock_guard<std::mutex> l(m_); if (!queue_) return MF_E_SHUTDOWN; q = queue_; }
    return q->GetEvent(flags, e);
  }
  STDMETHODIMP BeginGetEvent(IMFAsyncCallback* cb, IUnknown* state) override {
    std::lock_guard<std::mutex> l(m_);
    return queue_ ? queue_->BeginGetEvent(cb, state) : MF_E_SHUTDOWN;
  }
  STDMETHODIMP EndGetEvent(IMFAsyncResult* r, IMFMediaEvent** e) override {
    std::lock_guard<std::mutex> l(m_);
    return queue_ ? queue_->EndGetEvent(r, e) : MF_E_SHUTDOWN;
  }
  STDMETHODIMP QueueEvent(MediaEventType t, REFGUID ext, HRESULT hr, const PROPVARIANT* v) override {
    std::lock_guard<std::mutex> l(m_);
    return queue_ ? queue_->QueueEventParamVar(t, ext, hr, v) : MF_E_SHUTDOWN;
  }

  // IMFMediaStream
  STDMETHODIMP GetMediaSource(IMFMediaSource** s) override;
  STDMETHODIMP GetStreamDescriptor(IMFStreamDescriptor** sd) override {
    if (!sd) return E_POINTER;
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    return sd_.CopyTo(sd);
  }
  STDMETHODIMP RequestSample(IUnknown* token) override {
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    if (state_ != MF_STREAM_STATE_RUNNING) return MF_E_MEDIA_SOURCE_WRONGSTATE;
    tokens_.push_back(ComPtr<IUnknown>(token));
    cv_.notify_all();
    return S_OK;
  }

  // IMFMediaStream2
  STDMETHODIMP SetStreamState(MF_STREAM_STATE s) override {
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    if (s == state_) return S_OK;
    state_ = s;
    if (s == MF_STREAM_STATE_RUNNING) {
      queue_->QueueEventParamVar(MEStreamStarted, GUID_NULL, S_OK, nullptr);
    } else {
      tokens_.clear();
      queue_->QueueEventParamVar(s == MF_STREAM_STATE_PAUSED ? MEStreamPaused : MEStreamStopped, GUID_NULL, S_OK, nullptr);
    }
    cv_.notify_all();
    return S_OK;
  }
  STDMETHODIMP GetStreamState(MF_STREAM_STATE* s) override {
    if (!s) return E_POINTER;
    std::lock_guard<std::mutex> l(m_);
    *s = state_;
    return S_OK;
  }

  void Shutdown() {
    {
      std::lock_guard<std::mutex> l(m_);
      if (!queue_) return;
      queue_->Shutdown();
      queue_.Reset();
      tokens_.clear();
      stop_ = true;
      cv_.notify_all();
    }
    if (worker_.joinable()) {
      if (worker_.get_id() == std::this_thread::get_id()) worker_.detach();
      else worker_.join();
    }
    source_ = nullptr;
  }

private:
  // One sample per request, no faster than the frame rate.
  void Run() {
    const LONGLONG period = 10000000LL / VCAM_FPS;
    LONGLONG next = MFGetSystemTime();
    LONG64 last = 0;
    FrameMap frames;
    for (;;) {
      ComPtr<IUnknown> token;
      ComPtr<IMFMediaEventQueue> q;
      {
        std::unique_lock<std::mutex> l(m_);
        cv_.wait(l, [this] { return stop_ || (state_ == MF_STREAM_STATE_RUNNING && !tokens_.empty()); });
        if (stop_) return;
      }
      LONGLONG now = MFGetSystemTime();
      if (now < next) Sleep((DWORD)((next - now) / 10000));
      now = MFGetSystemTime();
      next = (now - next > period) ? now + period : next + period;
      {
        std::lock_guard<std::mutex> l(m_);
        if (stop_) return;
        if (state_ != MF_STREAM_STATE_RUNNING || tokens_.empty()) continue;
        token = tokens_.front();
        tokens_.pop_front();
        q = queue_;
      }
      if (!frame_ || !q) continue;
      frames.Read(frame_, last);
      ComPtr<IMFSample> sample;
      ComPtr<IMFMediaBuffer> buf;
      BYTE* p = nullptr;
      if (FAILED(MFCreateMemoryBuffer(VCAM_FRAME, &buf)) || FAILED(buf->Lock(&p, nullptr, nullptr))) continue;
      memcpy(p, frame_, VCAM_FRAME);
      buf->Unlock();
      buf->SetCurrentLength(VCAM_FRAME);
      if (FAILED(MFCreateSample(&sample))) continue;
      sample->AddBuffer(buf.Get());
      sample->SetSampleTime(now);
      sample->SetSampleDuration(period);
      sample->SetUINT32(MFSampleExtension_CleanPoint, TRUE);
      if (token) sample->SetUnknown(MFSampleExtension_Token, token.Get());
      q->QueueEventParamUnk(MEMediaSample, GUID_NULL, S_OK, sample.Get());
    }
  }

  volatile ULONG refs_ = 1;
  Source* source_; // weak: the source owns the stream
  ComPtr<IMFStreamDescriptor> sd_;
  ComPtr<IMFMediaEventQueue> queue_;
  MF_STREAM_STATE state_ = MF_STREAM_STATE_STOPPED;
  std::deque<ComPtr<IUnknown>> tokens_;
  std::mutex m_;
  std::condition_variable cv_;
  bool stop_ = false;
  BYTE* frame_ = nullptr;
  std::thread worker_;
};

// ---- the source ----

class Source : public IMFMediaSourceEx, public IMFGetService, public IKsControl {
public:
  static HRESULT Create(Source** out) {
    Source* s = new (std::nothrow) Source();
    if (!s) return E_OUTOFMEMORY;
    HRESULT hr = s->Init();
    if (FAILED(hr)) { s->Release(); return hr; }
    *out = s;
    return S_OK;
  }

  // IUnknown
  STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
    if (!ppv) return E_POINTER;
    if (riid == __uuidof(IUnknown) || riid == __uuidof(IMFMediaEventGenerator) || riid == __uuidof(IMFMediaSource) || riid == __uuidof(IMFMediaSourceEx))
      *ppv = static_cast<IMFMediaSourceEx*>(this);
    else if (riid == __uuidof(IMFGetService)) *ppv = static_cast<IMFGetService*>(this);
    else if (riid == __uuidof(IKsControl)) *ppv = static_cast<IKsControl*>(this);
    else { *ppv = nullptr; return E_NOINTERFACE; }
    AddRef();
    return S_OK;
  }
  STDMETHODIMP_(ULONG) AddRef() override { return InterlockedIncrement(&refs_); }
  STDMETHODIMP_(ULONG) Release() override {
    ULONG n = InterlockedDecrement(&refs_);
    if (!n) delete this;
    return n;
  }

  // IMFMediaEventGenerator
  STDMETHODIMP GetEvent(DWORD flags, IMFMediaEvent** e) override {
    ComPtr<IMFMediaEventQueue> q;
    { std::lock_guard<std::mutex> l(m_); if (!queue_) return MF_E_SHUTDOWN; q = queue_; }
    return q->GetEvent(flags, e);
  }
  STDMETHODIMP BeginGetEvent(IMFAsyncCallback* cb, IUnknown* state) override {
    std::lock_guard<std::mutex> l(m_);
    return queue_ ? queue_->BeginGetEvent(cb, state) : MF_E_SHUTDOWN;
  }
  STDMETHODIMP EndGetEvent(IMFAsyncResult* r, IMFMediaEvent** e) override {
    std::lock_guard<std::mutex> l(m_);
    return queue_ ? queue_->EndGetEvent(r, e) : MF_E_SHUTDOWN;
  }
  STDMETHODIMP QueueEvent(MediaEventType t, REFGUID ext, HRESULT hr, const PROPVARIANT* v) override {
    std::lock_guard<std::mutex> l(m_);
    return queue_ ? queue_->QueueEventParamVar(t, ext, hr, v) : MF_E_SHUTDOWN;
  }

  // IMFMediaSource
  STDMETHODIMP GetCharacteristics(DWORD* c) override {
    if (!c) return E_POINTER;
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    *c = MFMEDIASOURCE_IS_LIVE;
    return S_OK;
  }
  STDMETHODIMP CreatePresentationDescriptor(IMFPresentationDescriptor** pd) override {
    if (!pd) return E_POINTER;
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    return pd_->Clone(pd);
  }
  STDMETHODIMP Start(IMFPresentationDescriptor* pd, const GUID* format, const PROPVARIANT* start) override {
    if (!pd || !start) return E_INVALIDARG;
    if (format && *format != GUID_NULL) return MF_E_UNSUPPORTED_TIME_FORMAT;
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    DWORD count = 0;
    HRESULT hr = pd->GetStreamDescriptorCount(&count);
    if (FAILED(hr)) return hr;
    for (DWORD i = 0; i < count; i++) {
      BOOL selected = FALSE;
      ComPtr<IMFStreamDescriptor> sd;
      if (FAILED(pd->GetStreamDescriptorByIndex(i, &selected, &sd))) continue;
      DWORD id = 0;
      sd->GetStreamIdentifier(&id);
      if (id != 0) continue;
      // The picture's format the app picked (one is offered, but it may set it again).
      ComPtr<IMFMediaTypeHandler> handler;
      ComPtr<IMFMediaType> type;
      if (SUCCEEDED(sd->GetMediaTypeHandler(&handler)) && SUCCEEDED(handler->GetCurrentMediaType(&type))) {
        ComPtr<IMFMediaTypeHandler> ours;
        if (SUCCEEDED(sd_->GetMediaTypeHandler(&ours))) ours->SetCurrentMediaType(type.Get());
      }
      if (selected) {
        pd_->SelectStream(0);
        queue_->QueueEventParamUnk(started_ ? MEUpdatedStream : MENewStream, GUID_NULL, S_OK, static_cast<IMFMediaStream2*>(stream_.Get()));
        stream_->SetStreamState(MF_STREAM_STATE_RUNNING);
      } else {
        pd_->DeselectStream(0);
        stream_->SetStreamState(MF_STREAM_STATE_STOPPED);
      }
    }
    started_ = true;
    PROPVARIANT t;
    PropVariantInit(&t);
    t.vt = VT_I8;
    t.hVal.QuadPart = MFGetSystemTime();
    return queue_->QueueEventParamVar(MESourceStarted, GUID_NULL, S_OK, &t);
  }
  STDMETHODIMP Stop() override {
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    stream_->SetStreamState(MF_STREAM_STATE_STOPPED);
    return queue_->QueueEventParamVar(MESourceStopped, GUID_NULL, S_OK, nullptr);
  }
  STDMETHODIMP Pause() override { return MF_E_INVALID_STATE_TRANSITION; } // live
  STDMETHODIMP Shutdown() override {
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    queue_->Shutdown();
    queue_.Reset();
    if (stream_) stream_->Shutdown();
    return S_OK;
  }

  // IMFMediaSourceEx
  STDMETHODIMP GetSourceAttributes(IMFAttributes** a) override {
    if (!a) return E_POINTER;
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    return attrs_.CopyTo(a);
  }
  STDMETHODIMP GetStreamAttributes(DWORD id, IMFAttributes** a) override {
    if (!a) return E_POINTER;
    if (id != 0) return MF_E_INVALIDSTREAMNUMBER;
    std::lock_guard<std::mutex> l(m_);
    if (!queue_) return MF_E_SHUTDOWN;
    return sd_.CopyTo(IID_PPV_ARGS(a));
  }
  STDMETHODIMP SetD3DManager(IUnknown*) override { return S_OK; } // system memory only

  // IMFGetService
  STDMETHODIMP GetService(REFGUID, REFIID, LPVOID*) override { return MF_E_UNSUPPORTED_SERVICE; }

  // IKsControl: no camera controls (focus, exposure, …).
  STDMETHODIMP KsProperty(PKSPROPERTY, ULONG, LPVOID, ULONG, ULONG*) override { return HRESULT_FROM_WIN32(ERROR_SET_NOT_FOUND); }
  STDMETHODIMP KsMethod(PKSMETHOD, ULONG, LPVOID, ULONG, ULONG*) override { return HRESULT_FROM_WIN32(ERROR_SET_NOT_FOUND); }
  STDMETHODIMP KsEvent(PKSEVENT, ULONG, LPVOID, ULONG, ULONG*) override { return HRESULT_FROM_WIN32(ERROR_SET_NOT_FOUND); }

private:
  Source() { InterlockedIncrement(&g_objects); }
  virtual ~Source() {
    if (stream_) stream_->Shutdown();
    InterlockedDecrement(&g_objects);
  }

  HRESULT Init() {
    HRESULT hr = MFCreateEventQueue(&queue_);
    if (FAILED(hr)) return hr;
    hr = MFCreateAttributes(&attrs_, 4);
    if (FAILED(hr)) return hr;
    ComPtr<IMFMediaType> type;
    hr = MFCreateMediaType(&type);
    if (FAILED(hr)) return hr;
    type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_NV12);
    MFSetAttributeSize(type.Get(), MF_MT_FRAME_SIZE, VCAM_W, VCAM_H);
    MFSetAttributeRatio(type.Get(), MF_MT_FRAME_RATE, VCAM_FPS, 1);
    MFSetAttributeRatio(type.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
    type->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    // As shared/vcam-feed.js makes it: BT.601, limited range.
    type->SetUINT32(MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT601);
    type->SetUINT32(MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235);
    type->SetUINT32(MF_MT_ALL_SAMPLES_INDEPENDENT, TRUE);
    type->SetUINT32(MF_MT_FIXED_SIZE_SAMPLES, TRUE);
    type->SetUINT32(MF_MT_DEFAULT_STRIDE, VCAM_W);
    type->SetUINT32(MF_MT_SAMPLE_SIZE, VCAM_FRAME);
    type->SetUINT32(MF_MT_AVG_BITRATE, VCAM_FRAME * 8 * VCAM_FPS);
    IMFMediaType* types[] = { type.Get() };
    hr = MFCreateStreamDescriptor(0, 1, types, &sd_);
    if (FAILED(hr)) return hr;
    ComPtr<IMFMediaTypeHandler> handler;
    hr = sd_->GetMediaTypeHandler(&handler);
    if (FAILED(hr)) return hr;
    handler->SetCurrentMediaType(type.Get());
    // A colour camera's capture pin, shared between the apps that open it.
    sd_->SetGUID(MF_DEVICESTREAM_STREAM_CATEGORY, PIN_CAPTURE);
    sd_->SetUINT32(MF_DEVICESTREAM_STREAM_ID, 0);
    sd_->SetUINT32(MF_DEVICESTREAM_FRAMESERVER_SHARED, 1);
    sd_->SetUINT32(MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, MFFrameSourceTypes_Color);
    IMFStreamDescriptor* sds[] = { sd_.Get() };
    hr = MFCreatePresentationDescriptor(1, sds, &pd_);
    if (FAILED(hr)) return hr;
    pd_->SelectStream(0);
    Stream* s = new (std::nothrow) Stream(this, sd_.Get());
    if (!s) return E_OUTOFMEMORY;
    stream_.Attach(s);
    return S_OK;
  }

  volatile ULONG refs_ = 1;
  std::mutex m_;
  ComPtr<IMFMediaEventQueue> queue_;
  ComPtr<IMFAttributes> attrs_;
  ComPtr<IMFStreamDescriptor> sd_;
  ComPtr<IMFPresentationDescriptor> pd_;
  ComPtr<Stream> stream_;
  bool started_ = false;
  friend class Stream;
};

STDMETHODIMP Stream::GetMediaSource(IMFMediaSource** s) {
  if (!s) return E_POINTER;
  std::lock_guard<std::mutex> l(m_);
  if (!queue_ || !source_) return MF_E_SHUTDOWN;
  *s = static_cast<IMFMediaSourceEx*>(source_);
  (*s)->AddRef();
  return S_OK;
}

// ---- the activator Frame Server makes (the class registered) ----

class Activate : public IMFActivate {
public:
  Activate() { InterlockedIncrement(&g_objects); MFCreateAttributes(&attrs_, 1); }
  virtual ~Activate() { InterlockedDecrement(&g_objects); }

  STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
    if (!ppv) return E_POINTER;
    if (riid == __uuidof(IUnknown) || riid == __uuidof(IMFAttributes) || riid == __uuidof(IMFActivate)) *ppv = static_cast<IMFActivate*>(this);
    else { *ppv = nullptr; return E_NOINTERFACE; }
    AddRef();
    return S_OK;
  }
  STDMETHODIMP_(ULONG) AddRef() override { return InterlockedIncrement(&refs_); }
  STDMETHODIMP_(ULONG) Release() override {
    ULONG n = InterlockedDecrement(&refs_);
    if (!n) delete this;
    return n;
  }

  STDMETHODIMP ActivateObject(REFIID riid, void** ppv) override {
    if (!ppv) return E_POINTER;
    std::lock_guard<std::mutex> l(m_);
    if (!source_) {
      Source* s = nullptr;
      HRESULT hr = Source::Create(&s);
      if (FAILED(hr)) return hr;
      source_.Attach(s);
      // What Frame Server set on us (the camera's properties) goes on the source too.
      ComPtr<IMFAttributes> a;
      if (SUCCEEDED(source_->GetSourceAttributes(&a))) attrs_->CopyAllItems(a.Get());
    }
    return source_->QueryInterface(riid, ppv);
  }
  STDMETHODIMP ShutdownObject() override {
    std::lock_guard<std::mutex> l(m_);
    if (source_) { source_->Shutdown(); source_.Reset(); }
    return S_OK;
  }
  STDMETHODIMP DetachObject() override {
    std::lock_guard<std::mutex> l(m_);
    source_.Reset();
    return S_OK;
  }

  // IMFAttributes: our own store.
#define FWD(sig, call) STDMETHODIMP sig override { return attrs_->call; }
  FWD(GetItem(REFGUID k, PROPVARIANT* v), GetItem(k, v))
  FWD(GetItemType(REFGUID k, MF_ATTRIBUTE_TYPE* t), GetItemType(k, t))
  FWD(CompareItem(REFGUID k, REFPROPVARIANT v, BOOL* r), CompareItem(k, v, r))
  FWD(Compare(IMFAttributes* a, MF_ATTRIBUTES_MATCH_TYPE t, BOOL* r), Compare(a, t, r))
  FWD(GetUINT32(REFGUID k, UINT32* v), GetUINT32(k, v))
  FWD(GetUINT64(REFGUID k, UINT64* v), GetUINT64(k, v))
  FWD(GetDouble(REFGUID k, double* v), GetDouble(k, v))
  FWD(GetGUID(REFGUID k, GUID* v), GetGUID(k, v))
  FWD(GetStringLength(REFGUID k, UINT32* n), GetStringLength(k, n))
  FWD(GetString(REFGUID k, LPWSTR v, UINT32 n, UINT32* len), GetString(k, v, n, len))
  FWD(GetAllocatedString(REFGUID k, LPWSTR* v, UINT32* n), GetAllocatedString(k, v, n))
  FWD(GetBlobSize(REFGUID k, UINT32* n), GetBlobSize(k, n))
  FWD(GetBlob(REFGUID k, UINT8* b, UINT32 n, UINT32* len), GetBlob(k, b, n, len))
  FWD(GetAllocatedBlob(REFGUID k, UINT8** b, UINT32* n), GetAllocatedBlob(k, b, n))
  FWD(GetUnknown(REFGUID k, REFIID riid, LPVOID* v), GetUnknown(k, riid, v))
  FWD(SetItem(REFGUID k, REFPROPVARIANT v), SetItem(k, v))
  FWD(DeleteItem(REFGUID k), DeleteItem(k))
  FWD(DeleteAllItems(), DeleteAllItems())
  FWD(SetUINT32(REFGUID k, UINT32 v), SetUINT32(k, v))
  FWD(SetUINT64(REFGUID k, UINT64 v), SetUINT64(k, v))
  FWD(SetDouble(REFGUID k, double v), SetDouble(k, v))
  FWD(SetGUID(REFGUID k, REFGUID v), SetGUID(k, v))
  FWD(SetString(REFGUID k, LPCWSTR v), SetString(k, v))
  FWD(SetBlob(REFGUID k, const UINT8* b, UINT32 n), SetBlob(k, b, n))
  FWD(SetUnknown(REFGUID k, IUnknown* v), SetUnknown(k, v))
  FWD(LockStore(), LockStore())
  FWD(UnlockStore(), UnlockStore())
  FWD(GetCount(UINT32* n), GetCount(n))
  FWD(GetItemByIndex(UINT32 i, GUID* k, PROPVARIANT* v), GetItemByIndex(i, k, v))
  FWD(CopyAllItems(IMFAttributes* to), CopyAllItems(to))
#undef FWD

private:
  volatile ULONG refs_ = 1;
  std::mutex m_;
  ComPtr<IMFAttributes> attrs_;
  ComPtr<Source> source_;
};

// ---- COM ----

class Factory : public IClassFactory {
public:
  STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
    if (!ppv) return E_POINTER;
    if (riid == __uuidof(IUnknown) || riid == __uuidof(IClassFactory)) { *ppv = this; return S_OK; }
    *ppv = nullptr;
    return E_NOINTERFACE;
  }
  STDMETHODIMP_(ULONG) AddRef() override { return 2; }
  STDMETHODIMP_(ULONG) Release() override { return 1; }
  STDMETHODIMP CreateInstance(IUnknown* outer, REFIID riid, void** ppv) override {
    if (!ppv) return E_POINTER;
    *ppv = nullptr;
    if (outer) return CLASS_E_NOAGGREGATION;
    Activate* a = new (std::nothrow) Activate();
    if (!a) return E_OUTOFMEMORY;
    HRESULT hr = a->QueryInterface(riid, ppv);
    a->Release();
    return hr;
  }
  STDMETHODIMP LockServer(BOOL lock) override {
    if (lock) InterlockedIncrement(&g_objects);
    else InterlockedDecrement(&g_objects);
    return S_OK;
  }
};
static Factory g_factory;

static void ClsidKey(wchar_t* out, size_t n) {
  wchar_t clsid[64];
  StringFromGUID2(CLSID_TwinDeskVcam, clsid, 64);
  swprintf_s(out, n, L"Software\\Classes\\CLSID\\%s", clsid);
  (void)n;
}

extern "C" {

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID) {
  if (reason == DLL_PROCESS_ATTACH) {
    g_module = h;
    DisableThreadLibraryCalls(h);
  }
  return TRUE;
}

HRESULT __stdcall DllCanUnloadNow() { return g_objects == 0 ? S_OK : S_FALSE; }

HRESULT __stdcall DllGetClassObject(REFCLSID clsid, REFIID riid, void** ppv) {
  if (!ppv) return E_POINTER;
  *ppv = nullptr;
  if (clsid != CLSID_TwinDeskVcam) return CLASS_E_CLASSNOTAVAILABLE;
  return g_factory.QueryInterface(riid, ppv);
}

// HKLM\Software\Classes\CLSID\{…}\InprocServer32 = this file (elevated: regsvr32).
HRESULT __stdcall DllRegisterServer() {
  wchar_t path[MAX_PATH];
  if (!GetModuleFileNameW(g_module, path, MAX_PATH)) return HRESULT_FROM_WIN32(GetLastError());
  wchar_t key[128], sub[160];
  ClsidKey(key, 128);
  swprintf_s(sub, 160, L"%s\\InprocServer32", key);
  HKEY k;
  LONG r = RegCreateKeyExW(HKEY_LOCAL_MACHINE, key, 0, nullptr, 0, KEY_WRITE, nullptr, &k, nullptr);
  if (r) return HRESULT_FROM_WIN32(r);
  RegSetValueExW(k, nullptr, 0, REG_SZ, (const BYTE*)VCAM_NAME, (DWORD)((wcslen(VCAM_NAME) + 1) * 2));
  RegCloseKey(k);
  r = RegCreateKeyExW(HKEY_LOCAL_MACHINE, sub, 0, nullptr, 0, KEY_WRITE, nullptr, &k, nullptr);
  if (r) return HRESULT_FROM_WIN32(r);
  RegSetValueExW(k, nullptr, 0, REG_SZ, (const BYTE*)path, (DWORD)((wcslen(path) + 1) * 2));
  RegSetValueExW(k, L"ThreadingModel", 0, REG_SZ, (const BYTE*)L"Both", 10);
  RegCloseKey(k);
  return S_OK;
}

HRESULT __stdcall DllUnregisterServer() {
  wchar_t key[128];
  ClsidKey(key, 128);
  LONG r = RegDeleteTreeW(HKEY_LOCAL_MACHINE, key);
  return r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND ? S_OK : HRESULT_FROM_WIN32(r);
}

}
