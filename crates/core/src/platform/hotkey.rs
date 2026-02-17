use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Start a background thread that listens for the global hotkey Cmd+Shift+K.
/// Sets `flag` to `true` when the hotkey is pressed.
#[cfg(target_os = "macos")]
pub fn start_hotkey_listener(flag: Arc<AtomicBool>) {
    use std::ffi::c_void;

    // CGEventTap FFI types and functions
    type CGEventTapProxy = *mut c_void;
    type CGEventRef = *mut c_void;
    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;
    type CGEventMask = u64;
    type CGEventType = u32;
    type CGEventFlags = u64;

    type CGEventTapCallBack = unsafe extern "C" fn(
        CGEventTapProxy,
        CGEventType,
        CGEventRef,
        *mut c_void,
    ) -> CGEventRef;

    const K_CG_HID_EVENT_TAP: u32 = 0; // kCGHIDEventTap
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
    const CG_EVENT_KEY_DOWN: u32 = 10;
    const CG_EVENT_FLAGS_CHANGED: u32 = 12;
    const CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;

    const K_CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 0x00080000;
    const K_CG_EVENT_FLAG_MASK_SHIFT: u64 = 0x00020000;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;
    const K_CG_EVENT_FLAG_MASK_CONTROL: u64 = 0x00040000;

    const KEYCODE_K: i64 = 40;

    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: CGEventMask,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> CFMachPortRef;

        fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: CFMachPortRef,
            order: i64,
        ) -> CFRunLoopSourceRef;

        fn CFRunLoopGetCurrent() -> CFRunLoopRef;

        fn CFRunLoopAddSource(
            rl: CFRunLoopRef,
            source: CFRunLoopSourceRef,
            mode: CFStringRef,
        );

        fn CFRunLoopRun();

        fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);

        static kCFRunLoopCommonModes: CFStringRef;
    }

    // Keyboard event keycode field
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

    unsafe extern "C" fn hotkey_callback(
        _proxy: CGEventTapProxy,
        event_type: CGEventType,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef {
        unsafe {
            // Re-enable tap if it was disabled by timeout
            if event_type == CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
                // user_info stores (flag_ptr, tap_ptr) — but we don't have tap here easily.
                // We handle this in a simpler way: just return the event.
                return event;
            }

            if event_type != CG_EVENT_KEY_DOWN {
                return event;
            }

            let flags = CGEventGetFlags(event);
            let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE);

            let has_cmd = (flags & K_CG_EVENT_FLAG_MASK_COMMAND) != 0;
            let has_shift = (flags & K_CG_EVENT_FLAG_MASK_SHIFT) != 0;
            let no_alt = (flags & K_CG_EVENT_FLAG_MASK_ALTERNATE) == 0;
            let no_ctrl = (flags & K_CG_EVENT_FLAG_MASK_CONTROL) == 0;

            if keycode == KEYCODE_K && has_cmd && has_shift && no_alt && no_ctrl {
                let flag = &*(user_info as *const AtomicBool);
                flag.store(true, Ordering::Release);
            }

            event
        }
    }

    std::thread::spawn(move || {
        unsafe {
            let mask: CGEventMask = (1 << CG_EVENT_KEY_DOWN) | (1 << CG_EVENT_FLAGS_CHANGED);
            let flag_ptr = Arc::into_raw(flag) as *mut c_void;

            let tap = CGEventTapCreate(
                K_CG_HID_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                mask,
                hotkey_callback,
                flag_ptr,
            );

            if tap.is_null() {
                crate::logger::error(
                    "failed to create event tap for global hotkey — \
                     grant Accessibility permission to your terminal",
                );
                // Reclaim the Arc so we don't leak
                let _ = Arc::from_raw(flag_ptr as *const AtomicBool);
                return;
            }

            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);

            CFRunLoopRun(); // blocks forever
        }
    });
}

/// Activate the terminal that owns our process (bring it to front).
#[cfg(target_os = "macos")]
pub fn activate_terminal() {
    let ppid = unsafe { libc::getppid() };
    let script = format!(
        "tell application \"System Events\" to set frontmost of \
         first process whose unix id is {} to true",
        ppid
    );
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok();
}

/// Start a background thread that listens for the global hotkey Ctrl+Shift+K (Windows).
/// Sets `flag` to `true` when the hotkey is pressed.
#[cfg(target_os = "windows")]
pub fn start_hotkey_listener(flag: Arc<AtomicBool>) {
    use std::ffi::c_void;

    type HWND = *mut c_void;
    type BOOL = i32;
    type UINT = u32;
    type WPARAM = usize;
    type LPARAM = isize;
    type DWORD = u32;
    type LONG = i32;

    #[repr(C)]
    struct POINT {
        x: LONG,
        y: LONG,
    }

    #[repr(C)]
    struct MSG {
        hwnd: HWND,
        message: UINT,
        w_param: WPARAM,
        l_param: LPARAM,
        time: DWORD,
        pt: POINT,
    }

    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_NOREPEAT: u32 = 0x4000;
    const VK_K: u32 = 0x4B;
    const WM_HOTKEY: u32 = 0x0312;
    const HOTKEY_ID: i32 = 1;

    extern "system" {
        fn RegisterHotKey(hwnd: HWND, id: i32, fs_modifiers: UINT, vk: UINT) -> BOOL;
        fn GetMessageW(
            msg: *mut MSG,
            hwnd: HWND,
            msg_filter_min: UINT,
            msg_filter_max: UINT,
        ) -> BOOL;
    }

    std::thread::spawn(move || {
        unsafe {
            let ok = RegisterHotKey(
                std::ptr::null_mut(),
                HOTKEY_ID,
                MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
                VK_K,
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
            // GetMessageW blocks until a message arrives; returns 0 on WM_QUIT
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY && msg.w_param == HOTKEY_ID as usize {
                    flag.store(true, Ordering::Release);
                }
            }
        }
    });
}

/// Bring the console window that owns our process to the foreground.
#[cfg(target_os = "windows")]
pub fn activate_terminal() {
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
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn start_hotkey_listener(_flag: Arc<AtomicBool>) {
    // Global hotkeys not supported on this platform
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn activate_terminal() {
    // Not supported on this platform
}
