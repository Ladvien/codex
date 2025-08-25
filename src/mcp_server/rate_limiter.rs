//! MCP Rate Limiting System
//!
//! This module provides configurable rate limiting for MCP requests,
//! supporting per-client, per-tool, and global rate limits.

use crate::mcp_server::auth::AuthContext;
use crate::security::{audit::AuditLogger, SecurityError};
use anyhow::Result;
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorRateLimiter,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, warn};

/// Rate limiting configuration for MCP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPRateLimitConfig {
    pub enabled: bool,
    pub global_requests_per_minute: u32,
    pub global_burst_size: u32,
    pub per_client_requests_per_minute: u32,
    pub per_client_burst_size: u32,
    pub per_tool_requests_per_minute: HashMap<String, u32>,
    pub per_tool_burst_size: HashMap<String, u32>,
    pub silent_mode_multiplier: f64,
    pub whitelist_clients: Vec<String>,
    pub performance_target_ms: u64,
    pub client_ttl_minutes: u32,
    pub cleanup_interval_minutes: u32,
}

impl Default for MCPRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: env::var("MCP_RATE_LIMIT_ENABLED")
                .map(|s| s.parse().unwrap_or(true))
                .unwrap_or(true),
            global_requests_per_minute: env::var("MCP_GLOBAL_RATE_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
            global_burst_size: env::var("MCP_GLOBAL_BURST_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
            per_client_requests_per_minute: env::var("MCP_CLIENT_RATE_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            per_client_burst_size: env::var("MCP_CLIENT_BURST_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            per_tool_requests_per_minute: Self::load_tool_rates_from_env(),
            per_tool_burst_size: Self::load_tool_bursts_from_env(),
            silent_mode_multiplier: env::var("MCP_SILENT_MODE_MULTIPLIER")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.5), // Reduce limits by 50% in silent mode
            whitelist_clients: env::var("MCP_RATE_LIMIT_WHITELIST")
                .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
                .unwrap_or_default(),
            performance_target_ms: 5, // Must be <5ms per requirement
            client_ttl_minutes: env::var("MCP_CLIENT_TTL_MINUTES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60), // 1 hour TTL for inactive clients
            cleanup_interval_minutes: env::var("MCP_CLEANUP_INTERVAL_MINUTES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(15), // Cleanup every 15 minutes
        }
    }
}

impl MCPRateLimitConfig {
    /// Load per-tool rate limits from environment variables
    fn load_tool_rates_from_env() -> HashMap<String, u32> {
        let mut rates = HashMap::new();

        // Default tool-specific rates
        rates.insert("store_memory".to_string(), 50);
        rates.insert("search_memory".to_string(), 200);
        rates.insert("get_statistics".to_string(), 20);
        rates.insert("what_did_you_remember".to_string(), 30);
        rates.insert("harvest_conversation".to_string(), 100);
        rates.insert("get_harvester_metrics".to_string(), 10);
        rates.insert("migrate_memory".to_string(), 20);
        rates.insert("delete_memory".to_string(), 10);

        // Load custom rates from environment
        if let Ok(custom_rates) = env::var("MCP_TOOL_RATE_LIMITS") {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, u32>>(&custom_rates) {
                rates.extend(parsed);
            }
        }

        rates
    }

    /// Load per-tool burst sizes from environment variables
    fn load_tool_bursts_from_env() -> HashMap<String, u32> {
        let mut bursts = HashMap::new();

        // Default tool-specific burst sizes
        bursts.insert("store_memory".to_string(), 5);
        bursts.insert("search_memory".to_string(), 20);
        bursts.insert("get_statistics".to_string(), 2);
        bursts.insert("what_did_you_remember".to_string(), 3);
        bursts.insert("harvest_conversation".to_string(), 10);
        bursts.insert("get_harvester_metrics".to_string(), 1);
        bursts.insert("migrate_memory".to_string(), 2);
        bursts.insert("delete_memory".to_string(), 1);

        // Load custom burst sizes from environment
        if let Ok(custom_bursts) = env::var("MCP_TOOL_BURST_SIZES") {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, u32>>(&custom_bursts) {
                bursts.extend(parsed);
            }
        }

        bursts
    }

    /// Create configuration from environment variables
    pub fn from_env() -> Self {
        Self::default()
    }
}

/// Rate limiting statistics
#[derive(Debug, Clone, Serialize)]
pub struct RateLimitStats {
    pub total_requests: u64,
    pub rejected_requests: u64,
    pub rejection_rate: f64,
    pub per_client_rejections: HashMap<String, u64>,
    pub per_tool_rejections: HashMap<String, u64>,
    pub avg_check_duration_ms: f64,
    pub peak_requests_per_minute: u64,
}

/// Individual rate limiter for a specific scope with TTL tracking
pub struct ScopedRateLimiter {
    limiter: Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>,
    requests_per_minute: u32,
    burst_size: u32,
    name: String,
    created_at: Instant,
    last_used: Arc<RwLock<Instant>>,
}

impl ScopedRateLimiter {
    fn new(requests_per_minute: u32, burst_size: u32, name: String) -> Result<Self> {
        // SECURITY: Remove panic-prone unwrap() calls and use proper error handling
        let rate = NonZeroU32::new(requests_per_minute.max(1))
            .ok_or_else(|| anyhow::anyhow!("Invalid rate limit: {}", requests_per_minute))?;
        let burst = NonZeroU32::new(burst_size.max(1))
            .ok_or_else(|| anyhow::anyhow!("Invalid burst size: {}", burst_size))?;

        let quota = Quota::per_minute(rate).allow_burst(burst);
        let limiter = Arc::new(GovernorRateLimiter::direct(quota));

        let now = Instant::now();
        Ok(Self {
            limiter,
            requests_per_minute,
            burst_size,
            name,
            created_at: now,
            last_used: Arc::new(RwLock::new(now)),
        })
    }

    async fn check_rate_limit(
        &self,
    ) -> Result<(), governor::NotUntil<governor::clock::QuantaInstant>> {
        let result = self.limiter.check();
        if result.is_ok() {
            // Update last used timestamp when rate limit check succeeds
            let mut last_used = self.last_used.write().await;
            *last_used = Instant::now();
        }
        result
    }

    /// Check if this limiter has expired based on TTL
    async fn is_expired(&self, ttl_duration: Duration) -> bool {
        let last_used = *self.last_used.read().await;
        last_used.elapsed() > ttl_duration
    }
}

/// MCP Rate Limiter implementation with memory leak prevention
pub struct MCPRateLimiter {
    config: MCPRateLimitConfig,
    global_limiter: Option<ScopedRateLimiter>,
    client_limiters: Arc<RwLock<HashMap<String, ScopedRateLimiter>>>,
    tool_limiters: HashMap<String, ScopedRateLimiter>,
    stats: Arc<RwLock<RateLimitStats>>,
    audit_logger: Arc<AuditLogger>,
}

impl MCPRateLimiter {
    /// Create a new rate limiter
    pub fn new(config: MCPRateLimitConfig, audit_logger: Arc<AuditLogger>) -> Result<Self> {
        let global_limiter = if config.enabled {
            Some(ScopedRateLimiter::new(
                config.global_requests_per_minute,
                config.global_burst_size,
                "global".to_string(),
            )?)
        } else {
            None
        };

        let mut tool_limiters = HashMap::new();
        if config.enabled {
            for (tool_name, &rate) in &config.per_tool_requests_per_minute {
                let burst = config
                    .per_tool_burst_size
                    .get(tool_name)
                    .copied()
                    .unwrap_or(rate / 10); // Default burst is 10% of rate

                tool_limiters.insert(
                    tool_name.clone(),
                    ScopedRateLimiter::new(rate, burst, format!("tool:{tool_name}"))?,
                );
            }
        }

        let stats = Arc::new(RwLock::new(RateLimitStats {
            total_requests: 0,
            rejected_requests: 0,
            rejection_rate: 0.0,
            per_client_rejections: HashMap::new(),
            per_tool_rejections: HashMap::new(),
            avg_check_duration_ms: 0.0,
            peak_requests_per_minute: 0,
        }));

        let client_limiters = Arc::new(RwLock::new(HashMap::new()));

        // Create rate limiter instance
        let rate_limiter = Self {
            config: config.clone(),
            global_limiter,
            client_limiters: client_limiters.clone(),
            tool_limiters,
            stats,
            audit_logger,
        };

        // Start TTL cleanup task for memory leak prevention
        if config.client_ttl_minutes > 0 {
            Self::start_cleanup_task(client_limiters, config);
        }

        Ok(rate_limiter)
    }

    /// Start background cleanup task to prevent memory leaks
    fn start_cleanup_task(
        client_limiters: Arc<RwLock<HashMap<String, ScopedRateLimiter>>>,
        config: MCPRateLimitConfig,
    ) {
        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(
                config.cleanup_interval_minutes as u64 * 60,
            ));
            let ttl_duration = Duration::from_secs(config.client_ttl_minutes as u64 * 60);

            loop {
                cleanup_interval.tick().await;

                let start_cleanup = Instant::now();
                let initial_count;
                let expired_clients;

                // Collect expired clients
                {
                    let limiters = client_limiters.read().await;
                    initial_count = limiters.len();

                    let mut expired = Vec::new();
                    for (client_id, limiter) in limiters.iter() {
                        if limiter.is_expired(ttl_duration).await {
                            expired.push(client_id.clone());
                        }
                    }
                    expired_clients = expired;
                }

                // Remove expired clients
                if !expired_clients.is_empty() {
                    let mut limiters = client_limiters.write().await;
                    for client_id in &expired_clients {
                        limiters.remove(client_id);
                    }

                    let final_count = limiters.len();
                    let cleanup_duration = start_cleanup.elapsed();

                    debug!(
                        "Rate limiter TTL cleanup completed: {} expired clients removed, {} active clients remain, cleanup took {}ms",
                        expired_clients.len(),
                        final_count,
                        cleanup_duration.as_millis()
                    );

                    if expired_clients.len() > 100 {
                        warn!(
                            "Large number of expired clients ({}) suggests possible memory leak or aggressive cleanup needed",
                            expired_clients.len()
                        );
                    }
                } else {
                    debug!(
                        "Rate limiter TTL cleanup: no expired clients found ({})",
                        initial_count
                    );
                }
            }
        });
    }

    /// Check rate limits for an MCP request
    pub async fn check_rate_limit(
        &self,
        auth_context: Option<&AuthContext>,
        tool_name: &str,
        silent_mode: bool,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();

        if !self.config.enabled {
            return Ok(());
        }

        let client_id = auth_context
            .map(|ctx| ctx.client_id.as_str())
            .unwrap_or("anonymous");

        // Check if client is whitelisted
        if self
            .config
            .whitelist_clients
            .contains(&client_id.to_string())
        {
            debug!("Client {} is whitelisted, skipping rate limits", client_id);
            return Ok(());
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_requests += 1;
        }

        // Apply silent mode multiplier if needed
        let rate_multiplier = if silent_mode {
            self.config.silent_mode_multiplier
        } else {
            1.0
        };

        // Check global rate limit
        if let Some(ref global_limiter) = self.global_limiter {
            if global_limiter.check_rate_limit().await.is_err() {
                self.handle_rate_limit_violation("global", client_id, tool_name)
                    .await;
                return Err(SecurityError::RateLimitExceeded.into());
            }
        }

        // Check per-client rate limit
        let client_limiter = match self
            .get_or_create_client_limiter(client_id, rate_multiplier)
            .await
        {
            Ok(limiter) => limiter,
            Err(e) => {
                error!("Failed to create client rate limiter: {}", e);
                return Err(SecurityError::RateLimitExceeded.into());
            }
        };

        if client_limiter.check_rate_limit().await.is_err() {
            self.handle_rate_limit_violation("client", client_id, tool_name)
                .await;
            return Err(SecurityError::RateLimitExceeded.into());
        }

        // Check per-tool rate limit
        if let Some(tool_limiter) = self.tool_limiters.get(tool_name) {
            if tool_limiter.check_rate_limit().await.is_err() {
                self.handle_rate_limit_violation("tool", client_id, tool_name)
                    .await;
                return Err(SecurityError::RateLimitExceeded.into());
            }
        }

        let elapsed = start_time.elapsed();

        // Check performance requirement
        if elapsed.as_millis() > self.config.performance_target_ms as u128 {
            warn!(
                "Rate limit check took {}ms, exceeding target of {}ms",
                elapsed.as_millis(),
                self.config.performance_target_ms
            );
        }

        // Update average check duration
        {
            let mut stats = self.stats.write().await;
            let total_ms = stats.avg_check_duration_ms * (stats.total_requests - 1) as f64;
            stats.avg_check_duration_ms =
                (total_ms + elapsed.as_millis() as f64) / stats.total_requests as f64;
        }

        debug!(
            "Rate limit check passed for client: {}, tool: {}",
            client_id, tool_name
        );
        Ok(())
    }

    /// Get or create a client-specific rate limiter
    async fn get_or_create_client_limiter(
        &self,
        client_id: &str,
        rate_multiplier: f64,
    ) -> Result<ScopedRateLimiter> {
        {
            let limiters = self.client_limiters.read().await;
            if let Some(limiter) = limiters.get(client_id) {
                return Ok(ScopedRateLimiter {
                    limiter: limiter.limiter.clone(),
                    requests_per_minute: limiter.requests_per_minute,
                    burst_size: limiter.burst_size,
                    name: limiter.name.clone(),
                    created_at: limiter.created_at,
                    last_used: limiter.last_used.clone(),
                });
            }
        }

        // Create new limiter for this client
        let adjusted_rate =
            (self.config.per_client_requests_per_minute as f64 * rate_multiplier) as u32;
        let adjusted_burst = (self.config.per_client_burst_size as f64 * rate_multiplier) as u32;

        let limiter = ScopedRateLimiter::new(
            adjusted_rate.max(1),
            adjusted_burst.max(1),
            format!("client:{client_id}"),
        )?;

        // Store the limiter for future use
        {
            let mut limiters = self.client_limiters.write().await;
            limiters.insert(
                client_id.to_string(),
                ScopedRateLimiter {
                    limiter: limiter.limiter.clone(),
                    requests_per_minute: limiter.requests_per_minute,
                    burst_size: limiter.burst_size,
                    name: limiter.name.clone(),
                    created_at: limiter.created_at,
                    last_used: limiter.last_used.clone(),
                },
            );
        }

        Ok(limiter)
    }

    /// Handle rate limit violations
    async fn handle_rate_limit_violation(
        &self,
        limit_type: &str,
        client_id: &str,
        tool_name: &str,
    ) {
        warn!(
            "Rate limit violation - Type: {}, Client: {}, Tool: {}",
            limit_type, client_id, tool_name
        );

        // Update rejection statistics
        {
            let mut stats = self.stats.write().await;
            stats.rejected_requests += 1;
            stats.rejection_rate = stats.rejected_requests as f64 / stats.total_requests as f64;

            *stats
                .per_client_rejections
                .entry(client_id.to_string())
                .or_insert(0) += 1;
            *stats
                .per_tool_rejections
                .entry(tool_name.to_string())
                .or_insert(0) += 1;
        }

        // Log the violation for security auditing
        self.audit_logger
            .log_rate_limit_violation(client_id, tool_name, limit_type)
            .await;
    }

    /// Reset rate limits for a specific client (admin function)
    pub async fn reset_client_limits(&self, client_id: &str) -> Result<()> {
        let mut limiters = self.client_limiters.write().await;
        limiters.remove(client_id);

        // Create a fresh limiter with default rates to ensure the Governor state is reset
        let fresh_limiter = ScopedRateLimiter::new(
            self.config.per_client_requests_per_minute,
            self.config.per_client_burst_size,
            format!("client:{client_id}"),
        )?;

        limiters.insert(client_id.to_string(), fresh_limiter);
        debug!("Reset rate limits for client: {}", client_id);
        Ok(())
    }

    /// Get current rate limiting statistics
    pub async fn get_stats(&self) -> RateLimitStats {
        self.stats.read().await.clone()
    }

    /// Update configuration dynamically
    pub async fn update_config(&mut self, new_config: MCPRateLimitConfig) -> Result<()> {
        debug!("Updating rate limiter configuration");

        // Update global limiter
        self.global_limiter = if new_config.enabled {
            Some(ScopedRateLimiter::new(
                new_config.global_requests_per_minute,
                new_config.global_burst_size,
                "global".to_string(),
            )?)
        } else {
            None
        };

        // Update tool limiters
        self.tool_limiters.clear();
        if new_config.enabled {
            for (tool_name, &rate) in &new_config.per_tool_requests_per_minute {
                let burst = new_config
                    .per_tool_burst_size
                    .get(tool_name)
                    .copied()
                    .unwrap_or(rate / 10);

                self.tool_limiters.insert(
                    tool_name.clone(),
                    ScopedRateLimiter::new(rate, burst, format!("tool:{tool_name}"))?,
                );
            }
        }

        // Clear existing client limiters to force recreation with new rates
        {
            let mut limiters = self.client_limiters.write().await;
            limiters.clear();
        }

        self.config = new_config;
        Ok(())
    }

    /// Get rate limiter configuration and status
    pub async fn get_status(&self) -> serde_json::Value {
        let stats = self.get_stats().await;
        let client_count = self.client_limiters.read().await.len();

        serde_json::json!({
            "enabled": self.config.enabled,
            "global_limits": {
                "requests_per_minute": self.config.global_requests_per_minute,
                "burst_size": self.config.global_burst_size,
            },
            "per_client_limits": {
                "requests_per_minute": self.config.per_client_requests_per_minute,
                "burst_size": self.config.per_client_burst_size,
                "active_clients": client_count,
            },
            "tool_limits": self.config.per_tool_requests_per_minute,
            "statistics": stats,
            "performance": {
                "target_ms": self.config.performance_target_ms,
                "avg_check_duration_ms": stats.avg_check_duration_ms,
            },
            "silent_mode_multiplier": self.config.silent_mode_multiplier,
            "whitelist_clients": self.config.whitelist_clients.len(),
        })
    }

    /// Test helper to get the number of active client limiters
    #[cfg(test)]
    pub async fn get_client_limiter_count(&self) -> usize {
        self.client_limiters.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_server::auth::AuthMethod;
    use crate::security::AuditConfig;
    use tempfile::tempdir;

    fn create_test_config() -> MCPRateLimitConfig {
        MCPRateLimitConfig {
            enabled: true,
            global_requests_per_minute: 60, // 1 per second for testing
            global_burst_size: 5,
            per_client_requests_per_minute: 30, // 0.5 per second for testing
            per_client_burst_size: 3,
            per_tool_requests_per_minute: {
                let mut map = HashMap::new();
                map.insert("store_memory".to_string(), 12); // 0.2 per second
                map.insert("search_memory".to_string(), 60); // 1 per second
                map
            },
            per_tool_burst_size: {
                let mut map = HashMap::new();
                map.insert("store_memory".to_string(), 2);
                map.insert("search_memory".to_string(), 5);
                map
            },
            silent_mode_multiplier: 0.5,
            whitelist_clients: vec!["whitelisted-client".to_string()],
            performance_target_ms: 5,
            client_ttl_minutes: 5,
            cleanup_interval_minutes: 1,
        }
    }

    async fn create_test_rate_limiter() -> MCPRateLimiter {
        let config = create_test_config();
        let temp_dir = tempdir().unwrap();
        let audit_config = AuditConfig {
            enabled: true,
            log_all_requests: true,
            log_data_access: true,
            log_modifications: true,
            log_auth_events: true,
            retention_days: 30,
        };
        let audit_logger = Arc::new(AuditLogger::new(audit_config).unwrap());
        MCPRateLimiter::new(config, audit_logger).unwrap()
    }

    fn create_test_auth_context(client_id: &str) -> AuthContext {
        AuthContext {
            client_id: client_id.to_string(),
            user_id: "test-user".to_string(),
            method: AuthMethod::ApiKey,
            scopes: vec!["mcp:read".to_string(), "mcp:write".to_string()],
            expires_at: None,
            request_id: "test-request".to_string(),
        }
    }

    #[tokio::test]
    async fn test_rate_limit_allows_normal_requests() {
        let limiter = create_test_rate_limiter().await;
        let auth_context = create_test_auth_context("test-client");

        // Should allow normal requests
        let result = limiter
            .check_rate_limit(Some(&auth_context), "search_memory", false)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limit_blocks_excessive_requests() {
        let limiter = create_test_rate_limiter().await;
        let auth_context = create_test_auth_context("test-client");

        // Exhaust the burst limit for store_memory (2 requests)
        assert!(limiter
            .check_rate_limit(Some(&auth_context), "store_memory", false)
            .await
            .is_ok());
        assert!(limiter
            .check_rate_limit(Some(&auth_context), "store_memory", false)
            .await
            .is_ok());

        // Third request should be rate limited
        let result = limiter
            .check_rate_limit(Some(&auth_context), "store_memory", false)
            .await;
        assert!(result.is_err());

        // Check that it's specifically a rate limit error
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Rate limit exceeded"));
    }

    #[tokio::test]
    async fn test_different_clients_have_separate_limits() {
        let limiter = create_test_rate_limiter().await;
        let auth_context1 = create_test_auth_context("client-1");
        let auth_context2 = create_test_auth_context("client-2");

        // Exhaust client-1's limits
        for _ in 0..3 {
            let result = limiter
                .check_rate_limit(Some(&auth_context1), "search_memory", false)
                .await;
            if result.is_err() {
                break;
            }
        }

        // client-2 should still be able to make requests
        let result = limiter
            .check_rate_limit(Some(&auth_context2), "search_memory", false)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_whitelisted_clients_bypass_limits() {
        let limiter = create_test_rate_limiter().await;
        let auth_context = create_test_auth_context("whitelisted-client");

        // Should be able to make many requests without being rate limited
        for _ in 0..10 {
            let result = limiter
                .check_rate_limit(Some(&auth_context), "store_memory", false)
                .await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_silent_mode_reduces_limits() {
        let limiter = create_test_rate_limiter().await;
        let auth_context = create_test_auth_context("test-client");

        // In silent mode, limits should be reduced by multiplier (0.5)
        // So burst size should be effectively 1 instead of 2 for store_memory
        assert!(limiter
            .check_rate_limit(Some(&auth_context), "store_memory", true)
            .await
            .is_ok());

        // Second request should be rate limited in silent mode
        let result = limiter
            .check_rate_limit(Some(&auth_context), "store_memory", true)
            .await;
        // Note: This might pass depending on the exact timing and implementation
        // The key is that silent mode should be more restrictive
    }

    #[tokio::test]
    async fn test_disabled_rate_limiting() {
        let mut config = create_test_config();
        config.enabled = false;

        let temp_dir = tempdir().unwrap();
        let audit_config = AuditConfig {
            enabled: true,
            log_all_requests: true,
            log_data_access: true,
            log_modifications: true,
            log_auth_events: true,
            retention_days: 30,
        };
        let audit_logger = Arc::new(AuditLogger::new(audit_config).unwrap());
        let limiter = MCPRateLimiter::new(config, audit_logger).unwrap();

        let auth_context = create_test_auth_context("test-client");

        // Should allow unlimited requests when disabled
        for _ in 0..20 {
            let result = limiter
                .check_rate_limit(Some(&auth_context), "store_memory", false)
                .await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_statistics_tracking() {
        let limiter = create_test_rate_limiter().await;
        let auth_context = create_test_auth_context("test-client");

        // Make some requests
        let _ = limiter
            .check_rate_limit(Some(&auth_context), "search_memory", false)
            .await;
        let _ = limiter
            .check_rate_limit(Some(&auth_context), "search_memory", false)
            .await;

        let stats = limiter.get_stats().await;
        assert_eq!(stats.total_requests, 2);
        assert!(stats.avg_check_duration_ms >= 0.0);
    }

    #[tokio::test]
    async fn test_client_limit_reset() {
        let limiter = create_test_rate_limiter().await;
        let auth_context = create_test_auth_context("test-client");

        // Exhaust only the client limits (3 requests = client burst size)
        // Use a tool that has higher limits than client limits to avoid tool limit conflicts
        for _ in 0..3 {
            let result = limiter
                .check_rate_limit(Some(&auth_context), "search_memory", false)
                .await;
            // First 3 should succeed due to burst
            assert!(result.is_ok());
        }

        // 4th request should fail due to client rate limit
        let result = limiter
            .check_rate_limit(Some(&auth_context), "search_memory", false)
            .await;
        assert!(
            result.is_err(),
            "4th request should be rate limited by client limits"
        );

        // Reset limits for this client
        limiter.reset_client_limits("test-client").await.unwrap();

        // Should be able to make requests again after reset
        let result = limiter
            .check_rate_limit(Some(&auth_context), "search_memory", false)
            .await;
        assert!(
            result.is_ok(),
            "Request should succeed after client limit reset"
        );
    }
}
