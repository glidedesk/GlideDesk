// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Pure Rust Audio Capture & Streaming Engine using cpal

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct AudioEngine {
    is_capturing: Arc<AtomicBool>,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            is_capturing: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_capture(&self) -> Result<(), String> {
        if self.is_capturing.load(Ordering::SeqCst) {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| "No default audio input device available".to_string())?;

        let config = device.default_input_config()
            .map_err(|e| format!("Failed to get default input config: {}", e))?;

        log::info!("Starting pure Rust audio capture with config: {:?}", config);
        self.is_capturing.store(true, Ordering::SeqCst);
        let capturing = self.is_capturing.clone();
        let capturing_thread = self.is_capturing.clone();

        std::thread::spawn(move || {
            let err_fn = |err| {
                log::error!("Audio capture error: an error occurred on stream: {}", err);
            };

            let stream = device.build_input_stream(
                &config.config(),
                move |_data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !capturing.load(Ordering::SeqCst) {
                        return;
                    }
                },
                err_fn,
                None,
            );

            if let Ok(stream) = stream {
                if let Ok(()) = stream.play() {
                    while capturing_thread.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        });

        Ok(())
    }

    pub fn stop_capture(&self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        log::info!("Stopped pure Rust audio capture");
    }
}
