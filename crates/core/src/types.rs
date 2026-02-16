/// Window identifier (CGWindowID on macOS, HWND on Windows)
pub type WindowId = u64;

/// Screen-coordinate bounding box of a window
#[derive(Debug, Clone, Copy, Default)]
pub struct Region {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
    pub w: i32,
    pub h: i32,
    pub cx: i32,
    pub cy: i32,
}

/// Sub-region for partial capture (relative to window origin)
#[derive(Debug, Clone, Copy)]
pub struct CaptureRect {
    pub l: i32,
    pub t: i32,
    pub w: i32,
    pub h: i32,
}

/// Raw screenshot pixel data (BGRA)
#[derive(Debug)]
pub struct Capture {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
}

/// One discovered bot script and its runtime state
pub struct BotEntry {
    pub name: String,
    pub window_pattern: String,
    pub description: String,
    pub enabled: bool,
    pub instances: Vec<Instance>,
    pub error: Option<String>,
    pub script_path: std::path::PathBuf,
}

/// One bot instance bound to a specific window
pub struct Instance {
    pub id: String,
    pub window_id: WindowId,
    pub window_title: String,
    pub status: String,
    pub error: Option<String>,
}

impl Instance {
    pub fn new(bot_name: &str, window_id: WindowId, window_title: String) -> Self {
        Self {
            id: format!("{}-{}", bot_name, window_id),
            window_id,
            window_title,
            status: String::new(),
            error: None,
        }
    }
}

/// Command from TUI to orchestrator
pub enum Command {
    Toggle(usize),
    Quit,
}
