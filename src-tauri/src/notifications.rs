// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Notifications mirroring.
//
// Each computer reads its own notification list once a second and sends what is new
// to the paired computer, which shows it as a native notification from GlideDesk.
// The other direction works the same way, and both appear in the Notifications tab.
//
// Where the lists are read from:
// - Windows: `%LOCALAPPDATA%\Microsoft\Windows\Notifications\wpndatabase.db` through
//   `winsqlite3.dll`, which ships with Windows 10+. (The UserNotificationListener API
//   would need package identity, which a plain desktop build does not have.)
// - macOS: `~/Library/Group Containers/group.com.apple.usernoted/db2/db`, the
//   Notification Center store. Reading it requires Full Disk Access for GlideDesk;
//   without it the tab reports `readable: false` and only receiving works.
//
// Duplicates are stopped in both directions: a guard drops the same app/title/body
// twice within 15 seconds, and a notification shown here that was received from the
// other computer is never read back and sent out again (the "echo guard").

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::commands::push_state;
use crate::network::{NetworkManager, WireMessage};
use crate::state::{AppState, ClipboardItem, NotifApp};

/// The longest title/body kept from one notification, matching the helper ports.
const CUT: usize = 500;
/// The same app and text inside this window is a repeat, not a new notification.
const GUARD_WINDOW: Duration = Duration::from_secs(15);
/// A notification received from the other computer is in this list so the local
/// reader does not mistake GlideDesk's own toast for a new notification.
const ECHO_WINDOW: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct NotifEvent {
    pub id: String,
    pub app: String,
    pub app_name: String,
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub at: u64,
}

struct Guard {
    last: HashMap<String, Instant>,
}

impl Guard {
    fn new() -> Self {
        Self { last: HashMap::new() }
    }

    /// True when the same parts were let through a moment ago.
    fn repeat(&mut self, parts: &[&str]) -> bool {
        let key = parts
            .iter()
            .map(|p| p.chars().take(200).collect::<String>())
            .collect::<Vec<_>>()
            .join("\u{1}");
        let now = Instant::now();
        if let Some(prev) = self.last.get(&key) {
            if now.duration_since(*prev) < GUARD_WINDOW {
                return true;
            }
        }
        self.last.insert(key, now);
        if self.last.len() > 300 {
            self.last.retain(|_, at| now.duration_since(*at) < GUARD_WINDOW);
            if self.last.len() > 300 {
                if let Some(oldest) = self.last.keys().next().cloned() {
                    self.last.remove(&oldest);
                }
            }
        }
        false
    }
}

struct Echo {
    title: String,
    body: String,
    until: Instant,
}

struct Runtime {
    guard: Guard,
    echoes: Vec<Echo>,
}

static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();

fn runtime() -> &'static Mutex<Runtime> {
    RUNTIME.get_or_init(|| {
        Mutex::new(Runtime {
            guard: Guard::new(),
            echoes: Vec::new(),
        })
    })
}

fn cut(s: &str) -> String {
    s.chars().take(CUT).collect()
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

/// GlideDesk's own notifications are never forwarded; they would only come back.
fn is_own(app: &str, app_name: &str) -> bool {
    let app = app.to_ascii_lowercase();
    let name = app_name.to_ascii_lowercase();
    app.contains("glidedesk") || name.contains("glidedesk")
}

fn is_muted(app: &str, app_name: &str, muted: &[String]) -> bool {
    muted.iter().any(|m| {
        m.eq_ignore_ascii_case(app) || (!app_name.is_empty() && m.eq_ignore_ascii_case(app_name))
    })
}

/// A friendly name from an app identifier, for the cases where the system does not
/// give us one (macOS gives only a bundle id; Windows an AUMID or an exe path).
fn friendly_name(id: &str) -> String {
    if id.is_empty() {
        return String::new();
    }
    // Windows AUMIDs: "Microsoft.Windows.Explorer", "5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App",
    // or a path to an executable.
    let mut s = id;
    if let Some((left, _)) = s.split_once('!') {
        s = left;
    }
    if let Some((left, _)) = s.split_once('_') {
        s = left;
    }
    let mut last = if s.contains('\\') || s.contains('/') {
        s.rsplit(['\\', '/']).next().unwrap_or(s)
    } else {
        s.rsplit('.').next().unwrap_or(s)
    };
    if last.len() > 4 && last.to_ascii_lowercase().ends_with(".exe") {
        last = &last[..last.len() - 4];
    }
    if last.is_empty() {
        return id.to_string();
    }
    let mut chars = last.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => id.to_string(),
    }
}

