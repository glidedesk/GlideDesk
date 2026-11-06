// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Fast file transfer protocol header & endpoint definitions (fx2).

pub const FX_PATH: &str = "/glidedesk/fx/";
pub const FX_HEADER: &str = "x-glidedesk-fx";
pub const PARALLEL_FILES: usize = 4;
pub const READ_CHUNK_SIZE: usize = 1024 * 1024; // 1 MiB disk read buffer

/// Check whether both peers support fast file streaming (fx2).
pub fn fx_supported(peer_fx: Option<i32>) -> bool {
    matches!(peer_fx, Some(v) if v >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fx_support_detection() {
        assert!(fx_supported(Some(1)));
        assert!(fx_supported(Some(2)));
        assert!(!fx_supported(Some(0)));
        assert!(!fx_supported(None));
    }
}
