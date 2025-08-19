use crate::security::{RateLimitConfig, Result, SecurityError};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorRateLimiter,
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Rate limiting manager
pub struct RateLimitManager {
    config: RateLimitConfig,
    ip_limiters: Arc<
        RwLock<
            HashMap<
                IpAddr,
                Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>,
            >,
        >,
    >,
    user_limiters: Arc<
        RwLock<
            HashMap<
                String,
                Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>,
            >,
        >,
    >,
    global_limiter:
        Option<Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>>,
}

impl RateLimitManager {
    pub fn new(config: RateLimitConfig) -> Self {
        let global_limiter = if config.enabled {
            let quota = Quota::per_minute(
                NonZeroU32::new(config.requests_per_minute)
                    .unwrap_or(NonZeroU32::new(100).unwrap()),
            );
            Some(Arc::new(GovernorRateLimiter::direct(quota)))
        } else {
            None
        };

        Self {
            config,
            ip_limiters: Arc::new(RwLock::new(HashMap::new())),
            user_limiters: Arc::new(RwLock::new(HashMap::new())),
            global_limiter,
        }
    }

    /// Check rate limit for IP address
    pub async fn check_ip_limit(&self, ip: IpAddr) -> Result<()> {
        if !self.config.enabled || !self.config.per_ip {
            return Ok(());
        }

        // Check whitelist
        let ip_str = ip.to_string();
        if self.config.whitelist_ips.contains(&ip_str) {
            debug!("IP {} is whitelisted, bypassing rate limit", ip);
            return Ok(());
        }

        let mut limiters = self.ip_limiters.write().await;

        let limiter = limiters.entry(ip).or_insert_with(|| {
            let quota = Quota::per_minute(
                NonZeroU32::new(self.config.requests_per_minute)
                    .unwrap_or(NonZeroU32::new(100).unwrap()),
            )
            .allow_burst(
                NonZeroU32::new(self.config.burst_size).unwrap_or(NonZeroU32::new(10).unwrap()),
            );
            Arc::new(GovernorRateLimiter::direct(quota))
        });

        let limiter = Arc::clone(limiter);
        drop(limiters); // Release lock before checking

        match limiter.check() {
            Ok(_) => {
                debug!("Rate limit check passed for IP: {}", ip);
                Ok(())
            }
            Err(_) => {
                warn!("Rate limit exceeded for IP: {}", ip);
                Err(SecurityError::RateLimitExceeded)
            }
        }
    }

    /// Check rate limit for user
    pub async fn check_user_limit(&self, user_id: &str) -> Result<()> {
        if !self.config.enabled || !self.config.per_user {
            return Ok(());
        }

        let mut limiters = self.user_limiters.write().await;

        let limiter = limiters.entry(user_id.to_string()).or_insert_with(|| {
            let quota = Quota::per_minute(
                NonZeroU32::new(self.config.requests_per_minute)
                    .unwrap_or(NonZeroU32::new(100).unwrap()),
            )
            .allow_burst(
                NonZeroU32::new(self.config.burst_size).unwrap_or(NonZeroU32::new(10).unwrap()),
            );
            Arc::new(GovernorRateLimiter::direct(quota))
        });

        let limiter = Arc::clone(limiter);
        drop(limiters); // Release lock before checking

        match limiter.check() {
            Ok(_) => {
                debug!("Rate limit check passed for user: {}", user_id);
                Ok(())
            }
            Err(_) => {
                warn!("Rate limit exceeded for user: {}", user_id);
                Err(SecurityError::RateLimitExceeded)
            }
        }
    }

    /// Check global rate limit
    pub async fn check_global_limit(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if let Some(limiter) = &self.global_limiter {
            match limiter.check() {
                Ok(_) => {
                    debug!("Global rate limit check passed");
                    Ok(())
                }
                Err(_) => {
                    warn!("Global rate limit exceeded");
                    Err(SecurityError::RateLimitExceeded)
                }
            }
        } else {
            Ok(())
        }
    }