// ---------------------------------------------------------------------------
// The store readers
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod store {
    use super::{friendly_name, cut, now_ms, NotifEvent};
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_void};

    type Db = *mut c_void;
    type Stmt = *mut c_void;

    const SQLITE_OPEN_READONLY: c_int = 1;
    const SQLITE_ROW: c_int = 100;

    #[link(name = "sqlite3")]
    extern "C" {
        fn sqlite3_open_v2(
            filename: *const c_char,
            db: *mut Db,
            flags: c_int,
            vfs: *const c_char,
        ) -> c_int;
        fn sqlite3_close_v2(db: Db) -> c_int;
        fn sqlite3_busy_timeout(db: Db, ms: c_int) -> c_int;
        fn sqlite3_prepare_v2(
            db: Db,
            sql: *const c_char,
            bytes: c_int,
            stmt: *mut Stmt,
            tail: *mut *const c_char,
        ) -> c_int;
        fn sqlite3_bind_int64(stmt: Stmt, index: c_int, value: i64) -> c_int;
        fn sqlite3_step(stmt: Stmt) -> c_int;
        fn sqlite3_finalize(stmt: Stmt) -> c_int;
        fn sqlite3_column_int64(stmt: Stmt, col: c_int) -> i64;
        fn sqlite3_column_double(stmt: Stmt, col: c_int) -> f64;
        fn sqlite3_column_text(stmt: Stmt, col: c_int) -> *const c_char;
        fn sqlite3_column_blob(stmt: Stmt, col: c_int) -> *const c_void;
        fn sqlite3_column_bytes(stmt: Stmt, col: c_int) -> c_int;
    }

    fn db_path() -> Option<String> {
        let home = std::env::var("HOME").ok()?;
        Some(format!(
            "{home}/Library/Group Containers/group.com.apple.usernoted/db2/db"
        ))
    }

    fn open() -> Option<Db> {
        let path = db_path()?;
        if !std::path::Path::new(&path).exists() {
            return None;
        }
        let c = CString::new(path).ok()?;
        let mut db: Db = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_open_v2(c.as_ptr(), &mut db, SQLITE_OPEN_READONLY, std::ptr::null())
        };
        if rc != 0 {
            if !db.is_null() {
                unsafe { sqlite3_close_v2(db) };
            }
            return None;
        }
        unsafe { sqlite3_busy_timeout(db, 300) };
        Some(db)
    }

    fn prepare(db: Db, sql: &str) -> Option<Stmt> {
        let c = CString::new(sql).ok()?;
        let mut st: Stmt = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_prepare_v2(db, c.as_ptr(), -1, &mut st, std::ptr::null_mut())
        };
        if rc == 0 && !st.is_null() {
            Some(st)
        } else {
            None
        }
    }

    pub fn readable() -> bool {
        let Some(db) = open() else { return false };
        let c = CString::new("SELECT count(*) FROM record LIMIT 1").unwrap();
        let mut st: Stmt = std::ptr::null_mut();
        let rc = unsafe {
            sqlite3_prepare_v2(db, c.as_ptr(), -1, &mut st, std::ptr::null_mut())
        };
        let ok = rc == 0 && !st.is_null();
        if !st.is_null() {
            unsafe { sqlite3_finalize(st) };
        }
        unsafe { sqlite3_close_v2(db) };
        ok
    }

    pub fn max_id() -> Option<i64> {
        let db = open()?;
        let st = prepare(db, "SELECT IFNULL(MAX(rec_id), 0) FROM record");
        let mut out = None;
        if let Some(st) = st {
            if unsafe { sqlite3_step(st) } == SQLITE_ROW {
                out = Some(unsafe { sqlite3_column_int64(st, 0) });
            }
            unsafe { sqlite3_finalize(st) };
        }
        unsafe { sqlite3_close_v2(db) };
        out
    }

    fn column_string(st: Stmt, col: c_int) -> String {
        unsafe {
            let p = sqlite3_column_text(st, col);
            if p.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(p).to_string_lossy().to_string()
            }
        }
    }

    /// The binary property list's `req` dictionary: title, subtitle, body.
    fn parse_payload(data: &[u8]) -> Option<(String, String, String)> {
        let value = plist::Value::from_reader(std::io::Cursor::new(data)).ok()?;
        let dict = value.as_dictionary()?;
        let req = dict.get("req")?.as_dictionary()?;
        let title = req.get("titl").and_then(|v| v.as_string()).unwrap_or("");
        let subtitle = req.get("subt").and_then(|v| v.as_string()).unwrap_or("");
        let body = req.get("body").and_then(|v| v.as_string()).unwrap_or("");
        if title.is_empty() && body.is_empty() {
            return None;
        }
        Some((cut(title), cut(subtitle), cut(body)))
    }

    pub fn poll(after: i64) -> Option<(Vec<NotifEvent>, i64)> {
        let db = open()?;
        let Some(st) = prepare(
            db,
            "SELECT r.rec_id, a.identifier, r.data, r.delivered_date, r.request_date \
             FROM record r JOIN app a ON a.app_id = r.app_id \
             WHERE r.rec_id > ? ORDER BY r.rec_id LIMIT 50",
        ) else {
            unsafe { sqlite3_close_v2(db) };
            return None;
        };
        unsafe { sqlite3_bind_int64(st, 1, after) };
        let mut last = after;
        let mut out = Vec::new();
        while unsafe { sqlite3_step(st) } == SQLITE_ROW {
            let id = unsafe { sqlite3_column_int64(st, 0) };
            last = last.max(id);
            let app = column_string(st, 1);
            let n = unsafe { sqlite3_column_bytes(st, 2) }.max(0) as usize;
            let payload = if n == 0 {
                Vec::new()
            } else {
                let p = unsafe { sqlite3_column_blob(st, 2) };
                if p.is_null() {
                    Vec::new()
                } else {
                    unsafe { std::slice::from_raw_parts(p as *const u8, n).to_vec() }
                }
            };
            if app.is_empty() || app.to_ascii_lowercase().contains("glidedesk") {
                continue;
            }
            let Some((title, subtitle, body)) = parse_payload(&payload) else {
                continue;
            };
            let delivered = unsafe { sqlite3_column_double(st, 3) };
            let requested = unsafe { sqlite3_column_double(st, 4) };
            let seconds = if delivered > 0.0 {
                delivered
            } else if requested > 0.0 {
                requested
            } else {
                0.0
            };
            let at = if seconds > 0.0 {
                ((seconds + 978_307_200.0) * 1000.0).max(0.0) as u64
            } else {
                now_ms()
            };
            out.push(NotifEvent {
                id: id.to_string(),
                app_name: friendly_name(&app),
                app,
                title,
                subtitle,
                body,
                at,
            });
        }
        unsafe { sqlite3_finalize(st) };
        unsafe { sqlite3_close_v2(db) };
        Some((out, last))
    }
}

