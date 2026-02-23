use std::thread::sleep;
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::logger;
use crate::types::*;
use super::{Platform, WindowHandle};

// PrintWindow is in user32 / gdi32 but not always re-exported; declare directly.
extern "system" {
    fn PrintWindow(hwnd: HWND, hdcBlt: HDC, nFlags: u32) -> BOOL;
}

// HWND wraps *mut c_void which is !Send; we know window handles are safe to
// send across threads (they are process-wide integers), so we wrap isize.
struct SendHwnd(isize);
unsafe impl Send for SendHwnd {}
impl SendHwnd {
    fn hwnd(&self) -> HWND {
        HWND(self.0 as *mut std::ffi::c_void)
    }
    fn is_null(&self) -> bool {
        self.0 == 0
    }
}

// Virtual key codes (matches win32.js VK_CODES)
fn vk_code(key: &str) -> Option<u16> {
    match key {
        "enter" | "return" => Some(0x0D),
        "up"               => Some(0x26),
        "down"             => Some(0x28),
        "left"             => Some(0x25),
        "right"            => Some(0x27),
        "escape" | "esc"   => Some(0x1B),
        "space"            => Some(0x20),
        "tab"              => Some(0x09),
        "backspace"        => Some(0x08),
        "delete"           => Some(0x2E),
        "="                => Some(0xBB),
        "-"                => Some(0xBD),
        k if k.len() == 1 => {
            let c = k.chars().next().unwrap();
            if c.is_ascii_alphanumeric() {
                Some(c.to_ascii_uppercase() as u16)
            } else {
                None
            }
        }
        _ => None,
    }
}

// Parse "ctrl+shift+k" -> (modifiers_vk, main_key_str)
fn parse_key(key: &str) -> (Vec<u16>, String) {
    let parts: Vec<&str> = key.split('+').collect();
    let mut mods: Vec<u16> = Vec::new();
    for part in &parts[..parts.len().saturating_sub(1)] {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" | "cmd" => mods.push(VK_CONTROL.0),
            "shift"                    => mods.push(VK_SHIFT.0),
            "alt"                      => mods.push(VK_MENU.0),
            _                          => {}
        }
    }
    let main = parts.last().copied().unwrap_or(key).to_string();
    (mods, main)
}

// Build a keyboard INPUT (key-down or key-up) for a virtual key
unsafe fn key_input(vk: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk:         VIRTUAL_KEY(vk),
                wScan:       0,
                dwFlags:     if key_up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time:        0,
                dwExtraInfo: 0,
            },
        },
    }
}

// Build a Unicode keyboard INPUT for type_text
unsafe fn unicode_input(ch: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk:         VIRTUAL_KEY(0),
                wScan:       ch,
                dwFlags:     KEYEVENTF_UNICODE | if key_up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time:        0,
                dwExtraInfo: 0,
            },
        },
    }
}

// Build a mouse INPUT
unsafe fn mouse_input(flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx:          0,
                dy:          0,
                mouseData:   0,
                dwFlags:     flags,
                time:        0,
                dwExtraInfo: 0,
            },
        },
    }
}

// --- EnumWindows callback data ---
struct EnumData {
    re:      regex::Regex,
    windows: Vec<(WindowId, String)>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let data = &mut *(lparam.0 as *mut EnumData);

    if !IsWindowVisible(hwnd).as_bool() {
        return TRUE;
    }

    let mut buf = [0u16; 512];
    let len = GetWindowTextW(hwnd, &mut buf);
    if len <= 0 {
        return TRUE;
    }

    let title = String::from_utf16_lossy(&buf[..len as usize]);
    if data.re.is_match(&title) {
        logger::info_p("win32", &format!("found window: \"{}\" (hwnd: {})", title, hwnd.0 as u64));
        data.windows.push((hwnd.0 as u64 as WindowId, title));
    }

    TRUE
}

// ─── Platform ────────────────────────────────────────────────────────────────

pub struct Win32Platform;

impl Win32Platform {
    pub fn new() -> Self {
        Win32Platform
    }
}

impl Platform for Win32Platform {
    fn get_instances(&self, pattern: &str) -> Vec<(WindowId, String)> {
        let re = match regex::Regex::new(&format!("(?i){}", pattern)) {
            Ok(r) => r,
            Err(e) => {
                logger::error_p("win32", &format!("invalid pattern '{}': {}", pattern, e));
                return Vec::new();
            }
        };

        let mut data = Box::new(EnumData { re, windows: Vec::new() });
        unsafe {
            let _ = EnumWindows(
                Some(enum_windows_proc),
                LPARAM(data.as_mut() as *mut EnumData as isize),
            );
        }
        data.windows
    }

