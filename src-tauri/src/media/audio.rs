// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// This computer's sound on the other one, and the other one's here.
//
// Sending capture is the computer's own output, not a microphone: everything it
// plays. Windows has no capture API for that in a plain desktop process except
// WASAPI loopback on the default render device, which is what this uses; macOS
// uses ScreenCaptureKit's system audio (macOS 13+), leaving this app's own audio
// out so the other computer's sound coming back is never sent straight back.
//
// Both sides speak one format on the wire: 48 kHz, stereo, 32-bit float, little
// endian. The receiving side plays it through the default output device and
// resamples only if that device does not run at 48 kHz.
//
// Capture is off unless the Audio tab's send direction is on and the other
// computer says it plays our sound (`audio.policy`).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use tokio::sync::mpsc;

/// What travels on the wire, the same on every platform.
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u32 = 2;
/// Two seconds of playback; more than that is latency nobody wants.
const PLAYBACK_LIMIT: usize = (SAMPLE_RATE as usize) * (CHANNELS as usize) * 2;

/// Turn the device's frames into the wire's stereo 48 kHz float stream.
///
/// A device usually runs at 48 kHz, in which case this only upmixes; another
/// rate is interpolated linearly from the previous packet's last frame, so the
/// packets join without a click.
struct Upmix {
    src_rate: u32,
    src_channels: usize,
    /// Where the next output frame falls, counted in source frames from `carry`.
    pos: f64,
    carry: (f32, f32),
    /// Nothing has been buffered yet, so the first packet starts at its own frame
    /// zero instead of after a placeholder.
    primed: bool,
}

impl Upmix {
    fn new(src_rate: u32, src_channels: usize) -> Self {
        Self {
            src_rate: src_rate.max(1),
            src_channels: src_channels.max(1),
            pos: 0.0,
            carry: (0.0, 0.0),
            primed: false,
        }
    }

    /// `samples` are interleaved at the source rate; `out` receives stereo frames.
    fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        let ch = self.src_channels;
        if ch == 0 || samples.is_empty() {
            return;
        }
        let frames = samples.len() / ch;
        if frames == 0 {
            return;
        }
        // The previous packet's last frame first, so interpolation is continuous.
        let mut src: Vec<(f32, f32)> = Vec::with_capacity(frames + 1);
        if self.primed {
            src.push(self.carry);
        }
        for f in 0..frames {
            let base = f * ch;
            let l = samples[base];
            let r = if ch > 1 { samples[base + 1] } else { l };
            src.push((l, r));
        }
        let step = self.src_rate as f64 / SAMPLE_RATE as f64;
        while self.pos + 1.0 < src.len() as f64 {
            let i = self.pos.floor() as usize;
            let frac = (self.pos - i as f64) as f32;
            let (l0, r0) = src[i];
            let (l1, r1) = src[i + 1];
            out.push(l0 + (l1 - l0) * frac);
            out.push(r0 + (r1 - r0) * frac);
            self.pos += step;
        }
        let last = src.len() - 1;
        let consumed = self.pos.floor() as usize;
        if consumed > 0 {
            if consumed > last {
                // The whole packet was used; keep its final frame as the start of
                // the next one, counting the position from there.
                self.carry = src[last];
                self.pos -= last as f64;
            } else {
                self.carry = src[consumed];
                self.pos -= consumed as f64;
            }
            self.primed = true;
        }
    }
}

fn f32_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

// ---------------------------------------------------------------------------
// Receiving: the other computer's sound through this one's speakers
// ---------------------------------------------------------------------------

struct PlayerState {
    buffer: Mutex<VecDeque<f32>>,
    volume: AtomicU32,
    running: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
    error: Mutex<String>,
    packets: std::sync::atomic::AtomicU64,
    peak: AtomicU32,
}

