// Platform adapters — only where Slint/Windows forces us to (M8), plus the two
// seams an Android build has to fill: the file dialog and the data directory.
// Policy (ADR-0002): never implement TSF/IME ourselves.

pub mod dib;
pub mod picker;

use std::path::{Path, PathBuf};

/// Where a per-user library may live, or `None` when this OS has no such idea.
///
/// The placement rules in `storage::data_location` take the directory as a
/// parameter and read `%APPDATA%` themselves through `roaming_root()`. That is
/// right on a desktop and wrong here: Android has no per-user profile directory
/// in the environment (the app gets its own writable path from the Activity, and
/// the process's working directory is not writable at all), so the port has to
/// hand the path in rather than have it discovered. `android_main` stores it at
/// startup; until it does, `None` means the same thing it has always meant — the
/// session stays where it is.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        APP_DATA_DIR.get().cloned()
    }
    #[cfg(not(target_os = "android"))]
    {
        quire_core::storage::data_location::roaming_root()
    }
}

#[cfg(target_os = "android")]
static APP_DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Called once from `android_main`, before anything resolves a path.
#[cfg(target_os = "android")]
pub fn set_data_dir(dir: PathBuf) {
    let _ = APP_DATA_DIR.set(dir);
}

/// The window scale factor the Android shell pins, or `None` where the
/// platform's own factor is left alone.
///
/// The same seam as `data_dir`, for the same reason: the answer belongs to the
/// platform (`android_main` can read the Activity's `Configuration`, and
/// nothing below it can), while the *use* has to happen in `launcher::run`,
/// because a scale factor needs a `Window` and that is where one first exists.
/// `None` on a desktop is not "no scaling" — it means "whatever the window
/// system says", which is what the desktop has always done.
pub fn ui_scale() -> Option<f32> {
    #[cfg(target_os = "android")]
    {
        ANDROID_UI_SCALE.get().copied()
    }
    #[cfg(not(target_os = "android"))]
    {
        None
    }
}

#[cfg(target_os = "android")]
static ANDROID_UI_SCALE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();

/// Called once from `android_main`, before the window exists (M9.d).
#[cfg(target_os = "android")]
pub fn set_ui_scale(scale: f32) {
    let _ = ANDROID_UI_SCALE.set(scale);
}

/// Copy `text` to the system clipboard, as CF_UNICODETEXT via the same FFI
/// `read_clipboard` uses (ADR-0025's write half: `clip.exe`'s OEM-codepage
/// stdin garbles non-ASCII, and "Copy page as Markdown" must carry CJK).
/// No clipboard crate — user32/kernel32 are already linked for winit.
/// Other targets report failure instead of pretending.
pub fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        const CF_UNICODETEXT: u32 = 13;
        const GMEM_MOVEABLE: u32 = 0x0002;

        #[link(name = "user32")]
        extern "system" {
            fn OpenClipboard(hwnd: isize) -> i32;
            fn CloseClipboard() -> i32;
            fn EmptyClipboard() -> i32;
            fn SetClipboardData(format: u32, hmem: isize) -> isize;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
            fn GlobalLock(hmem: isize) -> *mut u16;
            fn GlobalUnlock(hmem: isize) -> i32;
            fn GlobalFree(hmem: isize) -> isize;
        }

        // UTF-16 units plus the terminating nul; the allocator wants bytes
        let mut units: Vec<u16> = text.encode_utf16().collect();
        units.push(0);
        let bytes = units.len() * std::mem::size_of::<u16>();

        unsafe {
            // another process may hold the clipboard open: a short retry
            // beats failing a copy over a transient lock
            let mut opened = false;
            for _ in 0..5 {
                if OpenClipboard(0) != 0 {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            if !opened {
                return false;
            }
            let ok = (|| {
                if EmptyClipboard() == 0 {
                    return false;
                }
                let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
                if handle == 0 {
                    return false;
                }
                let ptr = GlobalLock(handle);
                if ptr.is_null() {
                    GlobalFree(handle);
                    return false;
                }
                std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len());
                GlobalUnlock(handle);
                // success transfers ownership: the clipboard frees the block
                SetClipboardData(CF_UNICODETEXT, handle) != 0
            })();
            CloseClipboard();
            ok
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        false
    }
}

/// Open a folder in the system file manager (Windows: `explorer.exe <dir>`)
/// — the settings storage row's "Open folder" affordance. Only the launch
/// is checked; explorer returns odd exit codes by design.
pub fn open_folder(path: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .is_ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        false
    }
}

/// Open a file with whatever the system has registered for its type — an
/// attachment block's "Open" (SPEC §三十七 批次 A). `ShellExecuteW` rather than
/// `explorer.exe <path>`: explorer exits 0x1 on success by design, so the
/// subprocess can't tell a launched app from a blocked extension, while the
/// FFI call answers `> 32` only for a real launch. One call, so no `windows`
/// crate — the same rule that keeps user32 hand-declared above.
pub fn open_with_default(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        const SW_SHOWDEFAULT: i32 = 10;

        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                hwnd: isize,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show: i32,
            ) -> isize;
        }

        fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
            use std::os::windows::ffi::OsStrExt;
            s.encode_wide().chain(std::iter::once(0)).collect()
        }

        let verb = wide(std::ffi::OsStr::new("open"));
        let target = wide(path.as_os_str());
        // Below 33 the return value is an SE_ERR_* code, not an HINSTANCE.
        unsafe {
            ShellExecuteW(
                0,
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWDEFAULT,
            ) > 32
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        false
    }
}

