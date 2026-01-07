//! URL 延迟测试服务
//!
//! 后台定期测试 URL 延迟，更新端点健康状态

use crate::database::Database;
use crate::error::AppError;
use crate::proxy::url_router::UrlRouter;
use crate::services::speedtest::SpeedtestService;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

/// URL 延迟测试服务
pub struct UrlLatencyService {
    db: Arc<Database>,
    url_router: Arc<UrlRouter>,
    /// 是否正在运行
    running: Arc<RwLock<bool>>,
}

impl UrlLatencyService {
    /// 创建新的延迟测试服务
    pub fn new(db: Arc<Database>, url_router: Arc<UrlRouter>) -> Self {
        Self {
            db,
            url_router,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动后台测试任务
    pub async fn start(&self, interval_seconds: u64) {
        // 检查是否已在运行
        {
            let mut running = self.running.write().await;
            if *running {
                log::warn!("[UrlLatencyService] 服务已在运行");
                return;
            }
            *running = true;
        }

        let db = self.db.clone();
        let url_router = self.url_router.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_seconds));

            loop {
                ticker.tick().await;

                // 检查是否应该停止
                if !*running.read().await {
                    log::info!("[UrlLatencyService] 服务已停止");
                    break;
                }

                // 测试所有应用类型的端点
                for app_type in &["claude", "codex", "gemini"] {
                    if let Err(e) = Self::test_app_endpoints(&db, &url_router, app_type).await {
                        log::warn!("[UrlLatencyService] 测试 {} 端点失败: {}", app_type, e);
                    }
                }
            }
        });

        log::info!(
            "[UrlLatencyService] 后台测试任务已启动，间隔 {} 秒",
            interval_seconds
        );
    }

    /// 停止服务
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        log::info!("[UrlLatencyService] 正在停止服务...");
    }

    /// 测试指定应用类型的所有端点
    async fn test_app_endpoints(
        db: &Database,
        url_router: &UrlRouter,
        app_type: &str,
    ) -> Result<(), AppError> {
        // 获取所有启用代理的 provider
        let providers = db.get_failover_providers(app_type)?;

        for provider in providers {
            if let Err(e) =
                Self::test_provider_endpoints(db, url_router, app_type, &provider.id).await
            {
                log::warn!(
                    "[UrlLatencyService] 测试 provider {} 端点失败: {}",
                    provider.id,
                    e
                );
            }
        }

        Ok(())
    }

    /// 测试指定 Provider 的所有端点
    async fn test_provider_endpoints(
        db: &Database,
        url_router: &UrlRouter,
        app_type: &str,
        provider_id: &str,
    ) -> Result<(), AppError> {
        // 获取所有端点
        let endpoints = db.get_provider_endpoints_with_health(app_type, provider_id)?;

        if endpoints.is_empty() {
            return Ok(());
        }

        // 收集 URL 列表
        let urls: Vec<String> = endpoints.iter().map(|e| e.url.clone()).collect();

        // 执行测速
        let results = SpeedtestService::test_endpoints(urls, Some(8)).await?;

        // 更新端点健康状态
        for result in results {
            let latency_ms = result.latency.map(|l| l as u64);
            let is_healthy = result.error.is_none() && result.latency.is_some();

            // 查找对应的端点
            if let Some(endpoint) = endpoints.iter().find(|e| e.url == result.url) {
                let consecutive_failures = if is_healthy {
                    0
                } else {
                    endpoint.consecutive_failures + 1
                };

                // 更新数据库
                db.update_endpoint_health(
                    app_type,
                    provider_id,
                    &result.url,
                    latency_ms,
                    is_healthy,
                    consecutive_failures,
                )?;

                // 同步更新 UrlRouter 的熔断器状态
                url_router
                    .record_url_result(provider_id, app_type, &result.url, is_healthy, latency_ms)
                    .await;
            }
        }

        // 更新主端点（选择延迟最低的健康端点）
        Self::update_primary_endpoint(db, app_type, provider_id)?;

        Ok(())
    }

    /// 更新主端点
    fn update_primary_endpoint(
        db: &Database,
        app_type: &str,
        provider_id: &str,
    ) -> Result<(), AppError> {
        // 获取最佳端点
        if let Some(best_url) = db.get_best_endpoint_url(app_type, provider_id)? {
            db.set_primary_endpoint(app_type, provider_id, &best_url)?;
            log::info!(
                "[UrlLatencyService] 更新 {} provider {} 主端点: {}",
                app_type,
                provider_id,
                best_url
            );
        }

        Ok(())
    }

    /// 手动触发测试（用于 TUI 界面）
    pub async fn test_now(&self, app_type: &str) -> Result<(), AppError> {
        Self::test_app_endpoints(&self.db, &self.url_router, app_type).await
    }
}
