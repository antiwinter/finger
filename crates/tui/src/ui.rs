use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use finger_core::types::OrchestratorState;
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

    // Orchestrator state banner (rendered separately as full-width bar)
    let (banner_label, banner_bg) = {
        let orch = app.orch_state.lock().unwrap();
        match *orch {
            OrchestratorState::Running => ("RUNNING (Press S to stop)", Color::Green),
            OrchestratorState::Stopping => ("STOPPING...", Color::Yellow),
            OrchestratorState::Stopped => ("STOPPED (Press S to start)", Color::Red),
        }
    };

    let mut lines: Vec<Line> = Vec::new();

    // Blank row after banner
    // lines.push(Line::from(""));

    // Help line as first content line inside the bordered panel
    lines.push(Line::from(vec![
        Span::styled(" j", Style::default().fg(Color::Yellow)),
        Span::raw("/"),
        Span::styled("k", Style::default().fg(Color::Yellow)),
        Span::raw("/"),
        Span::styled("space", Style::default().fg(Color::Yellow)),
        Span::raw(" to select, "),
        Span::styled("r", Style::default().fg(Color::Yellow)),
        Span::raw(" to reset:"),
    ]));
    lines.push(Line::from(""));

    {
        let entries = app.state.lock().unwrap();

        for (i, entry) in entries.iter().enumerate() {
            let is_selected = i == app.selected;
            let prefix = if is_selected { "> " } else { "  " };

            let checkbox = if entry.enabled { "[●]" } else { "[ ]" };
            let check_color = banner_bg;

            // Bot header line: checkbox + name + description
            let name = entry.name.clone();
            let mut spans = vec![
                Span::raw(prefix),
                Span::styled(checkbox, Style::default().fg(check_color)),
                Span::raw(" "),
            ];
            spans.push(Span::styled(
                name,
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ));
            if !entry.description.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", entry.description),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            lines.push(Line::from(spans));

            // Instance lines (only for enabled bots)
            if entry.enabled {
                for inst in &entry.instances {
                    let status_color = if inst.error.is_some() {
                        Color::Red
                    } else {
                        Color::Cyan
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
            }
        }
    } // entries lock dropped here

    // Split left panel into banner (1 line) + bot list (fills space)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(chunks[0]);

    // Full-width centered banner
    let banner_width = left_chunks[0].width as usize;
    let pad_total = banner_width.saturating_sub(banner_label.len());
    let pad_left = pad_total / 2;
    let pad_right = pad_total - pad_left;
    let centered_banner = format!("{}{}{}", " ".repeat(pad_left), banner_label, " ".repeat(pad_right));
    let banner = Paragraph::new(Line::from(Span::styled(
        centered_banner,
        Style::default().fg(Color::Black).bg(banner_bg).add_modifier(Modifier::BOLD),
    )));
    f.render_widget(banner, left_chunks[0]);

    let bot_list = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(bot_list, left_chunks[1]);

    // -- Right panel: logs --
    if app.log_visible && chunks.len() > 1 {
        let visible_height = chunks[1].height.saturating_sub(2) as usize;
        let total = app.log_messages.len();
        let max_scroll = total.saturating_sub(visible_height);
        let scroll = app.log_scroll.min(max_scroll);
        let start = total.saturating_sub(visible_height + scroll);
        let end = total.saturating_sub(scroll);
        let log_lines: Vec<Line> = app.log_messages[start..end]
            .iter()
            .map(|m| parse_log_line(m))
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

/// Parse a structured log line (level\x1fprefix\x1fcolor\x1ftimestamp\x1fmessage)
/// into a colored Line for TUI rendering.
fn parse_log_line(raw: &str) -> Line<'_> {
    let parts: Vec<&str> = raw.splitn(5, '\x1f').collect();
    if parts.len() < 5 {
        // Fallback for unstructured messages
        return Line::from(raw);
    }

    let level = parts[0];
    let prefix = parts[1];
    let color_idx: u8 = parts[2].parse().unwrap_or(0);
    let timestamp = parts[3];
    let message = parts[4];

    let prefix_color = match color_idx {
        1 => Color::DarkGray,  // COLOR_GRAY
        2 => Color::LightBlue, // COLOR_BLUE
        _ => Color::White,
    };

    let msg_color = prefix_color;

    let mut spans = Vec::new();

    // Dim timestamp (no brackets)
    spans.push(Span::styled(
        timestamp,
        Style::default().fg(Color::DarkGray),
    ));
    spans.push(Span::raw(" "));

    // Level tag: only show for warn/error, colored (overrides line color)
    match level {
        "ERROR" => {
            spans.push(Span::styled("error ", Style::default().fg(Color::Red)));
        }
        "WARN" => {
            spans.push(Span::styled("warn ", Style::default().fg(Color::Yellow)));
        }
        _ => {} // INFO: no tag
    }

    // Prefix (bold to distinguish from message)
    if !prefix.is_empty() {
        spans.push(Span::styled(prefix, Style::default().fg(prefix_color).add_modifier(Modifier::BOLD)));
        spans.push(Span::styled(" ", Style::default().fg(msg_color)));
    }

    // Message in same color as prefix (default line color)
    spans.push(Span::styled(message, Style::default().fg(msg_color)));

    Line::from(spans)
}
