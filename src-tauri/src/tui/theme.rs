use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub title: Style,
    pub selected: Style,
    pub normal: Style,
    pub highlight: Style,
    pub inactive: Style,
    pub success: Style,
    pub error: Style,
    pub border: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            title: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            selected: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            normal: Style::default().fg(Color::White),
            highlight: Style::default().fg(Color::Green),
            inactive: Style::default().fg(Color::DarkGray),
            success: Style::default().fg(Color::Green),
            error: Style::default().fg(Color::Red),
            border: Style::default().fg(Color::Gray),
        }
    }
}
