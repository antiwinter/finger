pub mod stub;

#[cfg(target_os = "macos")]
pub mod darwin;

use crate::types::*;

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
}

/// Create the platform appropriate for the current OS.
pub fn create_platform(force_stub: bool) -> Box<dyn Platform> {
    if force_stub {
        return Box::new(stub::StubPlatform);
    }
    #[cfg(target_os = "macos")]
    {
        return Box::new(darwin::DarwinPlatform::new());
    }
    #[cfg(not(target_os = "macos"))]
    {
        return Box::new(stub::StubPlatform);
    }
}
