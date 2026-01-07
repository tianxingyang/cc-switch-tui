use std::sync::Arc;

use crossterm::event::KeyCode;
use indexmap::IndexMap;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Row, Table, TableState};

use super::{Theme, View};
use cc_switch_lib::{AppState, McpServer, McpService};

pub struct McpView {
    state: Arc<AppState>,
    servers: IndexMap<String, McpServer>,
    table_state: TableState,
}

impl McpView {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            servers: IndexMap::new(),
            table_state: TableState::default(),
        }
    }

    pub async fn refresh(&mut self) {
        self.servers = McpService::get_all_servers(&self.state).unwrap_or_default();
        if !self.servers.is_empty() && self.table_state.selected().is_none() {
            self.table_state.select(Some(0));
        }
    }

    pub async fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Up => self.select_prev(),
            KeyCode::Down => self.select_next(),
            _ => {}
        }
    }

    fn select_prev(&mut self) {
        if self.servers.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    fn select_next(&mut self) {
        if self.servers.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => (i + 1).min(self.servers.len() - 1),
            None => 0,
        };
        self.table_state.select(Some(i));
    }
}

impl View for McpView {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let header = Row::new(vec!["Name", "Claude", "Codex", "Gemini"]).style(theme.title);

        let rows: Vec<Row> = self
            .servers
            .iter()
            .map(|(id, server)| {
                let claude = if server.apps.claude { "[x]" } else { "[ ]" };
                let codex = if server.apps.codex { "[x]" } else { "[ ]" };
                let gemini = if server.apps.gemini { "[x]" } else { "[ ]" };
                Row::new(vec![id.as_str(), claude, codex, gemini])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ],
        )
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("MCP Servers"))
        .highlight_style(theme.selected);

        frame.render_stateful_widget(table, area, &mut self.table_state);
    }
}
