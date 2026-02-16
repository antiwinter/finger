use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::App;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = if app.log_visible {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(f.area())
    } else {
        Layout::default()
            .constraints([Constraint::Percentage(100)])
            .split(f.area())
    };

    // -- Left panel: bot list --
    let mut lines: Vec<Line> = Vec::new();

    {
        let entries = app.state.lock().unwrap();

        for (i, entry) in entries.iter().enumerate() {
            let is_selected = i == app.selected;
            let prefix = if is_selected { "> " } else { "  " };

            let (indicator, color) = if entry.enabled {
                if entry.instances.iter().any(|ins| ins.error.is_some()) {
                    ("●", Color::Red)
                } else {
                    ("●", Color::Green)
                }
            } else {
                ("○", Color::DarkGray)
            };

            // Bot header line
            let name = entry.name.clone();
            let mut spans = vec![
                Span::raw(prefix),
                Span::styled(indicator, Style::default().fg(color)),
                Span::raw(" "),
            ];
            if is_selected {
                spans.push(Span::styled(
                    name,
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(name, Style::default().fg(color)));
            }
            spans.push(Span::styled(
                format!("  {} ins", entry.instances.len()),
                Style::default().fg(Color::DarkGray),
            ));
            lines.push(Line::from(spans));

            // Description
            lines.push(Line::from(Span::styled(
                format!("    {}", entry.description),
                Style::default().fg(Color::DarkGray),
            )));

            // Instance lines
            for inst in &entry.instances {
                let status_color = if inst.error.is_some() {
                    Color::Red
                } else if entry.enabled {
                    Color::Cyan
                } else {
                    Color::DarkGray
                };

                let status_text = if let Some(ref e) = inst.error {
                    format!(" err: {}", e)
                } else if !inst.status.is_empty() {
                    format!(" {}", inst.status)
                } else {
                    String::new()
                };

                lines.push(Line::from(vec![
                    Span::styled(
                        format!("    {} ", inst.window_title),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        format!("#{}", inst.window_id),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(status_text, Style::default().fg(status_color)),
                ]));
            }

            lines.push(Line::from(""));
        }
    } // entries lock dropped here

    // Help bar at bottom
    lines.push(Line::from(vec![
        Span::styled(" j/k", Style::default().fg(Color::Yellow)),
        Span::raw(" nav  "),
        Span::styled("space", Style::default().fg(Color::Yellow)),
        Span::raw(" toggle  "),
        Span::styled("L", Style::default().fg(Color::Yellow)),
        Span::raw(" log  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" quit"),
    ]));

    let bot_list = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Extra Fingers ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(bot_list, chunks[0]);

    // -- Right panel: logs --
    if app.log_visible && chunks.len() > 1 {
        let visible_height = chunks[1].height.saturating_sub(2) as usize;
        let skip = app.log_messages.len().saturating_sub(visible_height);
        let log_lines: Vec<Line> = app.log_messages[skip..]
            .iter()
            .map(|m| Line::from(m.as_str()))
            .collect();

        let log_panel = Paragraph::new(log_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Logs ")
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(log_panel, chunks[1]);
    }
}
