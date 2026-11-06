// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Log files, a crash log and link statistics.
//
// What is printed with the `log` crate (and the platform helpers' stderr) also goes to
// glidedesk.log in the system's log folder, kept to MAX_BYTES (5 MB) x MAX_FILES (3):
// glidedesk.log, glidedesk.1.log, glidedesk.2.log. Crashes (a panic) go to crash.log as
// well and the app goes on as before. While connected, once every LINK_PERIOD_S (30 s),
// one line on the link: round-trip p50/p95, pointer moves a second, bytes queued.
//
// `init()` as early as possible; `dir()` is where the files live ("Open log folder");
// `start_link_stats(sample)` runs the periodic line, `sample()` -> None when not connected,
// else `Some(LinkSample { rtt, queued })`, asked every second.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub const MAX_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_FILES: usize = 3;
pub const LINK_PERIOD_S: u32 = 30;

const ACTIVE_LOG: &str = "glidedesk.log";
const CRASH_LOG: &str = "crash.log";

/// One sample of the link, taken every second while connected.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinkSample {
    /// Round-trip time in milliseconds, when known.
    pub rtt: Option<u32>,
    /// Bytes waiting to go out on the socket (estimate).
    pub queued: u64,
}

/// The log directory for this OS (created on demand).
///
/// macOS: ~/Library/Logs/GlideDesk; Windows: %APPDATA%\GlideDesk\logs;
/// Linux: $XDG_CONFIG_HOME/glidedesk/logs or ~/.config/glidedesk/logs.
pub fn dir() -> PathBuf {
    if let Some(dir) = platform_dir() {
        return dir;
    }
    std::env::temp_dir().join("glidedesk-logs")
}

#[cfg(target_os = "macos")]
fn platform_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/GlideDesk"))
}

#[cfg(target_os = "windows")]
fn platform_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("GlideDesk").join("logs"))
}

#[cfg(target_os = "linux")]
fn platform_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("glidedesk").join("logs"));
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/glidedesk/logs"))
}

/// Rotating log sink plus crash log. Pure filesystem logic, unit tested.
pub struct LogSink {
    dir: PathBuf,
    max_bytes: u64,
    max_files: usize,
    file: Option<File>,
    written: u64,
}

impl LogSink {
    pub fn new(dir: PathBuf, max_bytes: u64, max_files: usize) -> Self {
        let _ = fs::create_dir_all(&dir);
        let mut sink = Self {
            dir,
            max_bytes,
            max_files: max_files.max(1),
            file: None,
            written: 0,
        };
        sink.open_active();
        sink
    }

    fn active_path(&self) -> PathBuf {
        self.dir.join(ACTIVE_LOG)
    }

    /// Open the active file for append (used at start, appends to an existing log).
    fn open_active(&mut self) {
        match OpenOptions::new().create(true).append(true).open(self.active_path()) {
            Ok(f) => {
                self.written = f.metadata().map(|m| m.len()).unwrap_or(0);
                self.file = Some(f);
            }
            Err(_) => self.file = None,
        }
    }

    fn open_fresh(&mut self) {
        match OpenOptions::new().create(true).write(true).truncate(true).open(self.active_path()) {
            Ok(f) => {
                self.written = 0;
                self.file = Some(f);
            }
            Err(_) => self.file = None,
        }
    }

    fn numbered(&self, n: usize) -> PathBuf {
        self.dir.join(format!("glidedesk.{}.log", n))
    }

    /// Rotate: drop the oldest, shift .n -> .n+1, then active -> .1 and start fresh.
    fn rotate(&mut self) {
        self.file = None;
        let last = self.max_files - 1;
        let _ = fs::remove_file(self.numbered(last));
        for i in (1..last).rev() {
            let _ = fs::rename(self.numbered(i), self.numbered(i + 1));
        }
        if last >= 1 {
            let _ = fs::rename(self.active_path(), self.numbered(1));
        }
        self.open_fresh();
    }

    /// Write a chunk, rotating first when it would push the file past max_bytes.
    pub fn write(&mut self, buf: &[u8]) {
        if self.max_bytes > 0 && self.written + buf.len() as u64 > self.max_bytes {
            self.rotate();
        }
        if let Some(f) = self.file.as_mut() {
            if f.write_all(buf).is_ok() {
                self.written += buf.len() as u64;
                let _ = f.flush();
            }
        }
    }

    /// Append to crash.log only (crash entries also go through `write` for the main log).
    pub fn write_crash(&mut self, buf: &[u8]) {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(self.dir.join(CRASH_LOG)) {
            let _ = f.write_all(buf);
            let _ = f.flush();
        }
    }
}

