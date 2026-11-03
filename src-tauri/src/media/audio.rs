// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Real WebSocket Audio Streaming Engine using cpal & tokio-tungstenite

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use parking_lot::Mutex;

pub struct AudioEngine {
    is_capturing: Arc<AtomicBool>,
    audio_tx: Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
}

impl AudioEngine {
    pub fn new() -> (Self, mpsc::Receiver<Vec<u8>>) {
        let (tx, rx) = mpsc::channel(100);
        (
            Self {
                is_capturing: Arc::new(AtomicBool::new(false)),
                audio_tx: Arc::new(Mutex::new(Some(tx))),
            },
            rx,
        )
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

        log::info!("Starting real cpal audio capture stream: {:?}", config);
        self.is_capturing.store(true, Ordering::SeqCst);
        let capturing = self.is_capturing.clone();
        let capturing_thread = self.is_capturing.clone();
        let tx = self.audio_tx.lock().clone();

        std::thread::spawn(move || {
            let err_fn = |err| {
                log::error!("Audio capture error: {}", err);
            };

            let stream = device.build_input_stream(
                &config.config(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !capturing.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Some(ref sender) = tx {
                        let mut bytes = Vec::with_capacity(data.len() * 4);
                        for &sample in data {
                            bytes.extend_from_slice(&sample.to_le_bytes());
                        }
                        let _ = sender.try_send(bytes);
                    }
                },
                err_fn,
                None,
            );

            if let Ok(stream) = stream {
                if let Ok(()) = stream.play() {
                    while capturing_thread.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
            }
        });

        Ok(())
    }

    pub fn stop_capture(&self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        log::info!("Stopped real cpal audio capture stream");
    }
}
