use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
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
            if let Event::Key(key) = event::read()? {
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
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        app.toggle_log();
                    }
                    _ => {}
                }
            }
        }
    }
}