pub struct AudioPlayer {
    state: Arc<PlayerState>,
    started: Mutex<bool>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(PlayerState {
                buffer: Mutex::new(VecDeque::with_capacity(PLAYBACK_LIMIT)),
                volume: AtomicU32::new(1.0f32.to_bits()),
                running: Arc::new(AtomicBool::new(false)),
                playing: Arc::new(AtomicBool::new(false)),
                error: Mutex::new(String::new()),
                packets: std::sync::atomic::AtomicU64::new(0),
                peak: AtomicU32::new(0f32.to_bits()),
            }),
            started: Mutex::new(false),
        }
    }

    pub fn set_volume(&self, volume: f64) {
        let v = volume.clamp(0.0, 2.0) as f32;
        self.state.volume.store(v.to_bits(), Ordering::Relaxed);
    }

    /// How many packets have arrived, and the loudest sample in them.
    pub fn traffic(&self) -> (u64, f32) {
        (
            self.state.packets.load(Ordering::Relaxed),
            f32::from_bits(self.state.peak.load(Ordering::Relaxed)),
        )
    }

    /// Start measuring the peak again, so the log describes the last window.
    pub fn reset_peak(&self) {
        self.state.peak.store(0f32.to_bits(), Ordering::Relaxed);
    }

    /// True while sound has arrived in the last couple of seconds.
    pub fn is_playing(&self) -> bool {
        self.state.playing.load(Ordering::Relaxed)
    }

    pub fn error(&self) -> String {
        self.state.error.lock().clone()
    }

    pub fn stop(&self) {
        self.state.running.store(false, Ordering::SeqCst);
        self.state.playing.store(false, Ordering::Relaxed);
        self.state.buffer.lock().clear();
        *self.started.lock() = false;
    }

    /// Add wire bytes (48 kHz stereo float32 LE). Drops when nothing is playing
    /// yet, so a stopped tab cannot grow a buffer nobody hears.
    pub fn push(&self, bytes: &[u8]) {
        if !self.state.running.load(Ordering::Relaxed) {
            return;
        }
        let mut buf = self.state.buffer.lock();
        let mut i = 0;
        let mut peak = f32::from_bits(self.state.peak.load(Ordering::Relaxed));
        while i + 4 <= bytes.len() {
            let sample = f32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
            if sample.abs() > peak {
                peak = sample.abs();
            }
            if buf.len() >= PLAYBACK_LIMIT {
                buf.pop_front();
            }
            buf.push_back(sample);
            i += 4;
        }
        drop(buf);
        self.state.peak.store(peak.to_bits(), Ordering::Relaxed);
        self.state.playing.store(true, Ordering::Relaxed);
        let n = self.state.packets.fetch_add(1, Ordering::Relaxed) + 1;
        if n == 1 {
            log::info!("[Audio] First sound packet from the other computer ({} bytes)", bytes.len());
        }
    }

    /// Open the output device once; a second call does nothing.
    pub fn start(&self) {
        {
            let mut started = self.started.lock();
            if *started {
                return;
            }
            *started = true;
        }
        self.state.running.store(true, Ordering::SeqCst);
        let state = self.state.clone();
        let running = state.running.clone();
        let thread = std::thread::Builder::new()
            .name("glidedesk-audio-out".into())
            .spawn(move || {
                if let Err(e) = run_player(state.clone()) {
                    log::warn!("[Audio] Could not play the other computer's sound: {}", e);
                    *state.error.lock() = e;
                }
                running.store(false, Ordering::SeqCst);
            });
        if let Err(e) = thread {
            log::warn!("[Audio] Could not start the playback thread: {}", e);
        }
    }
}

fn run_player(state: Arc<PlayerState>) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "No default audio output device".to_string())?;
    let supported = device
        .default_output_config()
        .map_err(|e| format!("No output format: {}", e))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();
    let rate = config.sample_rate.0;
    let channels = config.channels as usize;
    log::info!(
        "[Audio] Playing the other computer's sound: {:?}, {} Hz, {} channel(s)",
        format,
        rate,
        channels
    );

    let stream = match format {
        cpal::SampleFormat::F32 => build_output::<f32>(&device, &config, state.clone(), rate, channels),
        cpal::SampleFormat::I16 => build_output::<i16>(&device, &config, state.clone(), rate, channels),
        cpal::SampleFormat::U16 => build_output::<u16>(&device, &config, state.clone(), rate, channels),
        other => Err(format!("Unsupported output sample format: {:?}", other)),
    }?;
    stream.play().map_err(|e| format!("Could not start playback: {}", e))?;

    // cpal's stream is not Send, so it lives on this thread for as long as the
    // tab wants sound; dropping it here is what closes the device.
    while state.running.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(stream);
    log::info!("[Audio] Stopped playing the other computer's sound");
    Ok(())
}