/// `log` backend writer: formats its records into the sink and also keeps stderr.
struct LogWriter {
    sink: Arc<Mutex<LogSink>>,
    tee_stderr: bool,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.tee_stderr {
            let _ = io::stderr().write_all(buf);
        }
        if let Ok(mut sink) = self.sink.lock() {
            sink.write(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

static SINK: OnceLock<Arc<Mutex<LogSink>>> = OnceLock::new();
static MOVES: AtomicU64 = AtomicU64::new(0);

/// Install the file backend and the panic hook. Safe to call once; later calls are ignored.
pub fn init() {
    let dir = dir();
    let sink = LogSink::new(dir.clone(), MAX_BYTES, MAX_FILES);
    let sink = SINK.get_or_init(|| Arc::new(Mutex::new(sink))).clone();

    let writer = LogWriter {
        sink: sink.clone(),
        tee_stderr: true,
    };

    let mut builder = env_logger::Builder::new();
    builder.target(env_logger::Target::Pipe(Box::new(writer)));
    if std::env::var_os("RUST_LOG").is_some() {
        builder.parse_default_env();
    } else {
        builder.filter_level(log::LevelFilter::Info);
    }
    let _ = builder.try_init();

    install_panic_hook(sink);

    log::info!("[Log] GlideDesk {} logging to {}", env!("CARGO_PKG_VERSION"), dir.display());
}

fn install_panic_hook(sink: Arc<Mutex<LogSink>>) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let line = format!("{} PANIC {}\n", chrono::Local::now().to_rfc3339(), info);
        let _ = io::stderr().write_all(line.as_bytes());
        if let Ok(mut s) = sink.lock() {
            s.write_crash(line.as_bytes());
            s.write(line.as_bytes());
        }
        default_hook(info);
    }));
}

/// Count a coalesced pointer move (called by the KVM path; see module kvm-moves).
pub fn note_move(n: u64) {
    MOVES.fetch_add(n, Ordering::Relaxed);
}

/// Total pointer moves counted so far.
pub fn moves() -> u64 {
    MOVES.load(Ordering::Relaxed)
}

/// The `[link]` line, factored out so its shape is unit tested.
pub fn format_link_line(p50: u32, p95: u32, moves_per_s: u64, queued_max: u64) -> String {
    format!(
        "[link] rtt p50 {} ms / p95 {} ms, kvm moves {}/s, queued max {} bytes",
        p50, p95, moves_per_s, queued_max
    )
}

/// Nearest-rank percentile (0 when empty).
fn percentile(sorted: &[u32], p: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

/// Sample the link every second; every LINK_PERIOD_S seconds while connected emit one
/// `[link]` line. `sample()` returns None when not connected, which resets the window.
pub fn start_link_stats<F>(sample: F)
where
    F: Fn() -> Option<LinkSample> + Send + 'static,
{
    std::thread::Builder::new()
        .name("glidedesk-link-stats".to_string())
        .spawn(move || {
            let mut rtts: Vec<u32> = Vec::new();
            let mut queued_max: u64 = 0;
            let mut window: u32 = 0;
            let mut last_moves = moves();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                match sample() {
                    None => {
                        rtts.clear();
                        queued_max = 0;
                        window = 0;
                        last_moves = moves();
                    }
                    Some(s) => {
                        if let Some(r) = s.rtt {
                            rtts.push(r);
                        }
                        queued_max = queued_max.max(s.queued);
                        window += 1;
                        if window >= LINK_PERIOD_S {
                            rtts.sort_unstable();
                            let p50 = percentile(&rtts, 0.50);
                            let p95 = percentile(&rtts, 0.95);
                            let now = moves();
                            let per_s = now.saturating_sub(last_moves) / LINK_PERIOD_S as u64;
                            log::info!("{}", format_link_line(p50, p95, per_s, queued_max));
                            rtts.clear();
                            queued_max = 0;
                            window = 0;
                            last_moves = now;
                        }
                    }
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn tmp_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        let dir = std::env::temp_dir().join(format!("glidedesk-applog-{}-{}-{}", tag, std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn writes_and_appends() {
        let dir = tmp_dir("append");
        let mut sink = LogSink::new(dir.clone(), MAX_BYTES, MAX_FILES);
        sink.write(b"first\n");
        sink.write(b"second\n");
        drop(sink);

        let body = fs::read_to_string(dir.join(ACTIVE_LOG)).unwrap();
        assert!(body.contains("first"));
        assert!(body.contains("second"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotates_and_drops_oldest() {
        let dir = tmp_dir("rotate");
        // max 10 bytes => each write of 6 bytes triggers a rotation after the first.
        let mut sink = LogSink::new(dir.clone(), 10, 3);
        for i in 0..6 {
            sink.write(format!("ev{}\n", i).as_bytes());
        }
        drop(sink);

        assert!(dir.join(ACTIVE_LOG).exists(), "active log missing");
        assert!(dir.join("glidedesk.1.log").exists(), "rotation .1 missing");
        assert!(dir.join("glidedesk.2.log").exists(), "rotation .2 missing");
        // Only three generations are ever kept.
        assert!(!dir.join("glidedesk.3.log").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_log_written() {
        let dir = tmp_dir("crash");
        let mut sink = LogSink::new(dir.clone(), MAX_BYTES, MAX_FILES);
        sink.write_crash(b"PANIC boom\n");
        drop(sink);

        let body = fs::read_to_string(dir.join(CRASH_LOG)).unwrap();
        assert!(body.contains("PANIC boom"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn link_line_shape() {
        let line = format_link_line(12, 40, 55, 4096);
        assert_eq!(line, "[link] rtt p50 12 ms / p95 40 ms, kvm moves 55/s, queued max 4096 bytes");
    }

    #[test]
    fn percentile_nearest_rank() {
        assert_eq!(percentile(&[], 0.5), 0);
        assert_eq!(percentile(&[10], 0.95), 10);
        let v = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&v, 0.50), 60);
        assert_eq!(percentile(&v, 0.95), 100);
    }

    #[test]
    fn move_counter_accumulates() {
        // Note: the counter is process-global; assert monotonic growth only.
        let base = moves();
        note_move(3);
        note_move(2);
        assert!(moves() >= base + 5);
    }
}
