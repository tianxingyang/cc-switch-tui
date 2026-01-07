//! URL 路由模块
//!
//! 提供 URL 级别的选择和熔断功能，支持混合模式（最低延迟 + Failover）

use super::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use super::error::ProxyError;
use super::types::{HybridModeConfig, ProviderEndpoint};
use crate::database::Database;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// URL 路由器
///
/// 负责在单个 Provider 内的多个 URL 之间进行选择和熔断
pub struct UrlRouter {
    db: Arc<Database>,
    /// URL 级别熔断器: key = "provider_id:url_hash"
    circuit_breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
    /// 默认熔断器配置
    default_config: CircuitBreakerConfig,
}

impl UrlRouter {
    /// 创建新的 URL 路由器
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            default_config: CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 2,
                timeout_seconds: 30,
                error_rate_threshold: 0.5,
                min_requests: 5,
            },
        }
    }

    /// 选择最佳 URL
    ///
    /// 选择逻辑：
    /// 1. 获取所有 URL（config base_url + custom endpoints）
    /// 2. 过滤掉 Circuit Breaker 处于 Open 状态的 URL
    /// 3. 按延迟升序排序
    /// 4. 返回延迟最低的健康 URL
    /// 5. 若所有 URL 都不可用，返回 config base_url（降级）
    pub async fn select_url(
        &self,
        provider_id: &str,
        app_type: &str,
        config_base_url: &str,
    ) -> Result<String, ProxyError> {
        // 获取所有端点
        let endpoints = self.get_all_urls(provider_id, app_type, config_base_url)?;

        if endpoints.is_empty() {
            return Ok(config_base_url.to_string());
        }

        // 过滤可用的 URL
        let mut available_urls = Vec::new();
        for endpoint in &endpoints {
            let breaker = self
                .get_or_create_circuit_breaker(provider_id, &endpoint.url)
                .await;
            if breaker.is_available().await {
                available_urls.push(endpoint.clone());
            }
        }

        // 如果没有可用的 URL，降级到 config base_url
        if available_urls.is_empty() {
            log::warn!(
                "[UrlRouter] 所有 URL 都不可用，降级到 config base_url: {}",
                config_base_url
            );
            return Ok(config_base_url.to_string());
        }

        // 按延迟排序（主端点优先，然后按延迟升序）
        available_urls.sort_by(|a, b| {
            // 主端点优先
            if a.is_primary && !b.is_primary {
                return std::cmp::Ordering::Less;
            }
            if !a.is_primary && b.is_primary {
                return std::cmp::Ordering::Greater;
            }
            // 按延迟排序
            match (a.latency_ms, b.latency_ms) {
                (Some(a_lat), Some(b_lat)) => a_lat.cmp(&b_lat),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        let selected = &available_urls[0];
        log::info!(
            "[UrlRouter] 选择 URL: {} (延迟: {:?}ms, 主端点: {})",
            selected.url,
            selected.latency_ms,
            selected.is_primary
        );

        Ok(selected.url.clone())
    }

    /// 获取所有 URL（config base_url + custom endpoints）
    fn get_all_urls(
        &self,
        provider_id: &str,
        app_type: &str,
        config_base_url: &str,
    ) -> Result<Vec<ProviderEndpoint>, ProxyError> {
        // 从数据库获取自定义端点
        let mut endpoints = self
            .db
            .get_provider_endpoints_with_health(app_type, provider_id)
            .map_err(|e| ProxyError::Internal(e.to_string()))?;

        // 检查 config base_url 是否已在列表中
        let config_url_normalized = config_base_url.trim_end_matches('/');
        let config_exists = endpoints
            .iter()
            .any(|e| e.url.trim_end_matches('/') == config_url_normalized);

        // 如果 config base_url 不在列表中，添加为虚拟端点
        if !config_exists {
            endpoints.insert(
                0,
                ProviderEndpoint {
                    id: 0,
                    provider_id: provider_id.to_string(),
                    app_type: app_type.to_string(),
                    url: config_base_url.to_string(),
                    latency_ms: None,
                    last_tested_at: None,
                    is_healthy: true,
                    consecutive_failures: 0,
                    is_primary: endpoints.is_empty(), // 如果没有其他端点，设为主端点
                },
            );
        }

        Ok(endpoints)
    }

    /// 记录 URL 请求结果
    pub async fn record_url_result(
        &self,
        provider_id: &str,
        app_type: &str,
        url: &str,
        success: bool,
        latency_ms: Option<u64>,
    ) {
        // 更新熔断器状态
        let breaker = self.get_or_create_circuit_breaker(provider_id, url).await;

        // URL 级别的熔断器不使用 HalfOpen permit 机制
        if success {
            breaker.record_success(false).await;
        } else {
            breaker.record_failure(false).await;
        }

        // 更新数据库中的健康状态
        let breaker_state = breaker.get_state().await;
        let consecutive_failures = breaker.get_stats().await.consecutive_failures;
        let is_healthy = breaker_state != super::circuit_breaker::CircuitState::Open;

        if let Err(e) = self.db.update_endpoint_health(
            app_type,
            provider_id,
            url,
            latency_ms,
            is_healthy,
            consecutive_failures,
        ) {
            log::warn!("[UrlRouter] 更新端点健康状态失败: {}", e);
        }
    }

    /// 获取或创建 URL 级别的熔断器
    async fn get_or_create_circuit_breaker(
        &self,
        provider_id: &str,
        url: &str,
    ) -> Arc<CircuitBreaker> {
        let key = format!("{}:{}", provider_id, Self::hash_url(url));

        // 先尝试读取
        {
            let breakers = self.circuit_breakers.read().await;
            if let Some(breaker) = breakers.get(&key) {
                return breaker.clone();
            }
        }

        // 不存在则创建
        let mut breakers = self.circuit_breakers.write().await;
        // 双重检查
        if let Some(breaker) = breakers.get(&key) {
            return breaker.clone();
        }

        let breaker = Arc::new(CircuitBreaker::new(self.default_config.clone()));
        breakers.insert(key, breaker.clone());
        breaker
    }

    /// 计算 URL 的哈希值（用于熔断器 key）
    fn hash_url(url: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        hasher.finish()
    }

    /// 获取混合模式配置
    pub fn get_hybrid_config(&self, app_type: &str) -> HybridModeConfig {
        // 从数据库读取配置
        match self.db.get_hybrid_mode_config(app_type) {
            Ok(config) => config,
            Err(e) => {
                log::warn!("[UrlRouter] 读取混合模式配置失败: {}, 使用默认值", e);
                HybridModeConfig {
                    enabled: true,
                    latency_test_interval: 300,
                    url_circuit_failure_threshold: 3,
                }
            }
        }
    }

    /// 检查是否启用混合模式
    pub fn is_hybrid_mode_enabled(&self, app_type: &str) -> bool {
        self.get_hybrid_config(app_type).enabled
    }
}