/// Read the system clipboard as text (the rich-paste path, SPEC §二十七).
/// Direct Win32 FFI: a `Get-Clipboard` subprocess was measured at 7-10 s on
/// the dev desktop (PowerShell startup under AV), which no paste can wait
/// for, while `OpenClipboard`/`GetClipboardData` are microseconds and the
/// MSVC toolchain already links user32/kernel32 for winit — so no clipboard
/// crate is pulled in (dependency policy in DECISIONS). Reads
/// CF_UNICODETEXT only; other targets report absence instead of pretending.
pub fn read_clipboard() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        const CF_UNICODETEXT: u32 = 13;

        #[link(name = "user32")]
        extern "system" {
            fn IsClipboardFormatAvailable(format: u32) -> i32;
            fn OpenClipboard(hwnd: isize) -> i32;
            fn CloseClipboard() -> i32;
            fn GetClipboardData(format: u32) -> isize;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GlobalLock(hmem: isize) -> *mut u16;
            fn GlobalUnlock(hmem: isize) -> i32;
        }

        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
                return None;
            }
            // another process may hold the clipboard open: a short retry
            // beats failing a paste over a transient lock
            let mut opened = false;
            for _ in 0..5 {
                if OpenClipboard(0) != 0 {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            if !opened {
                return None;
            }
            let text = (|| {
                let handle = GetClipboardData(CF_UNICODETEXT);
                if handle == 0 {
                    return None;
                }
                let ptr = GlobalLock(handle);
                if ptr.is_null() {
                    return None;
                }
                let mut len = 0usize;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                GlobalUnlock(handle);
                Some(s)
            })();
            CloseClipboard();
            let text = text?.trim_end_matches(['\r', '\n']).to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Read the clipboard as a picture, returning PNG bytes (SPEC §三十七 批次 A's
/// last open item: paste a screenshot). CF_DIBV5 first because it is the format
/// that carries a real alpha channel, CF_DIB as the fallback every app writes.
/// Same rule as `read_clipboard`: hand-declared FFI, no clipboard crate, and the
/// decode itself lives in `dib` so it can be tested without a clipboard.
pub fn read_clipboard_image() -> Option<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        const CF_DIB: u32 = 8;
        const CF_DIBV5: u32 = 17;

        #[link(name = "user32")]
        extern "system" {
            fn IsClipboardFormatAvailable(format: u32) -> i32;
            fn OpenClipboard(hwnd: isize) -> i32;
            fn CloseClipboard() -> i32;
            fn GetClipboardData(format: u32) -> isize;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GlobalSize(hmem: isize) -> usize;
            // the same declaration `read_clipboard` makes — one symbol declared
            // twice with different types is a warning, so the cast happens here
            fn GlobalLock(hmem: isize) -> *mut u16;
            fn GlobalUnlock(hmem: isize) -> i32;
        }

        let format = [CF_DIBV5, CF_DIB]
            .into_iter()
            .find(|f| unsafe { IsClipboardFormatAvailable(*f) != 0 })?;
        unsafe {
            let mut opened = false;
            for _ in 0..5 {
                if OpenClipboard(0) != 0 {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            if !opened {
                return None;
            }
            let raw = (|| {
                let handle = GetClipboardData(format);
                if handle == 0 {
                    return None;
                }
                let size = GlobalSize(handle);
                let ptr = GlobalLock(handle).cast::<u8>();
                if ptr.is_null() || size == 0 {
                    return None;
                }
                let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
                GlobalUnlock(handle);
                Some(bytes)
            })();
            CloseClipboard();
            dib::dib_to_png(&raw?)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_write_and_read_round_trip_unicode() {
        // the write path is FFI now (ADR-0025's second half): CJK survives
        let sample = "中文标题\n\n- item **bold**\nquire://page/7";
        assert!(copy_to_clipboard(sample), "the FFI write must succeed");
        let read = read_clipboard().expect("the FFI read must succeed");
        assert_eq!(read, sample, "the UTF-16 round trip preserves the text");
    }

    /// The one check the decoder cannot make on its own: that the FFI reads a
    /// picture some *other* process put on the clipboard. Ignored because it
    /// reads the user's real clipboard rather than a fixture -- run it by hand
    /// after loading a PNG (`Set-Clipboard` cannot do this; `[Windows.Forms.Clipboard]::SetImage`
    /// can, from an STA session).
    #[test]
    #[ignore = "reads the user's real clipboard"]
    fn a_picture_another_process_put_on_the_clipboard_decodes() {
        let png = read_clipboard_image().expect("the clipboard holds no CF_DIB / CF_DIBV5");
        let img = image::load_from_memory(&png).expect("the bytes must be a PNG");
        println!(
            "clipboard picture: {}x{} from {} PNG bytes",
            img.width(),
            img.height(),
            png.len()
        );
        assert!(img.width() > 0 && img.height() > 0);
    }
}
