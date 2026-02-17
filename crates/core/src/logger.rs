use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{mpsc, Mutex, OnceLock};
use chrono::Local;

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

struct Logger {
    file: File,
    tui_tx: Option<mpsc::Sender<String>>,
    prefixes: HashMap<String, u8>, // prefix -> color index
}

// Color indices for TUI rendering (mapped in ui.rs)
pub const COLOR_GRAY: u8 = 1;
pub const COLOR_BLUE: u8 = 2;

/// Initialize the global logger. Clears the log file.
pub fn init(log_dir: &Path) {
    fs::create_dir_all(log_dir).ok();
    let log_path = log_dir.join("app.log");
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .expect("failed to open log file");

    LOGGER
        .set(Mutex::new(Logger { file, tui_tx: None, prefixes: HashMap::new() }))
        .ok();
}

/// Wire the TUI log channel.
pub fn set_tui_sender(tx: mpsc::Sender<String>) {
    if let Some(logger) = LOGGER.get() {
        let mut l = logger.lock().unwrap();
        l.tui_tx = Some(tx);
    }
}

/// Register a prefix with a color. All subsequent log calls through
/// `log_with_prefix` will use this prefix and color.
pub fn register_prefix(prefix: &str, color: u8) {
    if let Some(logger) = LOGGER.get() {
        let mut l = logger.lock().unwrap();
        l.prefixes.insert(prefix.to_string(), color);
    }
}

/// Internal: format for TUI channel uses \x1f as field separator:
/// level\x1fprefix\x1fcolor\x1ftimestamp\x1fmessage
fn write_log(level: &str, prefix: &str, color: u8, msg: &str) {
    let ts = Local::now().format("%H:%M:%S").to_string();

    // File always gets plain text
    let file_line = if prefix.is_empty() {
        format!("[{}] [{}] {}", ts, level, msg)
    } else {
        format!("[{}] [{}] [{}] {}", ts, level, prefix, msg)
    };

    // TUI gets structured data
    let tui_line = format!("{}\x1f{}\x1f{}\x1f{}\x1f{}", level, prefix, color, ts, msg);

    if let Some(logger) = LOGGER.get() {
        let mut l = logger.lock().unwrap();
        writeln!(l.file, "{}", file_line).ok();
        if let Some(tx) = &l.tui_tx {
            tx.send(tui_line).ok();
        }
    }
}

pub fn info(msg: &str) {
    write_log("INFO", "", 0, msg);
}

pub fn warn(msg: &str) {
    write_log("WARN", "", 0, msg);
}

pub fn error(msg: &str) {
    write_log("ERROR", "", 0, msg);
}

/// Log with a registered prefix. Looks up the color from registration.
pub fn info_p(prefix: &str, msg: &str) {
    let color = LOGGER.get()
        .and_then(|l| l.lock().ok())
        .and_then(|l| l.prefixes.get(prefix).copied())
        .unwrap_or(0);
    write_log("INFO", prefix, color, msg);
}

pub fn warn_p(prefix: &str, msg: &str) {
    let color = LOGGER.get()
        .and_then(|l| l.lock().ok())
        .and_then(|l| l.prefixes.get(prefix).copied())
        .unwrap_or(0);
    write_log("WARN", prefix, color, msg);
}

pub fn error_p(prefix: &str, msg: &str) {
    let color = LOGGER.get()
        .and_then(|l| l.lock().ok())
        .and_then(|l| l.prefixes.get(prefix).copied())
        .unwrap_or(0);
    write_log("ERROR", prefix, color, msg);
}