fn build_output<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    state: Arc<PlayerState>,
    device_rate: u32,
    device_channels: usize,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let step = SAMPLE_RATE as f64 / device_rate.max(1) as f64;
    let mut pos = 0.0f64;
    let err_state = state.clone();
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                let volume = f32::from_bits(state.volume.load(Ordering::Relaxed));
                let frames = if device_channels == 0 {
                    0
                } else {
                    out.len() / device_channels
                };
                let need = ((frames as f64) * step).ceil() as usize + 2;
                let mut src: Vec<f32> = Vec::with_capacity(need * 2);
                {
                    let mut buffer = state.buffer.lock();
                    for _ in 0..need * 2 {
                        match buffer.pop_front() {
                            Some(s) => src.push(s),
                            None => break,
                        }
                    }
                }
                if src.is_empty() {
                    for sample in out.iter_mut() {
                        *sample = T::from_sample(0.0f32);
                    }
                    pos = 0.0;
                    state.playing.store(false, Ordering::Relaxed);
                    return;
                }
                for f in 0..frames {
                    let iframe = pos.floor() as usize;
                    let i = iframe * 2;
                    let frac = (pos - iframe as f64) as f32;
                    let l = src.get(i).copied().unwrap_or(0.0);
                    let l1 = src.get(i + 2).copied().unwrap_or(l);
                    let r = src.get(i + 1).copied().unwrap_or(l);
                    let r1 = src.get(i + 3).copied().unwrap_or(r);
                    let l = (l + (l1 - l) * frac) * volume;
                    let r = (r + (r1 - r) * frac) * volume;
                    let base = f * device_channels;
                    if device_channels == 1 {
                        out[base] = T::from_sample((l + r) * 0.5);
                    } else {
                        out[base] = T::from_sample(l);
                        out[base + 1] = T::from_sample(r);
                        for extra in 2..device_channels {
                            out[base + extra] = T::from_sample(0.0f32);
                        }
                    }
                    pos += step;
                }
                let consumed = pos.floor() as usize;
                if consumed > 0 {
                    // Put back what the next callback still needs.
                    let mut buffer = state.buffer.lock();
                    let mut rest: Vec<f32> = src.drain(consumed * 2..).collect();
                    rest.reverse();
                    for s in rest {
                        buffer.push_front(s);
                    }
                    pos -= consumed as f64;
                }
            },
            move |e| {
                *err_state.error.lock() = format!("Playback error: {}", e);
                log::warn!("[Audio] Playback error: {}", e);
            },
            None,
        )
        .map_err(|e| format!("Could not open the output device: {}", e))
}

// ---------------------------------------------------------------------------
// Sending: this computer's sound
// ---------------------------------------------------------------------------

pub struct AudioEngine {
    capturing: Arc<AtomicBool>,
    tx: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    error: Arc<Mutex<String>>,
    upmix: Arc<Mutex<Option<Upmix>>>,
}

impl AudioEngine {
    pub fn new() -> (Self, mpsc::Receiver<Vec<u8>>) {
        let (tx, rx) = mpsc::channel(200);
        (
            Self {
                capturing: Arc::new(AtomicBool::new(false)),
                tx: Mutex::new(Some(tx)),
                error: Arc::new(Mutex::new(String::new())),
                upmix: Arc::new(Mutex::new(None)),
            },
            rx,
        )
    }

    pub fn is_capturing(&self) -> bool {
        self.capturing.load(Ordering::Relaxed)
    }

    pub fn error(&self) -> String {
        self.error.lock().clone()
    }

    pub fn start_capture(&self) -> Result<(), String> {
        if self.capturing.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        *self.error.lock() = String::new();
        let tx = self.tx.lock().clone();
        let Some(tx) = tx else {
            return Err("The audio channel is closed".to_string());
        };
        let running = self.capturing.clone();
        let error = self.error.clone();
        let upmix = self.upmix.clone();

        let spawned = std::thread::Builder::new()
            .name("glidedesk-audio-in".into())
            .spawn(move || {
                if let Err(e) = run_capture(tx, running.clone(), upmix) {
                    log::warn!("[Audio] Could not capture this computer's sound: {}", e);
                    *error.lock() = e;
                }
                running.store(false, Ordering::SeqCst);
            });
        if let Err(e) = spawned {
            self.capturing.store(false, Ordering::SeqCst);
            return Err(format!("Could not start the capture thread: {}", e));
        }
        Ok(())
    }

