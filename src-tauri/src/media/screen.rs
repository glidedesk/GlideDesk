// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Real Native Screen Frame Streaming Engine via OS Graphics Subsystem

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use parking_lot::Mutex;

#[cfg(target_os = "macos")]
mod macos_capture {
    use std::ffi::c_void;

    pub type CGDirectDisplayID = u32;
    pub type CGImageRef = *mut c_void;
    pub type CGDataProviderRef = *mut c_void;
    pub type CFDataRef = *const c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        pub fn CGMainDisplayID() -> CGDirectDisplayID;
        pub fn CGDisplayCreateImage(display: CGDirectDisplayID) -> CGImageRef;
        pub fn CGImageGetWidth(image: CGImageRef) -> usize;
        pub fn CGImageGetHeight(image: CGImageRef) -> usize;
        pub fn CGImageGetBytesPerRow(image: CGImageRef) -> usize;
        pub fn CGImageGetDataProvider(image: CGImageRef) -> CGDataProviderRef;
        pub fn CGDataProviderCopyData(provider: CGDataProviderRef) -> CFDataRef;
        pub fn CGImageRelease(image: CGImageRef);
        pub fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        pub fn CFDataGetLength(data: CFDataRef) -> isize;
        pub fn CFRelease(cf: *const c_void);
    }

    pub fn capture_primary_display() -> Result<(u32, u32, Vec<u8>), String> {
        unsafe {
            let display_id = CGMainDisplayID();
            let image_ref = CGDisplayCreateImage(display_id);
            if image_ref.is_null() {
                return Err("CGDisplayCreateImage returned NULL (screen capture permission missing or display disconnected)".to_string());
            }

            let width = CGImageGetWidth(image_ref);
            let height = CGImageGetHeight(image_ref);
            let bytes_per_row = CGImageGetBytesPerRow(image_ref);

            let provider = CGImageGetDataProvider(image_ref);
            if provider.is_null() {
                CGImageRelease(image_ref);
                return Err("Failed to get CGDataProvider from CGImage".to_string());
            }

            let data_ref = CGDataProviderCopyData(provider);
            if data_ref.is_null() {
                CGImageRelease(image_ref);
                return Err("Failed to copy CGDataProvider data".to_string());
            }

            let len = CFDataGetLength(data_ref) as usize;
            let ptr = CFDataGetBytePtr(data_ref);

            let mut rgba = Vec::with_capacity(width * height * 4);
            if !ptr.is_null() && len >= height * bytes_per_row {
                let slice = std::slice::from_raw_parts(ptr, len);
                for y in 0..height {
                    let row_start = y * bytes_per_row;
                    for x in 0..width {
                        let px_idx = row_start + x * 4;
                        if px_idx + 3 < len {
                            // macOS screen bitmap is BGRA; convert to RGBA
                            let b = slice[px_idx];
                            let g = slice[px_idx + 1];
                            let r = slice[px_idx + 2];
                            let a = slice[px_idx + 3];
                            rgba.push(r);
                            rgba.push(g);
                            rgba.push(b);
                            rgba.push(a);
                        }
                    }
                }
            }

            CFRelease(data_ref);
            CGImageRelease(image_ref);

            if rgba.is_empty() {
                return Err("Captured empty pixel buffer from primary display".to_string());
            }

            Ok((width as u32, height as u32, rgba))
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_capture {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    pub fn capture_primary_display() -> Result<(u32, u32, Vec<u8>), String> {
        unsafe {
            let width = GetSystemMetrics(SM_CXSCREEN);
            let height = GetSystemMetrics(SM_CYSCREEN);
            if width <= 0 || height <= 0 {
                return Err("Failed to get Windows display dimensions".to_string());
            }

            let hdc_screen = GetDC(None);
            if hdc_screen.is_invalid() {
                return Err("GetDC failed".to_string());
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            let hbm = CreateCompatibleBitmap(hdc_screen, width, height);
            let old_obj = SelectObject(hdc_mem, hbm);

            let ok = BitBlt(hdc_mem, 0, 0, width, height, hdc_screen, 0, 0, SRCCOPY);
            if ok.is_err() {
                SelectObject(hdc_mem, old_obj);
                let _ = DeleteObject(hbm);
                let _ = DeleteDC(hdc_mem);
                ReleaseDC(None, hdc_screen);
                return Err("BitBlt failed".to_string());
            }

            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bgra = vec![0u8; (width * height * 4) as usize];
            GetDIBits(
                hdc_mem,
                hbm,
                0,
                height as u32,
                Some(bgra.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            SelectObject(hdc_mem, old_obj);
            let _ = DeleteObject(hbm);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(None, hdc_screen);

            // Convert BGRA to RGBA
            let mut rgba = Vec::with_capacity(bgra.len());
            for chunk in bgra.chunks_exact(4) {
                rgba.push(chunk[2]);
                rgba.push(chunk[1]);
                rgba.push(chunk[0]);
                rgba.push(chunk[3]);
            }

            Ok((width as u32, height as u32, rgba))
        }
    }
}

#[cfg(any(test, target_os = "linux"))]
pub fn convert_x11_image_to_rgba(
    width: u32,
    height: u32,
    depth: u8,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "Screen dimensions overflow buffer calculation".to_string())?;

    if data.len() < expected_len {
        return Err(format!(
            "X11 ZPixmap data length {} is smaller than expected {} for {}x{} (depth {})",
            data.len(),
            expected_len,
            width,
            height,
            depth
        ));
    }

    let mut rgba = Vec::with_capacity(expected_len);
    for chunk in data[..expected_len].chunks_exact(4) {
        let b = chunk[0];
        let g = chunk[1];
        let r = chunk[2];
        let a = if depth == 32 && chunk[3] > 0 { chunk[3] } else { 255 };
        rgba.push(r);
        rgba.push(g);
        rgba.push(b);
        rgba.push(a);
    }

    Ok(rgba)
}

#[cfg(target_os = "linux")]
mod linux_capture {
    use super::convert_x11_image_to_rgba;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto;

    /// Captures the primary X11 root window display using x11rb `get_image`.
    /// Note: Wayland compositors restrict global root window capture without PipeWire or portal permissions.
    /// Under X11 or XWayland, full desktop frame readback is supported.
    pub fn capture_primary_display() -> Result<(u32, u32, Vec<u8>), String> {
        let display = std::env::var("DISPLAY").ok();
        if display.as_deref().map_or(true, |d| d.trim().is_empty()) {
            return Err("Screen capture unavailable on Linux: headless session (DISPLAY not set)".to_string());
        }

        if let Ok(session) = std::env::var("XDG_SESSION_TYPE") {
            if session.eq_ignore_ascii_case("wayland") {
                log::warn!("[ScreenCapture] Wayland session detected; root-window capture may be blocked by compositor without PipeWire portal");
            }
        }

        let (conn, screen_num) = x11rb::connect(display.as_deref())
            .map_err(|e| format!("Screen capture failed to open X display: {}", e))?;

        let setup = conn.setup();
        let screen = setup.roots.get(screen_num)
            .ok_or_else(|| format!("Invalid X screen index {}", screen_num))?;

        let root = screen.root;
        let width = screen.width_in_pixels;
        let height = screen.height_in_pixels;

        if width == 0 || height == 0 {
            return Err("Captured X screen has 0 width or height".to_string());
        }

        let reply = xproto::get_image(
            &conn,
            xproto::ImageFormat::Z_PIXMAP,
            root,
            0,
            0,
            width,
            height,
            !0,
        )
        .map_err(|e| format!("X11 get_image request failed: {}", e))?
        .reply()
        .map_err(|e| format!("X11 get_image reply failed: {}", e))?;

        let rgba = convert_x11_image_to_rgba(width as u32, height as u32, reply.depth, &reply.data)?;
        Ok((width as u32, height as u32, rgba))
    }
}

pub fn capture_display() -> Result<(u32, u32, Vec<u8>), String> {
    #[cfg(target_os = "macos")]
    {
        macos_capture::capture_primary_display()
    }
    #[cfg(target_os = "windows")]
    {
        windows_capture::capture_primary_display()
    }
    #[cfg(target_os = "linux")]
    {
        linux_capture::capture_primary_display()
    }
}

pub struct ScreenEngine {
    is_capturing: Arc<AtomicBool>,
    frame_tx: Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
}

impl ScreenEngine {
    pub fn new() -> (Self, mpsc::Receiver<Vec<u8>>) {
        let (tx, rx) = mpsc::channel(30);
        (
            Self {
                is_capturing: Arc::new(AtomicBool::new(false)),
                frame_tx: Arc::new(Mutex::new(Some(tx))),
            },
            rx,
        )
    }

    pub fn start_streaming(&self, fps: u32) -> Result<(), String> {
        if self.is_capturing.load(Ordering::SeqCst) {
            return Ok(());
        }

        log::info!("Starting native desktop screen capture engine at {} FPS", fps);
        self.is_capturing.store(true, Ordering::SeqCst);
        let capturing = self.is_capturing.clone();
        let tx = self.frame_tx.lock().clone();

        std::thread::spawn(move || {
            let interval = std::time::Duration::from_millis(1000 / (fps.max(1) as u64));
            let mut logged_error = false;
            let mut consecutive_failures = 0usize;
            const MAX_CONSECUTIVE_FAILURES: usize = 5;

            while capturing.load(Ordering::SeqCst) {
                if let Some(ref sender) = tx {
                    match capture_display() {
                        Ok((width, height, rgba_data)) => {
                            consecutive_failures = 0;
                            logged_error = false;
                            if let Some(rgba) = image::RgbaImage::from_raw(width, height, rgba_data) {
                                // Downsample large Retina displays (e.g. >1920px wide) to reduce latency
                                let dyn_img = if width > 1920 {
                                    image::DynamicImage::ImageRgba8(rgba).thumbnail(1920, 1080)
                                } else {
                                    image::DynamicImage::ImageRgba8(rgba)
                                };
                                let mut frame_data = Vec::new();
                                if dyn_img
                                    .write_to(
                                        &mut std::io::Cursor::new(&mut frame_data),
                                        image::ImageFormat::Png,
                                    )
                                    .is_ok()
                                {
                                    let _ = sender.try_send(frame_data);
                                }
                            }
                        }
                        Err(e) => {
                            consecutive_failures += 1;
                            if !logged_error {
                                log::warn!("[ScreenCapture] Native display capture error: {}", e);
                                logged_error = true;
                            }
                            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                log::error!(
                                    "[ScreenCapture] Display capture stopped cleanly after {} consecutive errors: {}",
                                    consecutive_failures,
                                    e
                                );
                                capturing.store(false, Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                }
                std::thread::sleep(interval);
            }
            log::info!("Screen streaming capture thread exited");
        });

        Ok(())
    }

    pub fn stop_streaming(&self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        log::info!("Stopped native screen capture engine");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x11_image_conversion_to_rgba() {
        // 2x1 pixel test:
        // Pixel 0: B=10, G=20, R=30, pad=0
        // Pixel 1: B=255, G=128, R=64, pad=100
        let bgr_data = vec![10, 20, 30, 0, 255, 128, 64, 100];
        let rgba_24 = convert_x11_image_to_rgba(2, 1, 24, &bgr_data).expect("valid conversion");
        assert_eq!(rgba_24, vec![30, 20, 10, 255, 64, 128, 255, 255]);

        let rgba_32 = convert_x11_image_to_rgba(2, 1, 32, &bgr_data).expect("valid conversion");
        assert_eq!(rgba_32, vec![30, 20, 10, 255, 64, 128, 255, 100]);

        // Error path: insufficient buffer length
        let short_data = vec![10, 20, 30];
        let err = convert_x11_image_to_rgba(2, 1, 24, &short_data);
        assert!(err.is_err());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_capture_handles_headless_gracefully() {
        if std::env::var("DISPLAY").map_or(true, |d| d.trim().is_empty()) {
            let res = linux_capture::capture_primary_display();
            assert!(res.is_err());
            assert!(res.unwrap_err().contains("headless session"));
        }
    }
}
