use std::sync::Arc;

use anyhow::Result;
use cc_switch_lib::{AppState, AppType, Database, McpService, PromptService, ProviderService};

mod tui;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting CC Switch TUI v4.0.0");

    let db = match Database::init() {
        Ok(db) => Arc::new(db),
        Err(e) => {
            log::error!("Failed to initialize database: {e}");
            return Err(e.into());
        }
    };

    let app_state = Arc::new(AppState::new(db));

    // 首次运行时自动导入配置
    import_on_first_run(&app_state);

    tui::run(app_state).await
}

/// 首次运行时从 Live 配置导入数据
fn import_on_first_run(app_state: &AppState) {
    // 1. 导入供应商配置
    for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
        match ProviderService::import_default_config(app_state, app.clone()) {
            Ok(true) => {
                log::info!("✓ Imported default provider for {}", app.as_str());
            }
            Ok(false) => {} // 已有供应商，跳过
            Err(e) => {
                log::debug!("○ No default provider for {}: {}", app.as_str(), e);
            }
        }
    }

    // 2. 导入 MCP 服务器配置
    if app_state.db.is_mcp_table_empty().unwrap_or(false) {
        log::info!("MCP table empty, importing from live configurations...");
        import_mcp_servers(app_state);
    }

    // 3. 导入提示词
    if app_state.db.is_prompts_table_empty().unwrap_or(false) {
        log::info!("Prompts table empty, importing from live configurations...");
        import_prompts(app_state);
    }
}

fn import_mcp_servers(app_state: &AppState) {
    match McpService::import_from_claude(app_state) {
        Ok(count) if count > 0 => log::info!("✓ Imported {count} MCP server(s) from Claude"),
        Ok(_) => {}
        Err(e) => log::warn!("✗ Failed to import Claude MCP: {e}"),
    }

    match McpService::import_from_codex(app_state) {
        Ok(count) if count > 0 => log::info!("✓ Imported {count} MCP server(s) from Codex"),
        Ok(_) => {}
        Err(e) => log::warn!("✗ Failed to import Codex MCP: {e}"),
    }

    match McpService::import_from_gemini(app_state) {
        Ok(count) if count > 0 => log::info!("✓ Imported {count} MCP server(s) from Gemini"),
        Ok(_) => {}
        Err(e) => log::warn!("✗ Failed to import Gemini MCP: {e}"),
    }
}

fn import_prompts(app_state: &AppState) {
    for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
        match PromptService::import_from_file_on_first_launch(app_state, app.clone()) {
            Ok(count) if count > 0 => {
                log::info!("✓ Imported {count} prompt(s) for {}", app.as_str());
            }
            Ok(_) => {}
            Err(e) => log::warn!("✗ Failed to import prompt for {}: {e}", app.as_str()),
        }
    }
}