    pub fn stop_capture(&self) {
        if self.capturing.swap(false, Ordering::SeqCst) {
            log::info!("[Audio] Stopped sending this computer's sound");
        }
    }
}

/// One capture backend per platform; each calls `send` with stereo 48 kHz frames.
#[cfg(target_os = "macos")]
fn run_capture(
    tx: mpsc::Sender<Vec<u8>>,
    running: Arc<AtomicBool>,
    upmix: Arc<Mutex<Option<Upmix>>>,
) -> Result<(), String> {
    use screencapturekit::prelude::*;

    struct Handler {
        tx: mpsc::Sender<Vec<u8>>,
        upmix: Arc<Mutex<Option<Upmix>>>,
    }

    impl SCStreamOutputTrait for Handler {
        fn did_output_sample_buffer(&self, sample: CMSampleBuffer, output_type: SCStreamOutputType) {
            if output_type != SCStreamOutputType::Audio {
                return;
            }
            let Some(samples) = mac_sample_f32(&sample) else {
                return;
            };
            let mut up = self.upmix.lock();
            if up.is_none() {
                *up = Some(Upmix::new(SAMPLE_RATE, CHANNELS as usize));
            }
            let mut stereo = Vec::with_capacity(samples.len());
            if let Some(u) = up.as_mut() {
                u.push(&samples, &mut stereo);
            }
            if !stereo.is_empty() {
                let _ = self.tx.try_send(f32_bytes(&stereo));
            }
        }
    }

    // ScreenCaptureKit does not raise the permission dialog itself: asking it for
    // shareable content only fails, exactly as if the person had said no, so the
    // request has to be made through CoreGraphics first.
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    let granted = unsafe { CGPreflightScreenCaptureAccess() };
    log::info!("[Audio] Screen Recording permission: {}", granted);
    if !granted {
        let asked = unsafe { CGRequestScreenCaptureAccess() };
        log::info!("[Audio] Asked for Screen Recording permission: {}", asked);
        if !asked {
            return Err(
                "Screen Recording permission is needed for this computer's sound: allow GlideDesk in System Settings › Privacy & Security › Screen & System Audio Recording, then reopen GlideDesk"
                    .to_string(),
            );
        }
    }

    let content = SCShareableContent::get()
        .map_err(|e| format!("Screen Recording permission is needed to read this computer's sound: {}", e))?;
    let display = content
        .displays()
        .into_iter()
        .next()
        .ok_or_else(|| "No display to take sound from".to_string())?;
    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build()
        .map_err(|e| format!("Could not build the audio filter: {}", e))?;
    // ScreenCaptureKit always captures a picture as well: keep it tiny and let the
    // audio run on its own. The app's own audio is left out so sound arriving from
    // the other computer is never captured and sent back.
    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_captures_audio(true)
        .with_excludes_current_process_audio(true)
        .with_sample_rate(SAMPLE_RATE as i32)
        .with_channel_count(CHANNELS as i32);
    let mut stream = SCStream::new(&filter, &config)
        .map_err(|e| format!("Could not start the audio stream: {}", e))?;
    stream
        .add_output_handler(
            Handler {
                tx: tx.clone(),
                upmix: upmix.clone(),
            },
            SCStreamOutputType::Audio,
        )
        .map_err(|e| format!("Could not register the audio output: {}", e))?;
    stream
        .start_capture()
        .map_err(|e| format!("Could not start audio capture: {}", e))?;
    log::info!("[Audio] Sending this computer's sound (ScreenCaptureKit, 48 kHz stereo)");

    while running.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = stream.stop_capture();
    Ok(())
}

