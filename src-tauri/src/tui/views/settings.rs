use std::sync::Arc;

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{Theme, View};
use cc_switch_lib::AppState;

pub struct SettingsView {
    state: Arc<AppState>,
}

impl SettingsView {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn handle_key(&mut self, _key: KeyCode) {
        // TODO: Implement settings actions
    }
}

impl View for SettingsView {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let text = "Settings\n\n\
            [E] Export configuration\n\
            [I] Import configuration\n\n\
            (More settings coming soon)";

        let paragraph = Paragraph::new(text)
            .style(theme.normal)
            .block(Block::default().borders(Borders::ALL).title("Settings"));

        frame.render_widget(paragraph, area);
    }
}
