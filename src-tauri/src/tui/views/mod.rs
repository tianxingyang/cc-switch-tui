mod mcp;
mod provider_form;
mod providers;
mod proxy;
mod settings;

pub use mcp::McpView;
pub use provider_form::{FormMode, ProviderForm};
pub use providers::ProvidersView;
pub use proxy::ProxyView;
pub use settings::SettingsView;

use ratatui::prelude::*;

use super::theme::Theme;

pub trait View {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme);
}