/// A ScreenCaptureKit audio buffer as float samples (interleaved by channel).
#[cfg(target_os = "macos")]
fn mac_sample_f32(sample: &screencapturekit::cm::CMSampleBuffer) -> Option<Vec<f32>> {
    use std::ffi::c_void;

    #[link(name = "CoreMedia", kind = "framework")]
    extern "C" {
        fn CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sbuf: *mut c_void,
            size_needed_out: *mut usize,
            buffer_list_out: *mut c_void,
            buffer_list_size: usize,
            block_buffer_allocator: *const c_void,
            block_buffer_memory_allocator: *const c_void,
            flags: u32,
            block_buffer_out: *mut *mut c_void,
        ) -> i32;
        fn CFRelease(cf: *const c_void);
    }

    #[repr(C)]
    struct AudioBuffer {
        number_channels: u32,
        data_byte_size: u32,
        data: *mut u8,
    }

    unsafe {
        let sbuf = sample.as_ptr();
        let mut needed: usize = 0;
        let _ = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sbuf,
            &mut needed,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
        );
        if needed == 0 {
            return None;
        }
        let mut storage = vec![0u8; needed];
        let mut block: *mut c_void = std::ptr::null_mut();
        let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sbuf,
            std::ptr::null_mut(),
            storage.as_mut_ptr() as *mut c_void,
            needed,
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut block,
        );
        let out = if status == 0 {
            let count = *(storage.as_ptr() as *const u32) as usize;
            // The variable length list: the count, then AudioBuffer entries with
            // pointer alignment, which puts the first one at byte 8.
            let mut bufs: Vec<(usize, &[u8])> = Vec::with_capacity(count);
            for i in 0..count {
                let at = 8 + i * std::mem::size_of::<AudioBuffer>();
                if at + std::mem::size_of::<AudioBuffer>() > storage.len() {
                    break;
                }
                let b = &*(storage.as_ptr().add(at) as *const AudioBuffer);
                if b.data.is_null() || b.data_byte_size == 0 {
                    continue;
                }
                let bytes = std::slice::from_raw_parts(b.data, b.data_byte_size as usize);
                bufs.push((b.number_channels as usize, bytes));
            }
            let floats = |b: &[u8]| -> Vec<f32> {
                b.chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect()
            };
            let out = match bufs.len() {
                0 => None,
                1 => {
                    let (channels, bytes) = bufs[0];
                    let samples = floats(bytes);
                    if channels <= 1 {
                        // Mono arrives once per frame: both ears.
                        let mut stereo = Vec::with_capacity(samples.len() * 2);
                        for s in samples {
                            stereo.push(s);
                            stereo.push(s);
                        }
                        Some(stereo)
                    } else {
                        Some(samples)
                    }
                }
                _ => {
                    // One buffer per channel: interleave left and right.
                    let left = floats(bufs[0].1);
                    let right = floats(bufs[1].1);
                    let frames = left.len().min(right.len());
                    let mut stereo = Vec::with_capacity(frames * 2);
                    for f in 0..frames {
                        stereo.push(left[f]);
                        stereo.push(right[f]);
                    }
                    Some(stereo)
                }
            };
            out
        } else {
            None
        };
        if !block.is_null() {
            CFRelease(block as *const c_void);
        }
        out
    }
}

