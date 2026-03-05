use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};

use finger_core::platform::Platform;
use finger_core::types::OrchestratorState;

use crate::App;
use crate::ui;

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    hotkey_flag: Arc<AtomicBool>,
    platform: &dyn Platform,
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
            platform.activate_terminal();
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
                            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                                app.confirm.as_mut().unwrap().toggle();
                            }
                            KeyCode::Char('y') | KeyCode::Char('Y')
                            | KeyCode::Char('r') | KeyCode::Char('R') => {
                                app.confirm_restart();
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') => {
                                app.cancel_confirm();
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
                        KeyCode::Up => {
                            app.move_up();
                        }
                        KeyCode::Down => {
                            app.move_down();
                        }
                        KeyCode::Char('k') | KeyCode::Char('K') => {
                            app.start_stop();
                        }
                        KeyCode::Char(' ') => {
                            if app.is_stopped() {
                                app.toggle_selected();
                            }
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            app.restart_all();
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            app.toggle_log();
                        }
                        KeyCode::Char('m') | KeyCode::Char('M') => {
                            app.toggle_mouse_capture();
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
