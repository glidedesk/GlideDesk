// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Input methods across computers.
//
// Keys typed with an input method (Japanese, Pinyin, Korean...) type a–z and the input
// method composes them, which the other computer never sees while the keys go over. So a
// key-down that types a letter also says the language it is being typed with (`im`), and
// while driven the other side switches to its own input method of that language, back to
// what it had afterwards. Keys without one change nothing.
//
// Backends: Fcitx 5 and IBus through their command lines on Linux; the text input sources
// on macOS; the keyboard layout and IME on Windows. A platform that cannot tell returns
// nothing and selection does nothing, so the wire stays backward compatible.

use std::sync::Mutex;

/// Longest language tag we accept (`ja`, `zh-Hans`, ...).
pub const MAX_LANG: usize = 12;

/// Whether `s` is a plausible language tag (`ja`, `ko`, `zh-Hans`) and may be acted on.
pub fn valid_lang(s: &str) -> Option<String> {
    if s.len() < 2 || s.len() > MAX_LANG || !s.is_ascii() {
        return None;
    }
    let (base, rest) = match s.find('-') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    if !(2..=3).contains(&base.len()) || !base.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    if let Some(rest) = rest {
        if !(2..=8).contains(&rest.len())
            || !rest.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return None;
        }
    }
    Some(s.to_string())
}

/// The language an input-method engine belongs to (Fcitx 5 and IBus names).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn lang_of(engine: &str) -> Option<&'static str> {
    let e = engine.trim();
    if e.is_empty() {
        return None;
    }
    if matches!(e, "mozc" | "mozc-jp" | "anthy" | "kkc" | "skk") {
        return Some("ja");
    }
    if matches!(
        e,
        "chewing" | "new-chewing" | "zhuyin" | "bopomofo" | "mcbopomofo"
    ) || prefixed_digits(e, "cangjie")
        || prefixed_digits(e, "quick")
        || prefixed_digits(e, "table:cangjie")
        || prefixed_digits(e, "table:quick")
    {
        return Some("zh-Hant");
    }
    if matches!(
        e,
        "pinyin" | "libpinyin" | "shuangpin" | "sunpinyin" | "googlepinyin" | "rime" | "wbx"
    ) || prefixed_word(e, "wubi")
        || prefixed_word(e, "table:wubi")
    {
        return Some("zh-Hans");
    }
    if e == "hangul" {
        return Some("ko");
    }
    None
}

fn prefixed_digits(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix)
        .is_some_and(|rest| rest.bytes().all(|b| b.is_ascii_digit()))
}