#[cfg(target_os = "windows")]
fn run_capture(
    tx: mpsc::Sender<Vec<u8>>,
    running: Arc<AtomicBool>,
    upmix: Arc<Mutex<Option<Upmix>>>,
) -> Result<(), String> {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    const IEEE_FLOAT: u16 = 3;
    const EXTENSIBLE: u16 = 0xFFFE;
    const SUBTYPE_IEEE_FLOAT_TAIL: [u8; 12] = [
        0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
    ];

    // WASAPI needs COM on this thread, and every object belongs to it.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let result = (|| -> Result<(), String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| format!("Could not reach the audio devices: {}", e))?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| format!("No default sound output: {}", e))?;
            let client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| format!("Could not open the sound output: {}", e))?;
            let format_ptr = client
                .GetMixFormat()
                .map_err(|e| format!("Could not read the output format: {}", e))?;
            if format_ptr.is_null() {
                return Err("The output has no format".to_string());
            }
            let format = *format_ptr;
            let mut is_float = format.wFormatTag == IEEE_FLOAT;
            if format.wFormatTag == EXTENSIBLE {
                // WAVEFORMATEXTENSIBLE: the first 18 bytes are the same, then the
                // channel mask and the sub-format GUID.
                let ext = format_ptr as *const u8;
                let sub = std::slice::from_raw_parts(ext.add(24), 16);
                let tail = &sub[4..16];
                is_float = tail == SUBTYPE_IEEE_FLOAT_TAIL;
            }
            let rate = format.nSamplesPerSec;
            let channels = format.nChannels as usize;
            let bits = format.wBitsPerSample as usize;
            let block_align = format.nBlockAlign as usize;
            if channels == 0 || rate == 0 {
                CoTaskMemFree(Some(format_ptr as *const _));
                return Err("The output format is empty".to_string());
            }
            // 200 ms of shared-mode buffer; WASAPI loopback needs no event.
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    2_000_000,
                    0,
                    format_ptr,
                    None,
                )
                .map_err(|e| format!("Could not start listening to the output: {}", e))?;
            let capture: IAudioCaptureClient = client
                .GetService()
                .map_err(|e| format!("Could not read the output stream: {}", e))?;
            CoTaskMemFree(Some(format_ptr as *const _));
            client
                .Start()
                .map_err(|e| format!("Could not start the output stream: {}", e))?;
            log::info!(
                "[Audio] Sending this computer's sound (WASAPI loopback, {} Hz, {} channel(s), {} bit{})",
                rate,
                channels,
                bits,
                if is_float { " float" } else { "" }
            );

            let mut up = Upmix::new(rate, channels);
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames: u32 = 0;
            let mut flags: u32 = 0;
            let mut samples: Vec<f32> = Vec::new();
            let mut stereo: Vec<f32> = Vec::new();

            while running.load(Ordering::SeqCst) {
                let packet = match capture.GetNextPacketSize() {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if packet == 0 {
                    std::thread::sleep(Duration::from_millis(8));
                    continue;
                }
                let got = capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None);
                if got.is_err() {
                    std::thread::sleep(Duration::from_millis(8));
                    continue;
                }
                let n = frames as usize;
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                samples.clear();
                if !silent && !data.is_null() && n > 0 {
                    match (is_float, bits) {
                        (true, _) => {
                            let p = std::slice::from_raw_parts(data as *const f32, n * channels);
                            samples.extend_from_slice(p);
                        }
                        (false, 16) => {
                            let p = std::slice::from_raw_parts(data as *const i16, n * channels);
                            samples.extend(p.iter().map(|v| *v as f32 / 32768.0));
                        }
                        (false, 32) => {
                            let p = std::slice::from_raw_parts(data as *const i32, n * channels);
                            samples.extend(p.iter().map(|v| *v as f32 / 2147483648.0));
                        }
                        (false, 8) => {
                            let p = std::slice::from_raw_parts(data as *const u8, n * channels);
                            samples.extend(p.iter().map(|v| (*v as f32 - 128.0) / 128.0));
                        }
                        _ => {}
                    }
                    if block_align == 0 && channels > 0 {
                        // Nothing to do: channels already tell us the stride.
                    }
                }
                let _ = capture.ReleaseBuffer(frames);
                // Silence is still shipped: it keeps the receiver's clock right.
                stereo.clear();
                if samples.is_empty() {
                    stereo.resize(n * 2, 0.0);
                } else {
                    up.push(&samples, &mut stereo);
                }
                if !stereo.is_empty() {
                    let _ = tx.try_send(f32_bytes(&stereo));
                }
            }

            let _ = capture.ReleaseBuffer(0);
            let _ = client.Stop();
            let _ = client.Reset();
            Ok(())
        }
    })();

    unsafe {
        windows::Win32::System::Com::CoUninitialize();
    }
    result
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn run_capture(
    tx: mpsc::Sender<Vec<u8>>,
    running: Arc<AtomicBool>,
    upmix: Arc<Mutex<Option<Upmix>>>,
) -> Result<(), String> {
    // No system-output capture here: fall back to the default input, which at
    // least carries a voice and is what this path shipped with before.
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No default audio input device".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("No input format: {}", e))?;
    let format = supported.sample_format();
    let config = supported.config();
    let rate = config.sample_rate.0;
    let channels = config.channels as usize;
    let up = Arc::new(Mutex::new(Upmix::new(rate, channels)));
    let up_cb = up.clone();
    let tx_cb = tx.clone();
    let run = running.clone();
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !run.load(Ordering::Relaxed) {
                    return;
                }
                let mut stereo = Vec::with_capacity(data.len() * 2);
                if let Some(u) = up_cb.lock().as_mut() {
                    u.push(data, &mut stereo);
                }
                if !stereo.is_empty() {
                    let _ = tx_cb.try_send(f32_bytes(&stereo));
                }
            },
            |e| log::warn!("[Audio] Input error: {}", e),
            None,
        ),
        cpal::SampleFormat::I16 => {
            let up_cb = up.clone();
            let tx_cb = tx.clone();
            let run = running.clone();
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if !run.load(Ordering::Relaxed) {
                        return;
                    }
                    let floats: Vec<f32> = data.iter().map(|v| *v as f32 / 32768.0).collect();
                    let mut stereo = Vec::with_capacity(floats.len() * 2);
                    if let Some(u) = up_cb.lock().as_mut() {
                        u.push(&floats, &mut stereo);
                    }
                    if !stereo.is_empty() {
                        let _ = tx_cb.try_send(f32_bytes(&stereo));
                    }
                },
                |e| log::warn!("[Audio] Input error: {}", e),
                None,
            )
        }
        other => Err(cpal::BuildStreamError::StreamConfigNotSupported),
    }
    .map_err(|e| format!("Could not open the input: {}", e))?;
    let _ = tx;
    stream
        .play()
        .map_err(|e| format!("Could not start the input: {}", e))?;
    log::info!("[Audio] Sending this computer's input ({} Hz)", rate);
    while running.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(stream);
    Ok(())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
