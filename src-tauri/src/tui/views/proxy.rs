use std::sync::Arc;

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{Theme, View};
use cc_switch_lib::{AppState, ProxyService};

pub struct ProxyView {
    state: Arc<AppState>,
    is_running: bool,
}

impl ProxyView {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            is_running: false,
        }
    }

    pub async fn refresh(&mut self) {
        self.is_running = self.state.proxy_service.is_running().await;
    }

    pub async fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('p') => self.toggle_proxy().await,
            _ => {}
        }
    }

    async fn toggle_proxy(&mut self) {
        if self.is_running {
            let _ = self.state.proxy_service.stop().await;
        } else {
            let _ = self.state.proxy_service.start().await;
        }
        self.refresh().await;
    }
}

impl View for ProxyView {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let status = if self.is_running {
            "Running"
        } else {
            "Stopped"
        };
        let style = if self.is_running {
            theme.success
        } else {
            theme.inactive
        };

        let text = format!(
            "Proxy Status: {}\n\n\
             Press 'p' to start/stop proxy",
            status
        );

        let paragraph = Paragraph::new(text)
            .style(style)
            .block(Block::default().borders(Borders::ALL).title("Proxy"));

        frame.render_widget(paragraph, area);
    }
}
