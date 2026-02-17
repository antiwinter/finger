use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub struct ConfirmDialog {
    pub message: String,
    pub selected: bool, // true = Yes, false = No
}

impl ConfirmDialog {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            selected: false, // default to No
        }
    }

    pub fn toggle(&mut self) {
        self.selected = !self.selected;
    }

    pub fn render(&self, f: &mut Frame) {
        let area = centered_rect(40, 7, f.area());

        // Clear the area behind the dialog
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Confirm ");

        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // top padding
                Constraint::Length(1), // message
                Constraint::Length(1), // spacing
                Constraint::Length(1), // buttons
            ])
            .split(inner);

        // Message
        let msg = Paragraph::new(Line::from(Span::styled(
            &self.message,
            Style::default().fg(Color::White),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(msg, chunks[1]);

        // Buttons
        let yes_style = if self.selected {
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let no_style = if !self.selected {
            Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let buttons = Line::from(vec![
            Span::styled("  [Yes]  ", yes_style),
            Span::raw("   "),
            Span::styled("  [No]  ", no_style),
        ]);
        let buttons_para = Paragraph::new(buttons)
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(buttons_para, chunks[3]);
    }
}

/// Return a centered `Rect` of `width` columns and `height` rows inside `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
