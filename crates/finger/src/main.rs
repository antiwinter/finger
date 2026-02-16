use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use finger_core::{logger, orchestrator};
use finger_core::platform::create_platform;
use finger_core::types::Command;

fn main() -> Result<()> {
    let force_stub = std::env::args().any(|a| a == "--stub");

    // Resolve bots directory (next to the binary, or cwd/bots)
    let bots_dir = {
        let mut d = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        d.push("bots");
        d
    };
    let logs_dir = {
        let mut d = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        d.push("logs");
        d
    };

    // Init logger
    logger::init(&logs_dir);

    // Create platform
    let platform = create_platform(force_stub);

    // Load bots and scan instances
    let mut entries = orchestrator::load_bots(&bots_dir);
    orchestrator::scan_instances(&mut entries, platform.as_ref());

    logger::info(&format!("loaded {} bot(s), scanning windows", entries.len()));

    // Shared state
    let state = Arc::new(Mutex::new(entries));

    // Channels
    let (log_tx, log_rx) = mpsc::channel::<String>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();

    // Wire logger to TUI
    logger::set_tui_sender(log_tx);
    logger::info("finger started");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create TUI app
    let mut app = finger_tui::App::new(
        Arc::clone(&state),
        log_rx,
        cmd_tx,
    );

    // Spawn orchestrator on a background thread
    let orch_state = Arc::clone(&state);
    let orch_platform = create_platform(force_stub);
    let orch_bots_dir = bots_dir.clone();
    thread::spawn(move || {
        orchestrator::orchestrate(orch_state, orch_platform, orch_bots_dir, cmd_rx);
    });

    // Run TUI event loop on main thread
    let result = finger_tui::event::run(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