#[cfg(target_os = "windows")]
mod store {
    use super::{cut, friendly_name, now_ms, NotifEvent};
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_void};
    use std::sync::OnceLock;

    type Db = *mut c_void;
    type Stmt = *mut c_void;

    const SQLITE_OPEN_READONLY: c_int = 1;
    const SQLITE_ROW: c_int = 100;

    type OpenV2 = unsafe extern "C" fn(*const c_char, *mut Db, c_int, *const c_char) -> c_int;
    type CloseV2 = unsafe extern "C" fn(Db) -> c_int;
    type BusyTimeout = unsafe extern "C" fn(Db, c_int) -> c_int;
    type Prepare16 = unsafe extern "C" fn(Db, *const u16, c_int, *mut Stmt, *mut *const u16) -> c_int;
    type BindInt64 = unsafe extern "C" fn(Stmt, c_int, i64) -> c_int;
    type Step = unsafe extern "C" fn(Stmt) -> c_int;
    type Finalize = unsafe extern "C" fn(Stmt) -> c_int;
    type ColumnInt64 = unsafe extern "C" fn(Stmt, c_int) -> i64;
    type ColumnText16 = unsafe extern "C" fn(Stmt, c_int) -> *const u16;
    type ColumnBlob = unsafe extern "C" fn(Stmt, c_int) -> *const c_void;
    type ColumnBytes = unsafe extern "C" fn(Stmt, c_int) -> c_int;

    struct Api {
        open_v2: OpenV2,
        close_v2: CloseV2,
        busy_timeout: BusyTimeout,
        prepare16: Prepare16,
        bind_int64: BindInt64,
        step: Step,
        finalize: Finalize,
        column_int64: ColumnInt64,
        column_text16: ColumnText16,
        column_blob: ColumnBlob,
        column_bytes: ColumnBytes,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    unsafe fn symbol<T: Copy>(module: *mut c_void, name: &str) -> Option<T> {
        let c = CString::new(name).ok()?;
        let p = GetProcAddress(module, c.as_ptr());
        if p.is_null() {
            None
        } else {
            Some(std::mem::transmute_copy::<*mut c_void, T>(&p))
        }
    }

    fn api() -> Option<&'static Api> {
        static API: OnceLock<Option<Api>> = OnceLock::new();
        API.get_or_init(|| unsafe {
            let module = LoadLibraryA(b"winsqlite3.dll\0".as_ptr() as *const c_char);
            if module.is_null() {
                return None;
            }
            Some(Api {
                open_v2: symbol(module, "sqlite3_open_v2")?,
                close_v2: symbol(module, "sqlite3_close_v2")?,
                busy_timeout: symbol(module, "sqlite3_busy_timeout")?,
                prepare16: symbol(module, "sqlite3_prepare16_v2")?,
                bind_int64: symbol(module, "sqlite3_bind_int64")?,
                step: symbol(module, "sqlite3_step")?,
                finalize: symbol(module, "sqlite3_finalize")?,
                column_int64: symbol(module, "sqlite3_column_int64")?,
                column_text16: symbol(module, "sqlite3_column_text16")?,
                column_blob: symbol(module, "sqlite3_column_blob")?,
                column_bytes: symbol(module, "sqlite3_column_bytes")?,
            })
        })
        .as_ref()
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn db_path() -> Option<String> {
        let local = std::env::var("LOCALAPPDATA").ok()?;
        Some(format!(
            r"{local}\Microsoft\Windows\Notifications\wpndatabase.db"
        ))
    }

    fn open(a: &Api) -> Option<Db> {
        let path = db_path()?;
        if !std::path::Path::new(&path).exists() {
            return None;
        }
        let c = CString::new(path).ok()?;
        let mut db: Db = std::ptr::null_mut();
        let rc = unsafe { (a.open_v2)(c.as_ptr(), &mut db, SQLITE_OPEN_READONLY, std::ptr::null()) };
        if rc != 0 {
            if !db.is_null() {
                unsafe { (a.close_v2)(db) };
            }
            return None;
        }
        unsafe { (a.busy_timeout)(db, 500) };
        Some(db)
    }

    fn prepare(a: &Api, db: Db, sql: &str) -> Option<Stmt> {
        let w = wide(sql);
        let mut st: Stmt = std::ptr::null_mut();
        let rc = unsafe {
            (a.prepare16)(db, w.as_ptr(), -1, &mut st, std::ptr::null_mut())
        };
        if rc == 0 && !st.is_null() {
            Some(st)
        } else {
            None
        }
    }

    pub fn readable() -> bool {
        let Some(a) = api() else { return false };
        let Some(db) = open(a) else { return false };
        let result = prepare(a, db, "SELECT count(*) FROM Notification")
            .and_then(|st| {
                let ok = unsafe { (a.step)(st) } == SQLITE_ROW;
                unsafe { (a.finalize)(st) };
                if ok { Some(()) } else { None }
            })
            .is_some();
        unsafe { (a.close_v2)(db) };
        result
    }

    pub fn max_id() -> Option<i64> {
        let a = api()?;
        let db = open(a)?;
        let st = prepare(a, db, "SELECT IFNULL(MAX(Id), 0) FROM Notification");
        let mut out = None;
        if let Some(st) = st {
            if unsafe { (a.step)(st) } == SQLITE_ROW {
                out = Some(unsafe { (a.column_int64)(st, 0) });
            }
            unsafe { (a.finalize)(st) };
        }
        unsafe { (a.close_v2)(db) };
        out
    }

    fn column_string(a: &Api, st: Stmt, col: c_int) -> String {
        unsafe {
            let p = (a.column_text16)(st, col);
            if p.is_null() {
                return String::new();
            }
            // Text16 is NUL terminated; column_bytes() would convert it to
            // UTF-8 first, so measure the wide string itself.
            let mut len = 0usize;
            while len < 4096 && *p.add(len) != 0 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(p, len);
            String::from_utf16_lossy(slice)
        }
    }

    fn unescape(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(i) = rest.find('&') {
            out.push_str(&rest[..i]);
            rest = &rest[i..];
            let Some(end) = rest.find(';') else {
                out.push_str(rest);
                return out;
            };
            let entity = &rest[1..end];
            let replacement = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                    u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32)
                }
                _ if entity.starts_with('#') => {
                    entity[1..].parse::<u32>().ok().and_then(char::from_u32)
                }
                _ => None,
            };
            match replacement {
                Some(c) => out.push(c),
                None => out.push_str(&rest[..=end]),
            }
            rest = &rest[end + 1..];
        }
        out.push_str(rest);
        out
    }

    fn strip_tags(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut depth = 0usize;
        for c in s.chars() {
            match c {
                '<' => depth += 1,
                '>' if depth > 0 => depth -= 1,
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out
    }

    /// Toast XML: the first <text> of the first binding is the title, the rest the body.
    fn parse_payload(xml: &str) -> Option<(String, String)> {
        let start = xml.find("<binding")?;
        let end = xml[start..]
            .find("</binding>")
            .map(|i| start + i)
            .unwrap_or(xml.len());
        let block = &xml[start..end];
        let mut texts = Vec::new();
        let mut rest = block;
        while let Some(i) = rest.find("<text") {
            let after = &rest[i + 5..];
            let Some(open_end) = after.find('>') else { break };
            let attrs = &after[..open_end];
            let content_start = i + 5 + open_end + 1;
            let Some(close_rel) = rest[content_start..].find("</text>") else {
                break;
            };
            let content = &rest[content_start..content_start + close_rel];
            if !attrs.contains("placement=\"attribution\"") {
                let v = unescape(&strip_tags(content));
                let v = v.trim();
                if !v.is_empty() {
                    texts.push(cut(v));
                }
            }
            rest = &rest[content_start + close_rel + 7..];
        }
        if texts.is_empty() {
            return None;
        }
        let title = texts.remove(0);
        let body = cut(&texts.join("\n"));
        Some((title, body))
    }

    pub fn poll(after: i64) -> Option<(Vec<NotifEvent>, i64)> {
        let a = api()?;
        let db = open(a)?;
        let st = prepare(
            a,
            db,
            "SELECT n.Id, h.PrimaryId, n.Payload, n.ArrivalTime FROM Notification n \
             JOIN NotificationHandler h ON h.RecordId = n.HandlerId \
             WHERE n.Id > ? AND n.Type = 'toast' ORDER BY n.Id LIMIT 50",
        );
        let Some(st) = st else {
            unsafe { (a.close_v2)(db) };
            return None;
        };
        unsafe { (a.bind_int64)(st, 1, after) };
        let mut last = after;
        let mut out = Vec::new();
        while unsafe { (a.step)(st) } == SQLITE_ROW {
            let id = unsafe { (a.column_int64)(st, 0) };
            last = last.max(id);
            let app = column_string(a, st, 1);
            let n = unsafe { (a.column_bytes)(st, 2) }.max(0) as usize;
            let payload = if n == 0 {
                Vec::new()
            } else {
                let p = unsafe { (a.column_blob)(st, 2) };
                if p.is_null() {
                    Vec::new()
                } else {
                    unsafe { std::slice::from_raw_parts(p as *const u8, n).to_vec() }
                }
            };
            let arrival = unsafe { (a.column_int64)(st, 3) };
            if app.is_empty() || app.to_ascii_lowercase().contains("glidedesk") {
                continue;
            }
            let xml = String::from_utf8_lossy(&payload);
            let Some((title, body)) = parse_payload(xml.trim_end_matches('\0')) else {
                continue;
            };
            let at = if arrival > 0 {
                ((arrival / 10_000) - 11_644_473_600_000).max(0) as u64
            } else {
                now_ms()
            };
            out.push(NotifEvent {
                id: id.to_string(),
                app_name: friendly_name(&app),
                app,
                title,
                subtitle: String::new(),
                body,
                at,
            });
        }
        unsafe { (a.finalize)(st) };
        unsafe { (a.close_v2)(db) };
        Some((out, last))
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
mod store {
    use super::NotifEvent;

    pub fn readable() -> bool {
        false
    }
    pub fn max_id() -> Option<i64> {
        None
    }
    pub fn poll(_after: i64) -> Option<(Vec<NotifEvent>, i64)> {
        None
    }
}

// ---------------------------------------------------------------------------
// Engine: read locally, send over the link
// ---------------------------------------------------------------------------

/// Start the reader. It runs for the life of the app: one tick a second, doing work
/// only while the tab is on, sending is on, and the other computer accepts.
pub fn start(app: AppHandle, state: AppState, network: NetworkManager) {
    let spawn = std::thread::Builder::new()
        .name("glidedesk-notifs".into())
        .spawn(move || {
            let mut watching = false;
            let mut last_id = -1i64;
            let mut readable: Option<bool> = None;
            let mut next_probe = Instant::now();
            let mut warned_unreadable = false;

            loop {
                std::thread::sleep(Duration::from_millis(1000));

                let (enabled, send, receive, hide_text, muted) = {
                    let s = state.inner.read();
                    (
                        s.notifs.enabled,
                        s.notifs.send,
                        s.notifs.receive,
                        s.notifs.hide_text,
                        s.notifs.muted.clone(),
                    )
                };
                let _ = receive;
                let want = enabled && send && network.notif_accept();

                // The tab asks whether this computer's list can be read even while
                // nothing is being sent; probe once in a while, not every tick.
                let mut changed = false;
                if readable.is_none() || Instant::now() >= next_probe {
                    let now_readable = store::readable();
                    next_probe = Instant::now() + Duration::from_secs(5);
                    if readable != Some(now_readable) {
                        readable = Some(now_readable);
                        changed = true;
                    }
                    if !now_readable {
                        if !warned_unreadable {
                            warned_unreadable = true;
                            log::info!(
                                "[Notifs] This computer's notification list cannot be read; \
                                 only receiving works"
                            );
                        }
                        watching = false;
                        last_id = -1;
                    }
                }
                if changed {
                    let mut s = state.inner.write();
                    if let Some(r) = readable {
                        s.notifs.readable = r;
                    }
                    drop(s);
                    push_state(&app, &state);
                }

                if !want {
                    watching = false;
                    last_id = -1;
                    continue;
                }

                if !watching {
                    // Only what arrives from now on: the existing list is history.
                    if let Some(max) = store::max_id() {
                        last_id = max;
                        watching = true;
                    }
                    continue;
                }

                let Some((events, newest)) = store::poll(last_id) else {
                    continue;
                };
                last_id = newest;

                for ev in events {
                    if is_own(&ev.app, &ev.app_name) {
                        continue;
                    }
                    let mut rt = runtime().lock().unwrap();
                    let now = Instant::now();
                    rt.echoes.retain(|e| e.until > now);
                    if !ev.body.is_empty()
                        && rt
                            .echoes
                            .iter()
                            .any(|e| e.title == ev.title && ev.body.contains(&e.body))
                    {
                        continue;
                    }
                    if rt.guard.repeat(&["out", &ev.app, &ev.title, &ev.body]) {
                        continue;
                    }
                    drop(rt);

                    let muted_app = is_muted(&ev.app, &ev.app_name, &muted);
                    {
                        let mut s = state.inner.write();
                        s.notifs.apps.retain(|a| !a.app.eq_ignore_ascii_case(&ev.app));
                        s.notifs.apps.insert(
                            0,
                            NotifApp {
                                app: ev.app.clone(),
                                app_name: ev.app_name.clone(),
                            },
                        );
                        s.notifs.apps.truncate(40);
                    }
                    if muted_app {
                        push_state(&app, &state);
                        continue;
                    }

                    let body = if hide_text { String::new() } else { ev.body.clone() };
                    network.send_packet_sync(WireMessage {
                        t: "notif".to_string(),
                        v: Some(1),
                        id: Some(ev.id.clone()),
                        app: Some(ev.app.clone()),
                        app_name: Some(ev.app_name.clone()),
                        title: Some(ev.title.clone()),
                        subtitle: Some(ev.subtitle.clone()),
                        body: Some(body),
                        at: Some(ev.at),
                        ..Default::default()
                    });
                    {
                        let mut s = state.inner.write();
                        s.notifs.sent = s.notifs.sent.saturating_add(1);
                    }
                    log::info!(
                        "[Notifs] Sent a notification from {} to the other computer",
                        ev.app_name
                    );
                    push_state(&app, &state);
                }
            }
        });
    if let Err(e) = spawn {
        log::warn!("[Notifs] Could not start the reader thread: {}", e);
    }
}

// ---------------------------------------------------------------------------
// Received: show natively and keep in the tab
// ---------------------------------------------------------------------------

/// A `notif` message from the other computer.
pub fn incoming(app: &AppHandle, state: &AppState, pkt: &WireMessage) {
    let (enabled, receive) = {
        let s = state.inner.read();
        (s.notifs.enabled, s.notifs.receive)
    };
    if !enabled || !receive {
        log::debug!("[Notifs] Incoming notification ignored: receiving is off here");
        return;
    }

    let app_id = pkt.app.clone().unwrap_or_default();
    let app_name = pkt
        .app_name
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| friendly_name(&app_id));
    let title = cut(pkt.title.as_deref().unwrap_or(""));
    let subtitle = cut(pkt.subtitle.as_deref().unwrap_or(""));
    let body = cut(pkt.body.as_deref().unwrap_or(""));
    if title.is_empty() && body.is_empty() && app_name.is_empty() {
        return;
    }
    if is_own(&app_id, &app_name) {
        return;
    }

    {
        let mut rt = runtime().lock().unwrap();
        if rt.guard.repeat(&["in", &app_id, &title, &body]) {
            log::debug!("[Notifs] Incoming notification is a repeat; not shown twice");
            return;
        }
        // Read back from this computer's own notification store later: never send it on.
        let echo_body = body.chars().take(80).collect::<String>();
        rt.echoes.push(Echo {
            title: title.clone(),
            body: echo_body,
            until: Instant::now() + ECHO_WINDOW,
        });
        if rt.echoes.len() > 100 {
            rt.echoes.remove(0);
        }
    }

    let at = pkt.at.filter(|v| *v > 0).unwrap_or_else(now_ms);
    let id = pkt
        .id
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}-{}", at, app_id));

    {
        let mut s = state.inner.write();
        s.notifs.shown = s.notifs.shown.saturating_add(1);
        s.notifs.history.retain(|h| h.id != id);
        s.notifs.history.insert(
            0,
            ClipboardItem {
                id,
                text: String::new(),
                timestamp: at,
                dir: "in".to_string(),
                size: 0,
                image: None,
                app: app_id.clone(),
                app_name: app_name.clone(),
                title: title.clone(),
                subtitle: subtitle.clone(),
                body: body.clone(),
            },
        );
        s.notifs.history.truncate(50);
        s.notifs.apps.retain(|a| !a.app.eq_ignore_ascii_case(&app_id));
        s.notifs.apps.insert(
            0,
            NotifApp {
                app: app_id.clone(),
                app_name: app_name.clone(),
            },
        );
        s.notifs.apps.truncate(40);
    }

    let display_title = if title.is_empty() {
        if app_name.is_empty() { "GlideDesk".to_string() } else { app_name.clone() }
    } else {
        title.clone()
    };
    let mut lines: Vec<String> = Vec::new();
    if !app_name.is_empty() && app_name != display_title {
        lines.push(app_name.clone());
    }
    if !subtitle.is_empty() {
        lines.push(subtitle.clone());
    }
    if !body.is_empty() {
        lines.push(body.clone());
    }
    let display_body = lines.join("\n");

    let shown = app
        .notification()
        .builder()
        .title(display_title)
        .body(display_body)
        .show();
    if let Err(e) = shown {
        log::warn!("[Notifs] Could not show the notification: {}", e);
    } else {
        log::info!("[Notifs] Showed a notification from {} here", app_name);
    }

    push_state(app, state);
}

/// The other computer's `notif.policy`: whether it wants to receive ours.
pub fn set_peer_policy(app: &AppHandle, state: &AppState, accept: bool) {
    if let Some(net) = app.try_state::<NetworkManager>() {
        net.set_notif_accept(accept);
    }
    {
        let mut s = state.inner.write();
        s.notifs.peer_accepts = accept;
    }
    push_state(app, state);
    log::info!(
        "[Notifs] The other computer {} our notifications",
        if accept { "accepts" } else { "does not accept" }
    );
}

/// Send this computer's current receive choice, so the other side knows whether to send.
pub fn send_policy(network: &NetworkManager, state: &AppState) {
    let accept = {
        let s = state.inner.read();
        s.notifs.enabled && s.notifs.receive
    };
    network.send_packet_sync(WireMessage {
        t: "notif.policy".to_string(),
        v: Some(1),
        accept: Some(accept),
        ..Default::default()
    });
}

/// A small view of the engine for logs/tests.
#[allow(dead_code)]
pub fn debug_view() -> serde_json::Value {
    let rt = runtime().lock().unwrap();
    json!({
        "echoes": rt.echoes.len(),
        "guardKeys": rt.guard.last.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_notifications_are_never_forwarded() {
        assert!(is_own("com.glidedesk.desktop", "GlideDesk"));
        assert!(is_own("com.apple.notificationcenterui", "GlideDesk"));
        assert!(!is_own("com.apple.mail", "Mail"));
    }

    #[test]
    fn guard_drops_a_repeat_and_lets_a_new_one_through() {
        let mut g = Guard::new();
        assert!(!g.repeat(&["out", "mail", "Hello", "Body"]));
        assert!(g.repeat(&["out", "mail", "Hello", "Body"]));
        assert!(!g.repeat(&["out", "mail", "Hello", "Other"]));
    }

    #[test]
    fn friendly_names_read_well() {
        assert_eq!(friendly_name("Microsoft.Windows.Explorer"), "Explorer");
        assert_eq!(
            friendly_name("5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App"),
            "WhatsAppDesktop"
        );
        assert_eq!(friendly_name("com.apple.mail"), "Mail");
        assert_eq!(friendly_name("C:\\Apps\\notepad.exe"), "Notepad");
    }
}
