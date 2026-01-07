use std::collections::HashSet;
use std::sync::Arc;

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::Theme;
use crate::tui::widgets::TextInput;
use cc_switch_lib::{AppState, AppType, Provider, ProviderMeta, ProviderService};

const BASE_URL_LABEL: &str = "Base URLs (comma-separated)";

#[derive(Clone, Copy, PartialEq)]
pub enum FormMode {
    Add,
    Edit,
}

#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Name,
    ApiKey,
    BaseUrl,
}

impl FormField {
    fn next(&self) -> Self {
        match self {
            Self::Name => Self::ApiKey,
            Self::ApiKey => Self::BaseUrl,
            Self::BaseUrl => Self::Name,
        }
    }

    fn prev(&self) -> Self {
        match self {
            Self::Name => Self::BaseUrl,
            Self::ApiKey => Self::Name,
            Self::BaseUrl => Self::ApiKey,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::ApiKey => "API Key",
            Self::BaseUrl => BASE_URL_LABEL,
        }
    }
}

pub struct ProviderForm {
    state: Arc<AppState>,
    pub mode: FormMode,
    pub visible: bool,
    edit_id: Option<String>,
    active_field: FormField,
    name: TextInput,
    api_key: TextInput,
    base_url: TextInput,
    original_meta: Option<ProviderMeta>,
    pub message: Option<String>,
    // 编辑弹窗状态
    popup_editing: bool,
    popup_input: TextInput,
}