    /// Clean up old limiters to prevent memory leaks
    pub async fn cleanup_limiters(&self) -> Result<()> {
        let mut ip_limiters = self.ip_limiters.write().await;
        let mut user_limiters = self.user_limiters.write().await;

        let initial_ip_count = ip_limiters.len();
        let initial_user_count = user_limiters.len();

        // Remove limiters that haven't been used recently
        // This is a simplified cleanup - in production, you might want more sophisticated logic
        ip_limiters.retain(|_, limiter| Arc::strong_count(limiter) > 1);
        user_limiters.retain(|_, limiter| Arc::strong_count(limiter) > 1);

        let cleaned_ip = initial_ip_count - ip_limiters.len();
        let cleaned_user = initial_user_count - user_limiters.len();

        if cleaned_ip > 0 || cleaned_user > 0 {
            info!(
                "Cleaned up {} IP limiters and {} user limiters",
                cleaned_ip, cleaned_user
            );
        }

        Ok(())
    }

    /// Get rate limit statistics
    pub async fn get_statistics(&self) -> RateLimitStatistics {
        let ip_limiters = self.ip_limiters.read().await;
        let user_limiters = self.user_limiters.read().await;

        RateLimitStatistics {
            enabled: self.config.enabled,
            requests_per_minute: self.config.requests_per_minute,
            burst_size: self.config.burst_size,
            active_ip_limiters: ip_limiters.len(),
            active_user_limiters: user_limiters.len(),
            per_ip_enabled: self.config.per_ip,
            per_user_enabled: self.config.per_user,
            whitelist_count: self.config.whitelist_ips.len(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RateLimitStatistics {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub active_ip_limiters: usize,
    pub active_user_limiters: usize,
    pub per_ip_enabled: bool,
    pub per_user_enabled: bool,
    pub whitelist_count: usize,
}

/// Rate limiting middleware for Axum
pub async fn rate_limit_middleware(
    State(rate_limiter): State<Arc<RateLimitManager>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    if !rate_limiter.is_enabled() {
        return Ok(next.run(request).await);
    }

    // Check global rate limit first
    if let Err(_) = rate_limiter.check_global_limit().await {
        warn!("Global rate limit exceeded");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    // Check IP-based rate limit
    let ip = addr.ip();
    if let Err(_) = rate_limiter.check_ip_limit(ip).await {
        warn!("IP rate limit exceeded for: {}", ip);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    // Check user-based rate limit if user is authenticated
    if rate_limiter.config.per_user {
        if let Some(user_header) = headers.get("X-User-ID") {
            if let Ok(user_id) = user_header.to_str() {
                if let Err(_) = rate_limiter.check_user_limit(user_id).await {
                    warn!("User rate limit exceeded for: {}", user_id);
                    return Err(StatusCode::TOO_MANY_REQUESTS);
                }
            }
        }
    }

    debug!("Rate limit checks passed for IP: {}", ip);
    Ok(next.run(request).await)
}

/// Create rate limit middleware with custom configuration
pub fn create_rate_limit_middleware(
    requests_per_minute: u32,
    burst_size: u32,
    whitelist_ips: Vec<String>,
) -> RateLimitManager {
    let config = RateLimitConfig {
        enabled: true,
        requests_per_minute,
        burst_size,
        per_ip: true,
        per_user: true,
        whitelist_ips,
    };

    RateLimitManager::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_rate_limit_manager_creation() {
        let config = RateLimitConfig::default();
        let manager = RateLimitManager::new(config);
        assert!(!manager.is_enabled());
    }

    #[tokio::test]
    async fn test_disabled_rate_limiting() {
        let config = RateLimitConfig::default(); // disabled by default
        let manager = RateLimitManager::new(config);

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let result = manager.check_ip_limit(ip).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ip_whitelist() {
        let config = RateLimitConfig {
            enabled: true,
            requests_per_minute: 1, // Very low limit
            burst_size: 1,
            per_ip: true,
            per_user: false,
            whitelist_ips: vec!["192.168.1.1".to_string()],
        };

        let manager = RateLimitManager::new(config);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // Should pass even with low limit due to whitelist
        let result = manager.check_ip_limit(ip).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limit_exceeded() {
        let config = RateLimitConfig {
            enabled: true,
            requests_per_minute: 1,
            burst_size: 1,
            per_ip: true,
            per_user: false,
            whitelist_ips: Vec::new(),
        };

        let manager = RateLimitManager::new(config);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));

        // First request should pass
        let result1 = manager.check_ip_limit(ip).await;
        assert!(result1.is_ok());

        // Second request should fail (rate limit exceeded)
        let result2 = manager.check_ip_limit(ip).await;
        assert!(result2.is_err());

        if let Err(SecurityError::RateLimitExceeded) = result2 {
            // Expected error
        } else {
            panic!("Expected RateLimitExceeded error");
        }
    }

    #[tokio::test]
    async fn test_user_rate_limiting() {
        let config = RateLimitConfig {
            enabled: true,
            requests_per_minute: 1,
            burst_size: 1,
            per_ip: false,
            per_user: true,
            whitelist_ips: Vec::new(),
        };

        let manager = RateLimitManager::new(config);
        let user_id = "test-user";

        // First request should pass
        let result1 = manager.check_user_limit(user_id).await;
        assert!(result1.is_ok());

        // Second request should fail
        let result2 = manager.check_user_limit(user_id).await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_global_rate_limiting() {
        let config = RateLimitConfig {
            enabled: true,
            requests_per_minute: 1,
            burst_size: 1,
            per_ip: false,
            per_user: false,
            whitelist_ips: Vec::new(),
        };

        let manager = RateLimitManager::new(config);

        // First request should pass
        let result1 = manager.check_global_limit().await;
        assert!(result1.is_ok());

        // Second request should fail
        let result2 = manager.check_global_limit().await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_statistics() {
        let config = RateLimitConfig {
            enabled: true,
            requests_per_minute: 100,
            burst_size: 10,
            per_ip: true,
            per_user: true,
            whitelist_ips: vec!["127.0.0.1".to_string()],
        };

        let manager = RateLimitManager::new(config);
        let stats = manager.get_statistics().await;

        assert!(stats.enabled);
        assert_eq!(stats.requests_per_minute, 100);
        assert_eq!(stats.burst_size, 10);
        assert!(stats.per_ip_enabled);
        assert!(stats.per_user_enabled);
        assert_eq!(stats.whitelist_count, 1);
        assert_eq!(stats.active_ip_limiters, 0);
        assert_eq!(stats.active_user_limiters, 0);
    }

    #[tokio::test]
    async fn test_limiter_cleanup() {
        let config = RateLimitConfig {
            enabled: true,
            requests_per_minute: 100,
            burst_size: 10,
            per_ip: true,
            per_user: true,
            whitelist_ips: Vec::new(),
        };

        let manager = RateLimitManager::new(config);

        // Create some limiters
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let _ = manager.check_ip_limit(ip).await;
        let _ = manager.check_user_limit("test-user").await;

        let stats_before = manager.get_statistics().await;
        assert!(stats_before.active_ip_limiters > 0 || stats_before.active_user_limiters > 0);

        // Cleanup should work without errors
        let result = manager.cleanup_limiters().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_custom_rate_limiter_creation() {
        let manager =
            create_rate_limit_middleware(200, 20, vec!["127.0.0.1".to_string(), "::1".to_string()]);

        assert!(manager.is_enabled());
        assert_eq!(manager.config.requests_per_minute, 200);
        assert_eq!(manager.config.burst_size, 20);
        assert_eq!(manager.config.whitelist_ips.len(), 2);
    }
}
