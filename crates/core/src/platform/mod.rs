pub mod stub;

#[cfg(target_os = "macos")]
pub mod darwin;

#[cfg(target_os = "windows")]
pub mod win32;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::types::*;
use crate::logger;

/// Handle to a specific OS window, providing automation ops.
pub trait WindowHandle: Send {
    fn id(&self) -> WindowId;
    fn title(&self) -> &str;
    fn region(&self) -> Option<Region>;
    fn update(&mut self);
    fn activate(&mut self);
    fn click_relative(&mut self, x_ratio: f64, y_ratio: f64);
    fn tap(&mut self, key: &str);
    fn type_text(&mut self, text: &str);
    fn capture(&mut self, rect: Option<CaptureRect>) -> Option<Capture>;
}

/// Platform-level operations (window enumeration, factory).
pub trait Platform: Send {
    fn get_instances(&self, pattern: &str) -> Vec<(WindowId, String)>;
    fn create_window(&self, pattern: &str, window_id: WindowId) -> Box<dyn WindowHandle>;
    /// Start a background thread listening for the global hotkey; sets `flag` when triggered.
    fn start_hotkey_listener(&self, flag: Arc<AtomicBool>);
    /// Bring the terminal / launcher window that owns this process to the foreground.
    fn activate_terminal(&self);
}

/// Create the platform appropriate for the current OS.
pub fn create_platform(force_stub: bool) -> Box<dyn Platform> {
    logger::register_prefix("hint", logger::COLOR_GRAY);
    if force_stub {
        logger::register_prefix("stub", logger::COLOR_GRAY);
        return Box::new(stub::StubPlatform);
    }
    #[cfg(target_os = "macos")]
    {
        logger::register_prefix("darwin", logger::COLOR_GRAY);
        return Box::new(darwin::DarwinPlatform::new());
    }
    #[cfg(target_os = "windows")]
    {
        logger::register_prefix("win32", logger::COLOR_GRAY);
        return Box::new(win32::Win32Platform::new());
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        logger::register_prefix("stub", logger::COLOR_GRAY);
        return Box::new(stub::StubPlatform);
    }
}