fn prefixed_word(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix).is_some_and(|rest| {
        rest.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

/// The language of the input method typed with on this computer, while it composes.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn current() -> Option<String> {
    backend::current()
}

/// Whether this computer can switch input methods at all.
#[allow(dead_code)]
pub fn available() -> bool {
    backend::available()
}

/// Switch to an input method of `lang` here; false when none is enabled.
pub fn select(lang: &str) -> bool {
    backend::select(lang)
}

/// Back to the input method in use before the last `select`.
pub fn restore() {
    backend::restore()
}

/// The language we switched to for the incoming keys, if any.
static IM_HERE: Mutex<Option<String>> = Mutex::new(None);

/// An incoming key-down carried `im`: switch our input method to match, or back when it
/// carried none. `shortcut` is true while Ctrl or Cmd is held, where input methods are
/// not involved.
pub fn incoming(im: Option<&str>, shortcut: bool) {
    if shortcut {
        return;
    }
    let lang = im.and_then(valid_lang);
    let mut here = IM_HERE.lock().unwrap_or_else(|e| e.into_inner());
    if lang.as_deref() == here.as_deref() {
        return;
    }
    if let Some(l) = lang.as_deref() {
        if !available() {
            return;
        }
        select(l);
    } else {
        restore();
    }
    *here = lang;
}

/// Driving ended: back to our own input method, forgetting any switch.
pub fn reset() {
    let mut here = IM_HERE.lock().unwrap_or_else(|e| e.into_inner());
    if here.is_some() {
        restore();
        *here = None;
    }
}

// ---------------------------------------------------------------------------
// Linux: Fcitx 5 and IBus through their own command lines, on X11 and Wayland.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod backend {
    use super::lang_of;
    use std::process::Command;
    use std::sync::Mutex;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Fcitx5,
        Ibus,
    }

    fn run(cmd: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(cmd).args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn kind() -> Option<Kind> {
        if run("fcitx5-remote", &["-n"]).is_some() {
            Some(Kind::Fcitx5)
        } else if run("ibus", &["engine"]).is_some() {
            Some(Kind::Ibus)
        } else {
            None
        }
    }

    /// (backend, engine in use, whether it composes).
    fn now() -> Option<(Kind, String, bool)> {
        match kind()? {
            Kind::Fcitx5 => {
                let engine = run("fcitx5-remote", &["-n"])?;
                let active = run("fcitx5-remote", &[]).as_deref() == Some("2");
                Some((Kind::Fcitx5, engine, active))
            }
            Kind::Ibus => Some((Kind::Ibus, run("ibus", &["engine"])?, true)),
        }
    }

    pub fn available() -> bool {
        kind().is_some()
    }

    pub fn current() -> Option<String> {
        let (_, engine, active) = now()?;
        if !active {
            return None;
        }
        lang_of(&engine).map(str::to_string)
    }

    /// `('ibus', 'name')` entries in GNOME's input sources.
    fn parse_gnome_sources(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let marker = "('ibus'";
        let mut rest = text;
        while let Some(i) = rest.find(marker) {
            let after = &rest[i + marker.len()..];
            let after = after.trim_start();
            let after = after.strip_prefix(',').map(str::trim_start).unwrap_or(after);
            if let Some(after) = after.strip_prefix('\'') {
                if let Some(end) = after.find('\'') {
                    out.push(after[..end].to_string());
                }
            }
            rest = &rest[i + marker.len()..];
        }
        out
    }

    fn parse_preload_engines(text: &str) -> Vec<String> {
        let Some(i) = text.find("preload-engines:") else {
            return Vec::new();
        };
        let rest = &text[i + "preload-engines:".len()..];
        let Some(a) = rest.find('[') else {
            return Vec::new();
        };
        let Some(b) = rest[a + 1..].find(']') else {
            return Vec::new();
        };
        rest[a + 1..a + 1 + b]
            .split(',')
            .map(|x| x.trim().trim_matches('\'').to_string())
            .filter(|x| !x.is_empty())
            .collect()
    }

    fn enabled(kind: Kind) -> Vec<String> {
        match kind {
            Kind::Ibus => {
                let mut names = Vec::new();
                if let Some(g) = run(
                    "gsettings",
                    &["get", "org.gnome.desktop.input-sources", "sources"],
                ) {
                    names.extend(parse_gnome_sources(&g));
                }
                if let Some(cfg) = run("ibus", &["read-config"]) {
                    names.extend(parse_preload_engines(&cfg));
                }
                for d in ["mozc-jp", "anthy", "libpinyin", "pinyin", "chewing", "hangul"] {
                    names.push(d.to_string());
                }
                names.sort();
                names.dedup();
                names
            }
            Kind::Fcitx5 => [
                "mozc", "anthy", "kkc", "skk", "chewing", "cangjie", "quick", "zhuyin", "pinyin",
                "shuangpin", "rime", "wbx", "hangul",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }

    static BEFORE: Mutex<Option<(String, bool)>> = Mutex::new(None);

    pub fn select(lang: &str) -> bool {
        let Some((kind, engine, active)) = now() else {
            return false;
        };
        if active && lang_of(&engine) == Some(lang) {
            return true;
        }
        for cand in enabled(kind) {
            if lang_of(&cand) != Some(lang) {
                continue;
            }
            match kind {
                Kind::Fcitx5 => {
                    run("fcitx5-remote", &["-s", cand.as_str()]);
                    if run("fcitx5-remote", &["-n"]).as_deref() != Some(cand.as_str()) {
                        continue; // not enabled here
                    }
                    run("fcitx5-remote", &["-o"]);
                }
                Kind::Ibus => {
                    run("ibus", &["engine", cand.as_str()]);
                    if run("ibus", &["engine"]).as_deref() != Some(cand.as_str()) {
                        continue; // not installed here
                    }
                }
            }
            let mut before = BEFORE.lock().unwrap_or_else(|e| e.into_inner());
            if before.is_none() {
                *before = Some((engine, active));
            }
            return true;
        }
        false
    }

    pub fn restore() {
        let was = {
            let mut before = BEFORE.lock().unwrap_or_else(|e| e.into_inner());
            before.take()
        };
        let Some((engine, active)) = was else {
            return;
        };
        if engine.is_empty() {
            return;
        }
        match kind() {
            Some(Kind::Fcitx5) => {
                run("fcitx5-remote", &["-s", engine.as_str()]);
                if !active {
                    run("fcitx5-remote", &["-c"]);
                }
            }
            Some(Kind::Ibus) => {
                run("ibus", &["engine", engine.as_str()]);
            }
            None => {}
        }
    }
}

// ---------------------------------------------------------------------------
// macOS: text input sources (TIS).
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod backend {
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::Mutex;

    type CFStringRef = *const c_void;
    type CFArrayRef = *const c_void;
    type CFDictionaryRef = *mut c_void;
    type CFIndex = isize;
    type CFStringEncoding = u32;
    type Boolean = u8;
    type TISInputSourceRef = *mut c_void;
    type OSStatus = i32;

    const UTF8: CFStringEncoding = 0x0800_0100;

    #[link(name = "Carbon", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kTISPropertyInputSourceCategory: CFStringRef;
        static kTISPropertyInputSourceIsSelectCapable: CFStringRef;
        static kTISPropertyInputSourceID: CFStringRef;
        static kTISPropertyInputSourceType: CFStringRef;
        static kTISPropertyInputSourceLanguages: CFStringRef;
        static kTISCategoryKeyboardInputSource: CFStringRef;
        static kTISTypeKeyboardInputMode: CFStringRef;
        static kCFBooleanTrue: *const c_void;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;

        fn TISCopyCurrentKeyboardInputSource() -> TISInputSourceRef;
        fn TISCreateInputSourceList(properties: CFDictionaryRef, all: Boolean) -> CFArrayRef;
        fn TISGetInputSourceProperty(source: TISInputSourceRef, key: CFStringRef) -> *const c_void;
        fn TISSelectInputSource(source: TISInputSourceRef) -> OSStatus;

        fn CFStringGetLength(s: CFStringRef) -> CFIndex;
        fn CFStringGetMaximumSizeForEncoding(len: CFIndex, enc: CFStringEncoding) -> CFIndex;
        fn CFStringGetCString(
            s: CFStringRef,
            buf: *mut u8,
            size: CFIndex,
            enc: CFStringEncoding,
        ) -> bool;
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            cstr: *const u8,
            enc: CFStringEncoding,
        ) -> CFStringRef;
        fn CFArrayGetCount(a: CFArrayRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(a: CFArrayRef, idx: CFIndex) -> *const c_void;
        fn CFDictionaryCreate(
            alloc: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            n: CFIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFDictionaryRef;
        fn CFRelease(cf: *const c_void);
    }

    fn cf_to_string(s: CFStringRef) -> Option<String> {
        if s.is_null() {
            return None;
        }
        unsafe {
            let len = CFStringGetLength(s);
            if len < 0 {
                return None;
            }
            let max = CFStringGetMaximumSizeForEncoding(len, UTF8);
            if max <= 0 {
                return None;
            }
            let mut buf = vec![0u8; max as usize + 1];
            let ok = CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as CFIndex, UTF8);
            if !ok {
                return None;
            }
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            String::from_utf8(buf[..end].to_vec()).ok()
        }
    }

    fn property_string(source: TISInputSourceRef, key: CFStringRef) -> Option<String> {
        unsafe {
            let p = TISGetInputSourceProperty(source, key);
            if p.is_null() {
                None
            } else {
                cf_to_string(p as CFStringRef)
            }
        }
    }

    fn source_id(source: TISInputSourceRef) -> String {
        unsafe { property_string(source, kTISPropertyInputSourceID).unwrap_or_default() }
    }

    /// An input method's mode (not a layout): its first language, "ja", "zh-Hans"...
    fn im_language(source: TISInputSourceRef) -> Option<String> {
        let kind = unsafe { property_string(source, kTISPropertyInputSourceType) }?;
        let mode = unsafe { cf_to_string(kTISTypeKeyboardInputMode) }?;
        if kind != mode {
            return None;
        }
        unsafe {
            let langs = TISGetInputSourceProperty(source, kTISPropertyInputSourceLanguages);
            if langs.is_null() {
                return None;
            }
            let arr = langs as CFArrayRef;
            if CFArrayGetCount(arr) < 1 {
                return None;
            }
            cf_to_string(CFArrayGetValueAtIndex(arr, 0) as CFStringRef)
        }
    }

    fn selectable_sources() -> Option<CFArrayRef> {
        unsafe {
            let keys = [
                kTISPropertyInputSourceCategory as *const c_void,
                kTISPropertyInputSourceIsSelectCapable as *const c_void,
            ];
            let values = [
                kTISCategoryKeyboardInputSource as *const c_void,
                kCFBooleanTrue as *const c_void,
            ];
            let props = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                2,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            if props.is_null() {
                return None;
            }
            let list = TISCreateInputSourceList(props, 0);
            CFRelease(props as *const c_void);
            if list.is_null() {
                None
            } else {
                Some(list)
            }
        }
    }

    pub fn available() -> bool {
        true
    }

    pub fn current() -> Option<String> {
        unsafe {
            let cur = TISCopyCurrentKeyboardInputSource();
            if cur.is_null() {
                return None;
            }
            let lang = im_language(cur);
            CFRelease(cur as *const c_void);
            match lang {
                Some(l) if l != "en" => Some(l),
                _ => None,
            }
        }
    }

    static BEFORE: Mutex<Option<String>> = Mutex::new(None);

    pub fn select(lang: &str) -> bool {
        unsafe {
            let cur = TISCopyCurrentKeyboardInputSource();
            if cur.is_null() {
                return false;
            }
            if im_language(cur).as_deref() == Some(lang) {
                CFRelease(cur as *const c_void);
                return true;
            }
            let Some(list) = selectable_sources() else {
                CFRelease(cur as *const c_void);
                return false;
            };
            let n = CFArrayGetCount(list);
            let mut best: Option<TISInputSourceRef> = None;
            let mut best_len = usize::MAX;
            for i in 0..n.max(0) {
                let source = CFArrayGetValueAtIndex(list, i) as TISInputSourceRef;
                if source.is_null() || im_language(source).as_deref() != Some(lang) {
                    continue;
                }
                let id_len = source_id(source).len();
                if id_len < best_len {
                    best_len = id_len;
                    best = Some(source);
                }
            }
            let Some(target) = best else {
                CFRelease(list as *const c_void);
                CFRelease(cur as *const c_void);
                return false;
            };
            {
                let mut before = BEFORE.lock().unwrap_or_else(|e| e.into_inner());
                if before.is_none() {
                    *before = Some(source_id(cur));
                }
            }
            let status = TISSelectInputSource(target);
            CFRelease(list as *const c_void);
            CFRelease(cur as *const c_void);
            status == 0
        }
    }

    pub fn restore() {
        let was = {
            let mut before = BEFORE.lock().unwrap_or_else(|e| e.into_inner());
            before.take()
        };
        let Some(id) = was else {
            return;
        };
        let Ok(cstr) = std::ffi::CString::new(id) else {
            return;
        };
        unsafe {
            let key = CFStringCreateWithCString(ptr::null(), cstr.as_ptr() as *const u8, UTF8);
            if key.is_null() {
                return;
            }
            let keys = [key as *const c_void];
            let values = [key as *const c_void];
            let props = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            CFRelease(key as *const c_void);
            if props.is_null() {
                return;
            }
            let list = TISCreateInputSourceList(props, 0);
            CFRelease(props as *const c_void);
            if list.is_null() {
                return;
            }
            if CFArrayGetCount(list) > 0 {
                let source = CFArrayGetValueAtIndex(list, 0) as TISInputSourceRef;
                if !source.is_null() {
                    TISSelectInputSource(source);
                }
            }
            CFRelease(list as *const c_void);
        }
    }
}

// ---------------------------------------------------------------------------
// Windows: the keyboard layout (HKL) and its IME.
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod backend {
    use std::ffi::c_void;
    use std::sync::Mutex;
    use std::thread::sleep;
    use std::time::Duration;

    type HWND = *mut c_void;
    type HKL = *mut c_void;
    type WPARAM = usize;
    type LPARAM = isize;
    type LRESULT = isize;

    const WM_INPUTLANGCHANGEREQUEST: u32 = 0x0050;
    const WM_IME_CONTROL: u32 = 0x0283;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;
    const IMC_GETCONVERSIONMODE: usize = 0x0001;
    const IMC_SETCONVERSIONMODE: usize = 0x0002;
    const IMC_GETOPENSTATUS: usize = 0x0005;
    const IMC_SETOPENSTATUS: usize = 0x0006;
    const IME_CMODE_NATIVE: i32 = 0x0001;

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> HWND;
        fn GetWindowThreadProcessId(hwnd: HWND, pid: *mut u32) -> u32;
        fn GetKeyboardLayout(thread: u32) -> HKL;
        fn GetKeyboardLayoutList(n: i32, list: *mut HKL) -> i32;
        fn SendMessageTimeoutW(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
            flags: u32,
            timeout: u32,
            result: *mut usize,
        ) -> LRESULT;
        fn PostMessageW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> i32;
    }

    #[link(name = "imm32")]
    extern "system" {
        fn ImmGetDefaultIMEWnd(hwnd: HWND) -> HWND;
    }

    fn language(hkl: HKL) -> Option<&'static str> {
        let id = (hkl as usize as u32) & 0xFFFF;
        match id {
            0x0804 | 0x1004 => Some("zh-Hans"),
            0x0404 | 0x0C04 | 0x1404 => Some("zh-Hant"),
            0x0411 => Some("ja"),
            0x0412 => Some("ko"),
            _ => None,
        }
    }

    fn control(ime: HWND, cmd: usize, arg: isize) -> i32 {
        if ime.is_null() {
            return -1;
        }
        unsafe {
            let mut result: usize = 0;
            let ok = SendMessageTimeoutW(
                ime,
                WM_IME_CONTROL,
                cmd,
                arg,
                SMTO_ABORTIFHUNG,
                200,
                &mut result,
            );
            if ok == 0 {
                -1
            } else {
                result as i32
            }
        }
    }

    pub fn available() -> bool {
        true
    }

    pub fn current() -> Option<String> {
        unsafe {
            let w = GetForegroundWindow();
            let thread = GetWindowThreadProcessId(w, std::ptr::null_mut());
            let hkl = GetKeyboardLayout(thread);
            let lang = language(hkl)?;
            let ime = ImmGetDefaultIMEWnd(w);
            let open = control(ime, IMC_GETOPENSTATUS, 0);
            let mode = control(ime, IMC_GETCONVERSIONMODE, 0);
            if open == 0 || (mode >= 0 && (mode & IME_CMODE_NATIVE) == 0) {
                None
            } else {
                Some(lang.to_string())
            }
        }
    }

    static BEFORE: Mutex<Option<usize>> = Mutex::new(None);

    pub fn select(lang: &str) -> bool {
        unsafe {
            let w = GetForegroundWindow();
            let thread = GetWindowThreadProcessId(w, std::ptr::null_mut());
            let cur = GetKeyboardLayout(thread);
            if language(cur) != Some(lang) {
                let mut list = [std::ptr::null_mut(); 64];
                let n = GetKeyboardLayoutList(list.len() as i32, list.as_mut_ptr());
                let mut found: HKL = std::ptr::null_mut();
                for slot in list.iter().take(n.max(0) as usize) {
                    if language(*slot) == Some(lang) {
                        found = *slot;
                        break;
                    }
                }
                if found.is_null() {
                    return false;
                }
                {
                    let mut before = BEFORE.lock().unwrap_or_else(|e| e.into_inner());
                    if before.is_none() {
                        *before = Some(cur as usize);
                    }
                }
                PostMessageW(w, WM_INPUTLANGCHANGEREQUEST, 0, found as LPARAM);
                sleep(Duration::from_millis(150)); // the window takes it in its own time
            }
            let ime = ImmGetDefaultIMEWnd(w);
            control(ime, IMC_SETOPENSTATUS, 1);
            let mode = control(ime, IMC_GETCONVERSIONMODE, 0);
            if mode >= 0 && (mode & IME_CMODE_NATIVE) == 0 {
                control(ime, IMC_SETCONVERSIONMODE, (mode | IME_CMODE_NATIVE) as isize);
            }
            true
        }
    }

    pub fn restore() {
        let was = {
            let mut before = BEFORE.lock().unwrap_or_else(|e| e.into_inner());
            before.take()
        };
        let Some(hkl) = was else {
            return;
        };
        unsafe {
            PostMessageW(
                GetForegroundWindow(),
                WM_INPUTLANGCHANGEREQUEST,
                0,
                hkl as LPARAM,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Anything else: no input-method access.
// ---------------------------------------------------------------------------

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod backend {
    pub fn available() -> bool {
        false
    }

    pub fn current() -> Option<String> {
        None
    }

    pub fn select(_lang: &str) -> bool {
        false
    }

    pub fn restore() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_languages_match_their_family() {
        assert_eq!(lang_of("mozc-jp"), Some("ja"));
        assert_eq!(lang_of("anthy"), Some("ja"));
        assert_eq!(lang_of("libpinyin"), Some("zh-Hans"));
        assert_eq!(lang_of("table:wubi"), Some("zh-Hans"));
        assert_eq!(lang_of("chewing"), Some("zh-Hant"));
        assert_eq!(lang_of("table:cangjie3"), Some("zh-Hant"));
        assert_eq!(lang_of("hangul"), Some("ko"));
    }

    #[test]
    fn plain_layouts_are_not_input_methods() {
        assert_eq!(lang_of("keyboard-us"), None);
        assert_eq!(lang_of("xkb:us::eng"), None);
        assert_eq!(lang_of(""), None);
    }

    #[test]
    fn language_tags_are_validated() {
        assert_eq!(valid_lang("ja").as_deref(), Some("ja"));
        assert_eq!(valid_lang("zh-Hans").as_deref(), Some("zh-Hans"));
        assert_eq!(valid_lang("ko"), Some("ko".to_string()));
        assert_eq!(valid_lang("J"), None);
        assert_eq!(valid_lang("japanese"), None);
        assert_eq!(valid_lang("zh_"), None);
        assert_eq!(valid_lang("zh-Hans-"), None);
        assert_eq!(valid_lang("x".repeat(13).as_str()), None);
        assert_eq!(valid_lang("-hans"), None);
        assert_eq!(valid_lang("zh-漢"), None);
    }
}