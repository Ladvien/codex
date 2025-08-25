//! Model Context Protocol (MCP) Server Implementation
//!
//! This module provides a complete MCP server implementation that follows
//! the MCP protocol specification 2025-06-18 using stdio transport.
//!
//! The server exposes memory management tools through the MCP protocol,
//! allowing Claude to store, search, and manage hierarchical memories.

pub mod auth;
pub mod circuit_breaker;
pub mod handlers;
pub mod logging;
pub mod progress;
pub mod rate_limiter;
pub mod tools;
pub mod transport;

#[cfg(test)]
pub mod security_tests;

#[cfg(test)]
pub mod protocol_tests;

pub use auth::{AuthContext, AuthMethod, MCPAuth, MCPAuthConfig};
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerStats, CircuitState,
};
pub use handlers::MCPHandlers;
pub use logging::{LogLevel, LogMessage, MCPLogger};
pub use progress::{ProgressHandle, ProgressReport, ProgressTracker};
pub use rate_limiter::{MCPRateLimitConfig, MCPRateLimiter, RateLimitStats};
pub use tools::MCPTools;
pub use transport::StdioTransport;

use crate::memory::{
    ImportanceAssessmentConfig, ImportanceAssessmentPipeline, MemoryRepository,
    SilentHarvesterService,
};
use crate::security::{audit::AuditLogger, AuditConfig};
use crate::SimpleEmbedder;

#[cfg(feature = "codex-dreams")]
use crate::insights::processor::InsightsProcessor;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

/// MCP Server configuration
#[derive(Clone, Debug)]
pub struct MCPServerConfig {
    pub request_timeout_ms: u64, // Timeout for MCP requests (600000ms = 10min for LLM operations)
    pub max_request_size: usize,
    pub enable_circuit_breaker: bool,
    pub circuit_breaker: CircuitBreakerConfig,
    pub enable_authentication: bool,
    pub auth: MCPAuthConfig,
    pub enable_rate_limiting: bool,
    pub rate_limiting: MCPRateLimitConfig,
    pub audit: AuditConfig,
}

impl Default for MCPServerConfig {
    fn default() -> Self {
        Self {
            request_timeout_ms: 600000,         // 10 minutes for LLM operations
            max_request_size: 10 * 1024 * 1024, // 10MB
            enable_circuit_breaker: std::env::var("MCP_CIRCUIT_BREAKER_ENABLED")
                .map(|s| s.parse().unwrap_or(true))
                .unwrap_or(true),
            circuit_breaker: CircuitBreakerConfig::default(),
            enable_authentication: std::env::var("MCP_AUTH_ENABLED")
                .map(|s| s.parse().unwrap_or(true))
                .unwrap_or(true),
            auth: MCPAuthConfig::from_env(),
            enable_rate_limiting: std::env::var("MCP_RATE_LIMIT_ENABLED")
                .map(|s| s.parse().unwrap_or(true))
                .unwrap_or(true),
            rate_limiting: MCPRateLimitConfig::from_env(),
            audit: AuditConfig::default(),
        }
    }
}

/// Main MCP Server implementation
pub struct MCPServer {
    config: MCPServerConfig,
    repository: Arc<MemoryRepository>,
    embedder: Arc<SimpleEmbedder>,
    handlers: MCPHandlers,
    transport: StdioTransport,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
    harvester_service: Arc<SilentHarvesterService>,
    auth: Option<Arc<MCPAuth>>,
    rate_limiter: Option<Arc<MCPRateLimiter>>,
    audit_logger: Arc<AuditLogger>,
    mcp_logger: Arc<MCPLogger>,
    progress_tracker: Arc<ProgressTracker>,
    #[cfg(feature = "codex-dreams")]
    insights_processor: Option<Arc<InsightsProcessor>>,
}

impl MCPServer {
    /// Create a new MCP server instance
    #[cfg(not(feature = "codex-dreams"))]
    pub fn new(
        repository: Arc<MemoryRepository>,
        embedder: Arc<SimpleEmbedder>,
        config: MCPServerConfig,
    ) -> Result<Self> {
        Self::new_impl(repository, embedder, config, None, None)
    }

    /// Create a new MCP server instance with insights processor
    #[cfg(feature = "codex-dreams")]
    pub fn new(
        repository: Arc<MemoryRepository>,
        embedder: Arc<SimpleEmbedder>,
        config: MCPServerConfig,
    ) -> Result<Self> {
        Self::new_impl(repository, embedder, config, None, None)
    }

    /// Create a new MCP server instance with insights processor
    #[cfg(feature = "codex-dreams")]
    pub fn new_with_insights(
        repository: Arc<MemoryRepository>,
        embedder: Arc<SimpleEmbedder>,
        config: MCPServerConfig,
        insights_processor: Option<Arc<InsightsProcessor>>,
        insight_storage: Option<Arc<crate::insights::storage::InsightStorage>>,
    ) -> Result<Self> {
        Self::new_impl(
            repository,
            embedder,
            config,
            insights_processor,
            insight_storage,
        )
    }

