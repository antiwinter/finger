use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::App;
use crate::ui;

pub fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> anyhow::Result<()> {
    loop {
        if app.should_quit {
            return Ok(());
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
