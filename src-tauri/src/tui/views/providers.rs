use std::sync::Arc;

use crossterm::event::KeyCode;
use indexmap::IndexMap;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use super::{Theme, View};
use cc_switch_lib::{AppState, AppType, Provider, ProviderService};

pub struct ProvidersView {
    state: Arc<AppState>,
    providers: IndexMap<String, Provider>,
    current_id: Option<String>,
    list_state: ListState,
}

impl ProvidersView {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            providers: IndexMap::new(),
            current_id: None,
            list_state: ListState::default(),
        }
    }

    pub async fn refresh(&mut self, app_type: AppType) {
        self.providers = ProviderService::list(&self.state, app_type.clone()).unwrap_or_default();
        self.current_id = ProviderService::current(&self.state, app_type).ok();

        if !self.providers.is_empty() && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
    }

    pub async fn handle_key(&mut self, key: KeyCode, app_type: AppType) {
        match key {
            KeyCode::Up => self.select_prev(),
            KeyCode::Down => self.select_next(),
            KeyCode::Enter => self.switch_provider(app_type).await,
            _ => {}
        }
    }

    fn select_prev(&mut self) {
        if self.providers.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select_next(&mut self) {
        if self.providers.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1).min(self.providers.len() - 1),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    async fn switch_provider(&mut self, app_type: AppType) {
        if let Some(i) = self.list_state.selected() {
            if let Some((id, _)) = self.providers.get_index(i) {
                if ProviderService::switch(&self.state, app_type.clone(), id).is_ok() {
                    self.current_id = Some(id.clone());
                }
            }
        }
    }

    pub fn get_selected(&self) -> Option<Provider> {
        let i = self.list_state.selected()?;
        let (_, provider) = self.providers.get_index(i)?;
        Some(provider.clone())
    }
}

impl View for ProvidersView {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<ListItem> = self
            .providers
            .iter()
            .map(|(id, provider)| {
                let is_current = self.current_id.as_ref() == Some(id);
                let marker = if is_current { "[*]" } else { "   " };
                let text = format!("{} {}", marker, provider.name);
                let style = if is_current {
                    theme.highlight
                } else {
                    theme.normal
                };
                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Providers"))
            .highlight_style(theme.selected)
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }
}
