use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};

use finger_core::platform::hotkey;
use finger_core::types::OrchestratorState;

use crate::App;
use crate::ui;

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    hotkey_flag: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    loop {
        if app.should_quit {
            return Ok(());
        }

        // Check global hotkey (Alt+Shift+K)
        if hotkey_flag.swap(false, Ordering::Acquire) {
            let is_running = *app.orch_state.lock().unwrap() == OrchestratorState::Running;
            if is_running {
                app.start_stop(); // enter Stopping state
            }
            hotkey::activate_terminal();
        }

        // Drain log messages
        app.drain_logs();

        // Render
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for events with 100ms timeout (keeps TUI responsive)
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    // If confirm dialog is open, route input there
                    if app.confirm.is_some() {
                        match key.code {
                            KeyCode::Left | KeyCode::Right
                            | KeyCode::Char('h') | KeyCode::Char('l')
                            | KeyCode::Tab => {
                                app.confirm.as_mut().unwrap().toggle();
                            }
                            KeyCode::Enter => {
                                if app.confirm.as_ref().unwrap().selected {
                                    app.confirm_restart();
                                } else {
                                    app.cancel_confirm();
                                }
                            }
                            KeyCode::Esc | KeyCode::Char('q') => {
                                app.cancel_confirm();
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            app.quit();
                        }
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                            app.move_up();
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                            app.move_down();
                        }
                        KeyCode::Char(' ') => {
                            app.toggle_selected();
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            app.start_stop();
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            app.restart_selected();
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            app.toggle_log();
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            app.scroll_log_up(3);
                        }
                        MouseEventKind::ScrollDown => {
                            app.scroll_log_down(3);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}
