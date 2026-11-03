// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Pure Rust Screen Capture Engine for Cross-Platform Window/Display Streaming

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct ScreenEngine {
    is_capturing: Arc<AtomicBool>,
}

impl ScreenEngine {
    pub fn new() -> Self {
        Self {
            is_capturing: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_streaming(&self, fps: u32) -> Result<(), String> {
        if self.is_capturing.load(Ordering::SeqCst) {
            return Ok(());
        }

        log::info!("Starting pure Rust screen capture streaming at {} FPS", fps);
        self.is_capturing.store(true, Ordering::SeqCst);
        let capturing = self.is_capturing.clone();

        std::thread::spawn(move || {
            let interval = std::time::Duration::from_millis(1000 / (fps.max(1) as u64));
            while capturing.load(Ordering::SeqCst) {
                // Capture frame logic using native OS APIs or scrap/image buffers
                std::thread::sleep(interval);
            }
            log::info!("Screen capture streaming thread exited");
        });

        Ok(())
    }

    pub fn stop_streaming(&self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        log::info!("Stopped pure Rust screen capture streaming");
    }
}
