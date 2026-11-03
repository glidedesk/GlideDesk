// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Pure Rust Media Subsystems (Audio & Screen Streaming)

pub mod audio;
pub mod screen;
pub mod bridge;

pub use audio::AudioEngine;
pub use screen::ScreenEngine;
pub use bridge::NativeBridgeManager;