    fn create_window(&self, pattern: &str, window_id: WindowId) -> Box<dyn WindowHandle> {
        let mut win = Win32Window {
            _pattern: pattern.to_string(),
            hwnd:     SendHwnd(window_id as isize),
            title:   String::new(),
            region:  None,
        };
        win.do_update();
        Box::new(win)
    }
    fn start_hotkey_listener(&self, flag: Arc<AtomicBool>) {
        use std::ffi::c_void;

        type HWND  = *mut c_void;
        type BOOL  = i32;
        type UINT  = u32;
        type WPARAM = usize;
        type LPARAM = isize;
        type DWORD = u32;
        type LONG  = i32;

        #[repr(C)] struct POINT { x: LONG, y: LONG }
        #[repr(C)] struct MSG {
            hwnd:    HWND,
            message: UINT,
            w_param: WPARAM,
            l_param: LPARAM,
            time:    DWORD,
            pt:      POINT,
        }

        const MOD_CONTROL:  u32 = 0x0002;
        const MOD_SHIFT:    u32 = 0x0004;
        const MOD_NOREPEAT: u32 = 0x4000;
        const VK_K:         u32 = 0x4B;
        const WM_HOTKEY:    u32 = 0x0312;
        const HOTKEY_ID:    i32 = 1;

        extern "system" {
            fn RegisterHotKey(hwnd: HWND, id: i32, fs_modifiers: UINT, vk: UINT) -> BOOL;
            fn GetMessageW(
                msg: *mut MSG, hwnd: HWND,
                msg_filter_min: UINT, msg_filter_max: UINT,
            ) -> BOOL;
        }

        std::thread::spawn(move || unsafe {
            let ok = RegisterHotKey(
                std::ptr::null_mut(), HOTKEY_ID,
                MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_K,
            );
            if ok == 0 {
                crate::logger::error(
                    "failed to register global hotkey Ctrl+Shift+K — \
                     another application may have claimed it",
                );
                return;
            }
            crate::logger::info("global hotkey Ctrl+Shift+K registered");
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY && msg.w_param == HOTKEY_ID as usize {
                    flag.store(true, Ordering::Release);
                }
            }
        });
    }

    fn activate_terminal(&self) {
        use std::ffi::c_void;
        type HWND = *mut c_void;
        type BOOL = i32;
        const SW_RESTORE: i32 = 9;
        extern "system" {
            fn GetConsoleWindow() -> HWND;
            fn SetForegroundWindow(hwnd: HWND) -> BOOL;
            fn ShowWindow(hwnd: HWND, cmd_show: i32) -> BOOL;
        }
        unsafe {
            let hwnd = GetConsoleWindow();
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
        }
    }}

// ─── WindowHandle ─────────────────────────────────────────────────────────────

struct Win32Window {
    _pattern: String,
    hwnd:     SendHwnd,
    title:    String,
    region:   Option<Region>,
}

impl Win32Window {
    fn do_update(&mut self) {
        unsafe {
            if self.hwnd.is_null() || !IsWindow(self.hwnd.hwnd()).as_bool() {
                self.region = None;
                return;
            }

            // Refresh title
            let mut buf = [0u16; 512];
            let len = GetWindowTextW(self.hwnd.hwnd(), &mut buf);
            if len > 0 {
                self.title = String::from_utf16_lossy(&buf[..len as usize]);
            }

            // Get bounding rect
            let mut rect = RECT::default();
            if GetWindowRect(self.hwnd.hwnd(), &mut rect).is_ok() {
                let l = rect.left;
                let t = rect.top;
                let r = rect.right;
                let b = rect.bottom;
                let w = r - l;
                let h = b - t;
                self.region = Some(Region {
                    l, t, r, b, w, h,
                    cx: l + w / 2,
                    cy: t + h / 2,
                });
            } else {
                self.region = None;
            }
        }
    }

    fn do_activate(&self) {
        unsafe {
            let _ = SetForegroundWindow(self.hwnd.hwnd());
            let _ = SetActiveWindow(self.hwnd.hwnd());
            let _ = SetFocus(self.hwnd.hwnd());
        }
    }

    fn send_vk(&self, vk: u16) {
        unsafe {
            let down = key_input(vk, false);
            let up   = key_input(vk, true);
            SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(20));
            SendInput(&[up],   std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(20));
        }
    }

    fn send_unicode_char(&self, ch: u16) {
        unsafe {
            let down = unicode_input(ch, false);
            let up   = unicode_input(ch, true);
            SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(20));
            SendInput(&[up],   std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(20));
        }
    }
}

