use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};

use super::terminal::{self, Tui};
use super::theme::Theme;
use super::views::{McpView, ProviderForm, ProvidersView, ProxyView, SettingsView, View};
use cc_switch_lib::{AppState, AppType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Providers,
    Mcp,
    Proxy,
    Settings,
}

impl ActiveView {
    fn index(&self) -> usize {
        match self {
            Self::Providers => 0,
            Self::Mcp => 1,
            Self::Proxy => 2,
            Self::Settings => 3,
        }
    }

    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Providers,
            1 => Self::Mcp,
            2 => Self::Proxy,
            3 => Self::Settings,
            _ => Self::Providers,
        }
    }
}

pub struct App {
    pub state: Arc<AppState>,
    pub theme: Theme,
    pub active_view: ActiveView,
    pub active_app: AppType,
    pub should_quit: bool,

    pub providers_view: ProvidersView,
    pub mcp_view: McpView,
    pub proxy_view: ProxyView,
    pub settings_view: SettingsView,
    pub provider_form: ProviderForm,
}

impl App {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state: state.clone(),
            theme: Theme::default(),
            active_view: ActiveView::Providers,
            active_app: AppType::Claude,
            should_quit: false,
            providers_view: ProvidersView::new(state.clone()),
            mcp_view: McpView::new(state.clone()),
            proxy_view: ProxyView::new(state.clone()),
            settings_view: SettingsView::new(state.clone()),
            provider_form: ProviderForm::new(state.clone()),
        }
    }

    pub async fn refresh_data(&mut self) {
        match self.active_view {
            ActiveView::Providers => self.providers_view.refresh(self.active_app.clone()).await,
            ActiveView::Mcp => self.mcp_view.refresh().await,
            ActiveView::Proxy => self.proxy_view.refresh().await,
            ActiveView::Settings => {}
        }
    }

    fn switch_app(&mut self, app: AppType) {
        self.active_app = app;
    }

    fn next_app(&mut self) {
        self.active_app = match self.active_app {
            AppType::Claude => AppType::Codex,
            AppType::Codex => AppType::Gemini,
            AppType::Gemini => AppType::Claude,
        };
    }

    fn prev_app(&mut self) {
        self.active_app = match self.active_app {
            AppType::Claude => AppType::Gemini,
            AppType::Codex => AppType::Claude,
            AppType::Gemini => AppType::Codex,
        };
    }

    fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Length(1), // Tabs
                Constraint::Min(0),    // Content
                Constraint::Length(1), // Status bar
            ])
            .split(frame.area());

        self.render_header(frame, chunks[0]);
        self.render_tabs(frame, chunks[1]);
        self.render_content(frame, chunks[2]);
        self.render_status_bar(frame, chunks[3]);

        // 渲染表单（如果可见）
        self.provider_form.render(frame, &self.theme);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let app_names = ["Claude", "Codex", "Gemini"];
        let app_index = match self.active_app {
            AppType::Claude => 0,
            AppType::Codex => 1,
            AppType::Gemini => 2,
        };

        let header_text = format!(
            " CC Switch TUI v4.0.0    App: {}",
            app_names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    if i == app_index {
                        format!("[{}]", name)
                    } else {
                        format!(" {} ", name)
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        );

        let header = Paragraph::new(header_text)
            .style(self.theme.title)
            .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(header, area);
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles = vec!["[1]Providers", "[2]MCP", "[3]Proxy", "[4]Settings"];
        let tabs = Tabs::new(titles)
            .select(self.active_view.index())
            .style(self.theme.normal)
            .highlight_style(self.theme.selected);
        frame.render_widget(tabs, area);
    }

    fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        match self.active_view {
            ActiveView::Providers => self.providers_view.render(frame, area, &self.theme),
            ActiveView::Mcp => self.mcp_view.render(frame, area, &self.theme),
            ActiveView::Proxy => self.proxy_view.render(frame, area, &self.theme),
            ActiveView::Settings => self.settings_view.render(frame, area, &self.theme),
        }
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let hints = match self.active_view {
            ActiveView::Providers => {
                "↑↓:Select  Enter:Switch  a:Add  e:Edit  d:Delete  ←→:App  q:Quit"
            }
            ActiveView::Mcp => "↑↓:Select  Space:Toggle  a:Add  e:Edit  d:Delete  q:Quit",
            ActiveView::Proxy => "p:Start/Stop  t:Takeover  q:Quit",
            ActiveView::Settings => "Enter:Select  q:Quit",
        };
        let status = Paragraph::new(hints).style(self.theme.inactive);
        frame.render_widget(status, area);
    }

    async fn handle_key(&mut self, key: KeyCode) {
        // 如果表单可见，优先处理表单事件
        if self.provider_form.visible {
            let should_refresh = self.provider_form.handle_key(key, self.active_app.clone());
            if should_refresh {
                self.refresh_data().await;
            }
            return;
        }

        // Global keys
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('1') => {
                self.active_view = ActiveView::Providers;
                self.refresh_data().await;
            }
            KeyCode::Char('2') => {
                self.active_view = ActiveView::Mcp;
                self.refresh_data().await;
            }
            KeyCode::Char('3') => {
                self.active_view = ActiveView::Proxy;
                self.refresh_data().await;
            }
            KeyCode::Char('4') => self.active_view = ActiveView::Settings,
            KeyCode::Left => {
                self.prev_app();
                self.refresh_data().await;
            }
            KeyCode::Right => {
                self.next_app();
                self.refresh_data().await;
            }
            _ => {
                // Delegate to active view
                self.handle_view_key(key).await;
            }
        }
    }

    async fn handle_view_key(&mut self, key: KeyCode) {
        match self.active_view {
            ActiveView::Providers => match key {
                KeyCode::Char('a') => {
                    self.provider_form.open_add(self.active_app.clone());
                }
                KeyCode::Char('e') => {
                    if let Some(provider) = self.providers_view.get_selected() {
                        self.provider_form
                            .open_edit(&provider, self.active_app.clone());
                    }
                }
                KeyCode::Char('d') => {
                    self.delete_selected_provider().await;
                }
                _ => {
                    self.providers_view
                        .handle_key(key, self.active_app.clone())
                        .await;
                }
            },
            ActiveView::Mcp => self.mcp_view.handle_key(key).await,
            ActiveView::Proxy => self.proxy_view.handle_key(key).await,
            ActiveView::Settings => self.settings_view.handle_key(key).await,
        }
    }

    async fn delete_selected_provider(&mut self) {
        use cc_switch_lib::ProviderService;

        if let Some(provider) = self.providers_view.get_selected() {
            if ProviderService::delete(&self.state, self.active_app.clone(), &provider.id).is_ok() {
                self.refresh_data().await;
            }
        }
    }
}

pub async fn run(state: Arc<AppState>) -> Result<()> {
    let mut terminal = terminal::init()?;
    let mut app = App::new(state);

    // Initial data load
    app.refresh_data().await;

    loop {
        terminal.draw(|frame| app.render(frame))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code).await;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    terminal::restore(&mut terminal)?;
    Ok(())
}