    /// Internal implementation for creating MCP server
    fn new_impl(
        repository: Arc<MemoryRepository>,
        embedder: Arc<SimpleEmbedder>,
        config: MCPServerConfig,
        #[cfg(feature = "codex-dreams")] insights_processor: Option<Arc<InsightsProcessor>>,
        #[cfg(feature = "codex-dreams")] insight_storage: Option<
            Arc<crate::insights::storage::InsightStorage>,
        >,
        #[cfg(not(feature = "codex-dreams"))] _insights_processor: Option<()>,
        #[cfg(not(feature = "codex-dreams"))] _insight_storage: Option<()>,
    ) -> Result<Self> {
        // Initialize audit logger
        let audit_logger = Arc::new(AuditLogger::new(config.audit.clone())?);

        // Initialize MCP logger and progress tracker
        let mcp_logger = Arc::new(MCPLogger::new(LogLevel::Info));
        let progress_tracker = Arc::new(ProgressTracker::new());

        // Initialize authentication if enabled
        let auth = if config.enable_authentication {
            Some(Arc::new(MCPAuth::new(
                config.auth.clone(),
                audit_logger.clone(),
            )?))
        } else {
            None
        };

        // Initialize rate limiting if enabled
        let rate_limiter = if config.enable_rate_limiting {
            Some(Arc::new(MCPRateLimiter::new(
                config.rate_limiting.clone(),
                audit_logger.clone(),
            )?))
        } else {
            None
        };
        // Initialize Silent Harvester Service
        let importance_config = ImportanceAssessmentConfig::default();
        let importance_pipeline = Arc::new(ImportanceAssessmentPipeline::new(
            importance_config,
            embedder.clone(),
            prometheus::default_registry(),
        )?);

        let harvester_service = Arc::new(SilentHarvesterService::new(
            repository.clone(),
            importance_pipeline,
            embedder.clone(),
            None, // Use default config
            prometheus::default_registry(),
        )?);

        // Initialize circuit breaker if enabled
        let circuit_breaker = if config.enable_circuit_breaker {
            Some(Arc::new(CircuitBreaker::new(
                config.circuit_breaker.clone(),
            )))
        } else {
            None
        };

        // Create handlers
        #[cfg(feature = "codex-dreams")]
        let handlers = MCPHandlers::new_with_insights(
            repository.clone(),
            embedder.clone(),
            harvester_service.clone(),
            circuit_breaker.clone(),
            auth.clone(),
            rate_limiter.clone(),
            mcp_logger.clone(),
            progress_tracker.clone(),
            insights_processor.clone(),
            insight_storage.clone(),
        );

        #[cfg(not(feature = "codex-dreams"))]
        let handlers = MCPHandlers::new(
            repository.clone(),
            embedder.clone(),
            harvester_service.clone(),
            circuit_breaker.clone(),
            auth.clone(),
            rate_limiter.clone(),
            mcp_logger.clone(),
            progress_tracker.clone(),
        );

        // Create transport
        let transport = StdioTransport::new(config.request_timeout_ms)?;

        Ok(Self {
            config,
            repository,
            embedder,
            handlers,
            transport,
            circuit_breaker,
            harvester_service,
            auth,
            rate_limiter,
            audit_logger,
            mcp_logger,
            progress_tracker,
            #[cfg(feature = "codex-dreams")]
            insights_processor,
        })
    }

    /// Start the MCP server
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting MCP server with stdio transport");
        info!("Protocol version: 2025-06-18");
        info!("Capabilities: tools");

        // Start the transport layer
        self.transport.start(&mut self.handlers).await
    }

    /// Get server statistics
    pub async fn get_stats(&self) -> Result<serde_json::Value> {
        let repo_stats = self.repository.get_statistics().await?;
        let harvester_metrics = self.harvester_service.get_metrics().await;

        let circuit_breaker_stats = if let Some(ref cb) = self.circuit_breaker {
            Some(cb.get_stats().await)
        } else {
            None
        };

        let auth_stats = if let Some(ref auth) = self.auth {
            Some(auth.get_stats().await)
        } else {
            None
        };

        let rate_limit_stats = if let Some(ref rl) = self.rate_limiter {
            Some(rl.get_status().await)
        } else {
            None
        };

        Ok(serde_json::json!({
            "server": {
                "protocol_version": "2025-06-18",
                "transport": "stdio",
                "uptime_seconds": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                "security": {
                    "authentication_enabled": self.config.enable_authentication,
                    "rate_limiting_enabled": self.config.enable_rate_limiting,
                }
            },
            "memory_system": repo_stats,
            "harvester": harvester_metrics,
            "circuit_breaker": circuit_breaker_stats,
            "authentication": auth_stats,
            "rate_limiting": rate_limit_stats
        }))
    }

    /// Shutdown the server gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down MCP server");

        // Any cleanup logic here
        if let Some(ref cb) = self.circuit_breaker {
            cb.reset().await;
        }

        Ok(())
    }
}