use cpal::traits::StreamTrait as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upmix_passes_48k_stereo_through() {
        let mut up = Upmix::new(48_000, 2);
        let input: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let mut out = Vec::new();
        up.push(&input, &mut out);
        // The last frame needs its successor to be interpolated, so it waits for the
        // next packet; everything before it comes out unchanged.
        assert_eq!(out, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);

        let more: Vec<f32> = (8..16).map(|i| i as f32).collect();
        up.push(&more, &mut out);
        // The carried frame starts the next packet, then the new frames.
        assert_eq!(
            out[6..],
            [6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0]
        );
        assert_eq!(out.len(), 14);
    }

    #[test]
    fn upmix_duplicates_mono_to_both_ears() {
        let mut up = Upmix::new(48_000, 1);
        let mut out = Vec::new();
        up.push(&[0.5, -0.5, 0.25, 0.0], &mut out);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5, 0.25, 0.25]);
    }

    #[test]
    fn upmix_halves_the_frames_at_96k() {
        let mut up = Upmix::new(96_000, 2);
        let mut out = Vec::new();
        for _ in 0..3 {
            let input: Vec<f32> = (0..16).map(|i| i as f32).collect();
            up.push(&input, &mut out);
        }
        // Eight source frames become four per packet, in every packet, not one fewer
        // each time: 3 × 4 frames × 2 channels.
        assert_eq!(out.len(), 24);
    }

    #[test]
    fn upmix_keeps_the_rate_over_many_packets() {
        let mut up = Upmix::new(44_100, 2);
        let mut out = Vec::new();
        let input: Vec<f32> = vec![0.0; 441 * 2];
        for _ in 0..10 {
            up.push(&input, &mut out);
        }
        // 10 × 441 frames at 44.1 kHz is 4 800 frames at 48 kHz, give or take the
        // one frame the resampler is allowed to hold back.
        let frames = out.len() / 2;
        assert!((4_799..=4_800).contains(&frames), "got {frames} frames");
    }

    #[test]
    fn upmix_carries_the_last_frame_between_packets() {
        let mut up = Upmix::new(48_000, 2);
        let mut out = Vec::new();
        up.push(&[1.0, 1.0, 1.0, 1.0], &mut out);
        assert_eq!(out.len(), 2);
        up.push(&[1.0, 1.0, 1.0, 1.0], &mut out);
        assert_eq!(out.len(), 6);
        assert!(out.iter().all(|s| (*s - 1.0).abs() < 1e-6));
    }

    #[test]
    fn wire_bytes_are_little_endian_f32() {
        let bytes = f32_bytes(&[1.0, -1.0]);
        assert_eq!(bytes.len(), 8);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), -1.0);
    }
}