impl WindowHandle for Win32Window {
    fn id(&self) -> WindowId {
        self.hwnd.0 as u64
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn region(&self) -> Option<Region> {
        self.region
    }

    fn update(&mut self) {
        self.do_update();
    }

    fn activate(&mut self) {
        if self.region.is_none() {
            self.do_update();
        }
        self.do_activate();
    }

    fn click_relative(&mut self, x_ratio: f64, y_ratio: f64) {
        self.do_update();
        let region = match self.region {
            Some(r) => r,
            None => {
                logger::warn_p("win32", &format!(
                    "[{}]: window not found for click_relative", self.title
                ));
                return;
            }
        };

        let screen_x = region.l + (x_ratio * region.w as f64) as i32;
        let screen_y = region.t + (y_ratio * region.h as f64) as i32;

        self.do_activate();
        sleep(Duration::from_millis(50));

        unsafe {
            SetCursorPos(screen_x, screen_y).ok();
            sleep(Duration::from_millis(50));

            let down = mouse_input(MOUSEEVENTF_LEFTDOWN);
            SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(50));

            let up = mouse_input(MOUSEEVENTF_LEFTUP);
            SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(50));
        }
    }

    fn tap(&mut self, key: &str) {
        self.do_activate();
        sleep(Duration::from_millis(100));

        let (mods, main_key) = parse_key(key);

        // Press modifiers
        unsafe {
            for &m in &mods {
                let input = key_input(m, false);
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                sleep(Duration::from_millis(20));
            }
        }

        // Press main key (VK or Unicode)
        if let Some(vk) = vk_code(&main_key.to_lowercase()) {
            self.send_vk(vk);
        } else if main_key.len() == 1 {
            // Fallback: treat single unknown char as unicode
            let ch = main_key.chars().next().unwrap() as u16;
            self.send_unicode_char(ch);
        } else {
            logger::warn_p("win32", &format!("unknown key: {}", main_key));
        }

        // Release modifiers in reverse
        unsafe {
            for &m in mods.iter().rev() {
                let input = key_input(m, true);
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                sleep(Duration::from_millis(20));
            }
        }
    }

    fn type_text(&mut self, text: &str) {
        self.do_activate();
        sleep(Duration::from_millis(100));
        for ch in text.encode_utf16() {
            self.send_unicode_char(ch);
        }
    }

    fn capture(&mut self, rect: Option<CaptureRect>) -> Option<Capture> {
        self.do_update();
        let region = self.region?;

        unsafe {
            // Bring window forward so PrintWindow works reliably
            let _ = SetForegroundWindow(self.hwnd.hwnd());

            let screen_dc = GetDC(self.hwnd.hwnd());
            if screen_dc.is_invalid() {
                logger::warn_p("win32", "GetDC failed");
                return None;
            }

            let full_w = region.w;
            let full_h = region.h;

            let mem_dc     = CreateCompatibleDC(screen_dc);
            let bitmap     = CreateCompatibleBitmap(screen_dc, full_w, full_h);
            let old_bitmap = SelectObject(mem_dc, bitmap);

            // PW_RENDERFULLCONTENT = 0x00000002
            if !PrintWindow(self.hwnd.hwnd(), mem_dc, 0x2u32).as_bool() {
                // Fallback without flags
                let _ = PrintWindow(self.hwnd.hwnd(), mem_dc, 0u32);
            }

            let result = if let Some(cr) = rect {
                // Crop region
                let crop_dc     = CreateCompatibleDC(screen_dc);
                let crop_bitmap = CreateCompatibleBitmap(screen_dc, cr.w, cr.h);
                let crop_old    = SelectObject(crop_dc, crop_bitmap);

                let _ = BitBlt(crop_dc, 0, 0, cr.w, cr.h, mem_dc, cr.l, cr.t, SRCCOPY);

                let data = read_dib_bits(crop_dc, crop_bitmap, cr.w, cr.h);

                SelectObject(crop_dc, crop_old);
                let _ = DeleteObject(crop_bitmap);
                let _ = DeleteDC(crop_dc);

                data.map(|d| Capture {
                    data:         d,
                    width:        cr.w as u32,
                    height:       cr.h as u32,
                    bytes_per_row: (cr.w as u32) * 4,
                })
            } else {
                let data = read_dib_bits(mem_dc, bitmap, full_w, full_h);
                data.map(|d| Capture {
                    data:          d,
                    width:         full_w as u32,
                    height:        full_h as u32,
                    bytes_per_row: (full_w as u32) * 4,
                })
            };

            SelectObject(mem_dc, old_bitmap);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(self.hwnd.hwnd(), screen_dc);

            result
        }
    }
}

// Read a 32-bit top-down DIB from (dc, bitmap, w, h) -> BGRA bytes
unsafe fn read_dib_bits(dc: HDC, bitmap: HBITMAP, w: i32, h: i32) -> Option<Vec<u8>> {
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize:          std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth:         w,
            biHeight:        -h, // negative = top-down
            biPlanes:        1,
            biBitCount:      32,
            biCompression:   BI_RGB.0,
            biSizeImage:     0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed:       0,
            biClrImportant:  0,
        },
        bmiColors: [RGBQUAD::default()],
    };

    let mut buf: Vec<u8> = vec![0u8; (w * h * 4) as usize];
    let lines = GetDIBits(
        dc,
        bitmap,
        0,
        h as u32,
        Some(buf.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );

    if lines == 0 {
        logger::warn_p("win32", "GetDIBits failed");
        return None;
    }

    Some(buf)
}
