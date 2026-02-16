use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{mpsc, Mutex, OnceLock};
use chrono::Local;

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

struct Logger {
    file: File,
    tui_tx: Option<mpsc::Sender<String>>,
}

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
        .set(Mutex::new(Logger { file, tui_tx: None }))
        .ok();
}

/// Wire the TUI log channel.
pub fn set_tui_sender(tx: mpsc::Sender<String>) {
    if let Some(logger) = LOGGER.get() {
        let mut l = logger.lock().unwrap();
        l.tui_tx = Some(tx);
    }
}

fn write_log(level: &str, msg: &str) {
    let ts = Local::now().format("%H:%M:%S");
    let line = format!("[{}] [{}] {}", ts, level, msg);

    if let Some(logger) = LOGGER.get() {
        let mut l = logger.lock().unwrap();
        writeln!(l.file, "{}", line).ok();
        if let Some(tx) = &l.tui_tx {
            tx.send(line).ok();
        }
    }
}

pub fn info(msg: &str) {
    write_log("INFO", msg);
}

pub fn warn(msg: &str) {
    write_log("WARN", msg);
}

pub fn error(msg: &str) {
    write_log("ERROR", msg);
}