impl ProviderForm {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            mode: FormMode::Add,
            visible: false,
            edit_id: None,
            active_field: FormField::Name,
            name: TextInput::new("Name"),
            api_key: TextInput::new("API Key"),
            base_url: TextInput::new(BASE_URL_LABEL),
            original_meta: None,
            message: None,
            popup_editing: false,
            popup_input: TextInput::new(""),
        }
    }

    pub fn open_add(&mut self, app_type: AppType) {
        self.mode = FormMode::Add;
        self.visible = true;
        self.edit_id = None;
        self.active_field = FormField::Name;
        self.name.clear();
        self.api_key.clear();
        self.base_url.clear();
        self.original_meta = None;
        self.message = None;

        // 设置默认 Base URL
        let default_url = match app_type {
            AppType::Claude => "https://api.anthropic.com",
            AppType::Codex => "https://api.openai.com/v1",
            AppType::Gemini => "https://generativelanguage.googleapis.com",
        };
        self.base_url = TextInput::with_value(BASE_URL_LABEL, default_url);
    }

    pub fn open_edit(&mut self, provider: &Provider, app_type: AppType) {
        self.mode = FormMode::Edit;
        self.visible = true;
        self.edit_id = Some(provider.id.clone());
        self.active_field = FormField::Name;
        self.message = None;
        self.original_meta = provider.meta.clone();

        self.name = TextInput::with_value("Name", &provider.name);

        // 调用后端 Service 提取 API Key 和 Base URL
        let (api_key, base_url) = ProviderService::extract_credentials_lenient(provider, &app_type);
        self.api_key = TextInput::with_value("API Key", &api_key);

        // 将已有 base_url 与自定义端点合并展示，便于编辑多个地址
        let mut all_urls = Vec::new();
        let normalized_base = normalize_url(&base_url);
        if !normalized_base.is_empty() {
            all_urls.push(normalized_base);
        }
        if let Some(meta) = &self.original_meta {
            let mut endpoints: Vec<_> = meta.custom_endpoints.values().collect();
            endpoints.sort_by_key(|ep| ep.added_at);
            for ep in endpoints {
                let url = normalize_url(&ep.url);
                if !url.is_empty() && !all_urls.contains(&url) {
                    all_urls.push(url);
                }
            }
        }
        let base_url_text = all_urls.join(", ");
        self.base_url = TextInput::with_value(BASE_URL_LABEL, &base_url_text);
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.message = None;
    }

    fn active_input(&mut self) -> &mut TextInput {
        match self.active_field {
            FormField::Name => &mut self.name,
            FormField::ApiKey => &mut self.api_key,
            FormField::BaseUrl => &mut self.base_url,
        }
    }

    fn parse_base_urls(&self) -> Vec<String> {
        let mut urls = Vec::new();
        for part in self
            .base_url
            .value
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        {
            let url = normalize_url(part);
            if !url.is_empty() && !urls.contains(&url) {
                urls.push(url);
            }
        }
        urls
    }

    fn sync_custom_endpoints(
        &self,
        app_type: AppType,
        provider_id: &str,
        urls: &[String],
    ) -> Result<(), String> {
        let desired: HashSet<String> = urls
            .iter()
            .map(|u| normalize_url(u))
            .filter(|u| !u.is_empty())
            .collect();

        let existing_endpoints =
            ProviderService::get_custom_endpoints(&self.state, app_type.clone(), provider_id)
                .map_err(|e| e.to_string())?;
        let existing: HashSet<String> = existing_endpoints
            .into_iter()
            .map(|ep| normalize_url(&ep.url))
            .filter(|u| !u.is_empty())
            .collect();

        for url in existing.difference(&desired) {
            ProviderService::remove_custom_endpoint(
                &self.state,
                app_type.clone(),
                provider_id,
                url.to_string(),
            )
            .map_err(|e| e.to_string())?;
        }
        for url in desired.difference(&existing) {
            ProviderService::add_custom_endpoint(
                &self.state,
                app_type.clone(),
                provider_id,
                url.to_string(),
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// 返回 true 表示需要关闭表单并刷新列表
    pub fn handle_key(&mut self, key: KeyCode, app_type: AppType) -> bool {
        // 弹窗编辑模式
        if self.popup_editing {
            return self.handle_popup_key(key);
        }

        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.close();
                false
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
                self.active_field = self.active_field.next();
                false
            }
            KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => {
                self.active_field = self.active_field.prev();
                false
            }
            KeyCode::Enter => self.submit(app_type),
            KeyCode::Char('e') | KeyCode::F(2) => {
                self.open_popup();
                false
            }
            _ => false,
        }
    }

    fn open_popup(&mut self) {
        let current_value = self.active_input().value.clone();
        self.popup_input = TextInput::with_value(self.active_field.label(), &current_value);
        self.popup_editing = true;
    }

    fn handle_popup_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Esc => {
                self.popup_editing = false;
                false
            }
            KeyCode::Enter => {
                // 保存弹窗内容到对应字段
                let value = self.popup_input.value.clone();
                self.active_input().value = value;
                self.active_input().end();
                self.popup_editing = false;
                false
            }
            KeyCode::Char(c) => {
                self.popup_input.insert(c);
                false
            }
            KeyCode::Backspace => {
                self.popup_input.backspace();
                false
            }
            KeyCode::Delete => {
                self.popup_input.delete();
                false
            }
            KeyCode::Left => {
                self.popup_input.move_left();
                false
            }
            KeyCode::Right => {
                self.popup_input.move_right();
                false
            }
            KeyCode::Home => {
                self.popup_input.home();
                false
            }
            KeyCode::End => {
                self.popup_input.end();
                false
            }
            _ => false,
        }
    }

    fn submit(&mut self, app_type: AppType) -> bool {
        if self.name.value.trim().is_empty() {
            self.message = Some("Name is required".to_string());
            return false;
        }
        if self.api_key.value.trim().is_empty() {
            self.message = Some("API Key is required".to_string());
            return false;
        }

        let base_urls = self.parse_base_urls();
        if base_urls.is_empty() {
            self.message = Some("Base URL is required".to_string());
            return false;
        }

        let result = match self.mode {
            FormMode::Add => self.do_add(app_type, &base_urls),
            FormMode::Edit => self.do_edit(app_type, &base_urls),
        };

        match result {
            Ok(_) => {
                self.close();
                true
            }
            Err(e) => {
                self.message = Some(e);
                false
            }
        }
    }

    fn do_add(&self, app_type: AppType, base_urls: &[String]) -> Result<(), String> {
        let primary_base_url = base_urls.first().map(|s| s.as_str()).unwrap_or_default();
        let config = self.build_config(app_type.clone(), primary_base_url);
        let provider_id = uuid::Uuid::new_v4().to_string();
        let provider = Provider {
            id: provider_id.clone(),
            name: self.name.value.trim().to_string(),
            settings_config: config,
            website_url: None,
            category: None,
            created_at: Some(chrono::Utc::now().timestamp()),
            sort_index: None,
            notes: None,
            meta: self.original_meta.clone(),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        };

        ProviderService::add(&self.state, app_type.clone(), provider).map_err(|e| e.to_string())?;
        self.sync_custom_endpoints(app_type, &provider_id, base_urls)?;
        Ok(())
    }

    fn do_edit(&self, app_type: AppType, base_urls: &[String]) -> Result<(), String> {
        let id = self.edit_id.as_ref().ok_or("No provider ID")?;
        let primary_base_url = base_urls.first().map(|s| s.as_str()).unwrap_or_default();
        let config = self.build_config(app_type.clone(), primary_base_url);
        let provider = Provider {
            id: id.clone(),
            name: self.name.value.trim().to_string(),
            settings_config: config,
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: self.original_meta.clone(),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        };

        ProviderService::update(&self.state, app_type.clone(), provider)
            .map_err(|e| e.to_string())?;
        self.sync_custom_endpoints(app_type, id, base_urls)?;
        Ok(())
    }

    fn build_config(&self, app_type: AppType, primary_base_url: &str) -> serde_json::Value {
        match app_type {
            AppType::Claude => serde_json::json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": self.api_key.value.trim(),
                    "ANTHROPIC_BASE_URL": primary_base_url
                }
            }),
            AppType::Codex => {
                let provider_name = self.name.value.trim();
                // Sanitize provider name for TOML key (lowercase, replace special chars)
                let provider_key: String = provider_name
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let provider_key = if provider_key.is_empty() {
                    "custom".to_string()
                } else {
                    provider_key
                };

                serde_json::json!({
                    "auth": {
                        "OPENAI_API_KEY": self.api_key.value.trim()
                    },
                    "config": format!(
                        r#"model_provider = "{provider_key}"

[model_providers.{provider_key}]
name = "{provider_name}"
base_url = "{base_url}"
wire_api = "responses"
requires_openai_auth = true
"#,
                        provider_key = provider_key,
                        provider_name = provider_name,
                        base_url = primary_base_url
                    )
                })
            }
            AppType::Gemini => serde_json::json!({
                "env": {
                    "GEMINI_API_KEY": self.api_key.value.trim(),
                    "GOOGLE_GEMINI_BASE_URL": primary_base_url
                }
            }),
        }
    }

    pub fn render(&self, frame: &mut Frame, theme: &Theme) {
        if !self.visible {
            return;
        }

        let area = centered_rect(60, 14, frame.area());
        frame.render_widget(Clear, area);

        let title = match self.mode {
            FormMode::Add => "Add Provider",
            FormMode::Edit => "Edit Provider",
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(theme.border);
        frame.render_widget(block, area);

        let inner = area.inner(Margin::new(2, 1));
        self.render_fields(frame, inner, theme);

        // 渲染编辑弹窗
        if self.popup_editing {
            self.render_popup(frame, theme);
        }
    }

    fn render_fields(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(2),
            ])
            .split(area);

        self.render_input(frame, chunks[0], &self.name, FormField::Name, theme);
        self.render_input(frame, chunks[1], &self.api_key, FormField::ApiKey, theme);
        self.render_input(frame, chunks[2], &self.base_url, FormField::BaseUrl, theme);

        // Message
        if let Some(msg) = &self.message {
            let p = Paragraph::new(msg.as_str()).style(theme.error);
            frame.render_widget(p, chunks[3]);
        }

        // Hints
        let hints =
            Paragraph::new("j/k:Navigate  e:Edit  Enter:Save  q/Esc:Cancel").style(theme.inactive);
        frame.render_widget(hints, chunks[4]);
    }

    fn render_input(
        &self,
        frame: &mut Frame,
        area: Rect,
        input: &TextInput,
        field: FormField,
        theme: &Theme,
    ) {
        let is_active = self.active_field == field;
        let style = if is_active {
            theme.selected
        } else {
            theme.normal
        };

        // API Key: 激活时显示完整内容以便编辑，非激活时脱敏
        let display_value = if field == FormField::ApiKey && !input.value.is_empty() && !is_active {
            mask_api_key(&input.value)
        } else {
            input.value.clone()
        };

        let text = format!("{}: {}", input.label, display_value);
        let p = Paragraph::new(text).style(style);
        frame.render_widget(p, area);
    }

    fn render_popup(&self, frame: &mut Frame, theme: &Theme) {
        let area = centered_rect(70, 7, frame.area());
        frame.render_widget(Clear, area);

        let title = format!("Edit {}", self.active_field.label());
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(theme.highlight);
        frame.render_widget(block, area);

        let inner = area.inner(Margin::new(2, 1));
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Length(2)])
            .split(inner);

        // 输入内容（带光标）
        let value = &self.popup_input.value;
        let cursor = self.popup_input.cursor;
        let display = format!("{}│{}", &value[..cursor], &value[cursor..]);
        let p = Paragraph::new(display).style(theme.selected);
        frame.render_widget(p, chunks[0]);

        // 提示
        let hints = Paragraph::new("Enter:Confirm  Esc:Cancel").style(theme.inactive);
        frame.render_widget(hints, chunks[1]);
    }
}

fn normalize_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        "*".repeat(key.len())
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_width = r.width * percent_x / 100;
    let x = (r.width.saturating_sub(popup_width)) / 2;
    let y = (r.height.saturating_sub(height)) / 2;

    Rect::new(r.x + x, r.y + y, popup_width, height)
}
