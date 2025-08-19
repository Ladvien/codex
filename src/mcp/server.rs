use crate::memory::{models::*, MemoryRepository};
use crate::monitoring::{AlertManager, HealthChecker, MetricsCollector, PerformanceProfiler};
use crate::SimpleEmbedder;
use anyhow::Result;
use jsonrpc_core::{IoHandler, Params, Value};
use jsonrpc_tcp_server::ServerBuilder;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
pub struct McpServerConfig {
    pub tcp_addr: SocketAddr,
    pub unix_socket_path: Option<String>,
    pub max_connections: u32,
    pub request_timeout_ms: u64,
    pub max_request_size: usize,
    pub enable_compression: bool,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            tcp_addr: ([127, 0, 0, 1], 3333).into(),
            unix_socket_path: None,
            max_connections: 1000,
            request_timeout_ms: 30000,
            max_request_size: 10 * 1024 * 1024, // 10MB
            enable_compression: true,
        }
    }
}

pub struct MCPServer {
    repository: Arc<MemoryRepository>,
    embedder: Arc<SimpleEmbedder>,
    metrics: Arc<MetricsCollector>,
    health_checker: Arc<HealthChecker>,
    alert_manager: Arc<tokio::sync::RwLock<AlertManager>>,
    profiler: Arc<PerformanceProfiler>,
}

impl MCPServer {
    pub fn new(repository: Arc<MemoryRepository>, embedder: Arc<SimpleEmbedder>) -> Result<Self> {
        let metrics = Arc::new(MetricsCollector::new()?);
        let health_checker = Arc::new(HealthChecker::new(Arc::new(repository.pool().clone())));
        let alert_manager = Arc::new(tokio::sync::RwLock::new(AlertManager::new()));
        let profiler = Arc::new(PerformanceProfiler::new());

        Ok(Self {
            repository,
            embedder,
            metrics,
            health_checker,
            alert_manager,
            profiler,
        })
    }

    pub async fn start(&self, addr: SocketAddr) -> Result<()> {
        let handler = self.create_handler().await;

        // Start background monitoring task
        self.start_monitoring_task().await;

        let server = ServerBuilder::new(handler)
            .start(&addr)
            .map_err(|e| anyhow::anyhow!("Failed to start MCP server: {}", e))?;

        info!("MCP server listening on {}", addr);
        server.wait();
        Ok(())
    }

    async fn start_monitoring_task(&self) {
        let health_checker = self.health_checker.clone();
        let alert_manager = self.alert_manager.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

            loop {
                interval.tick().await;

                // Update derived metrics
                metrics.update_derived_metrics();

                // Run health checks and evaluate alerts
                match health_checker.check_system_health().await {
                    Ok(health) => {
                        let mut alert_mgr = alert_manager.write().await;
                        alert_mgr.evaluate_alerts(&health, None);

                        // Cleanup old alerts (keep 24 hours of history)
                        alert_mgr.cleanup_old_alerts(24);
                    }
                    Err(e) => {
                        tracing::error!("Health check failed: {}", e);
                    }
                }
            }
        });

        info!("Started background monitoring task");
    }

    async fn create_handler(&self) -> IoHandler {
        let mut handler = IoHandler::new();

        // Memory operations
        let repo = self.repository.clone();
        let embedder = self.embedder.clone();
        let metrics = self.metrics.clone();
        let profiler = self.profiler.clone();
        handler.add_method("memory.create", move |params: Params| {
            let repo = repo.clone();
            let embedder = embedder.clone();
            let metrics = metrics.clone();
            let profiler = profiler.clone();
            Box::pin(async move {
                let _trace = profiler.start_trace("memory.create".to_string());
                let start_time = std::time::Instant::now();

                let mut request: CreateMemoryRequest = params.parse()?;

                // Generate embedding if not provided
                if request.embedding.is_none() {
                    if let Ok(embedding) = embedder.generate_embedding(&request.content).await {
                        request.embedding = Some(embedding);
                    }
                }

                let result = repo.create_memory(request).await;

                match &result {
                    Ok(_) => {
                        metrics.record_request(start_time);
                        metrics.memory_creation_total.inc();
                    }
                    Err(_) => {
                        metrics.record_db_query(start_time, false);
                    }
                }

                let memory = result.map_err(|_| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(memory).unwrap())
            })
        });

        let repo = self.repository.clone();
        handler.add_method("memory.get", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let (id,): (Uuid,) = params.parse()?;
                let memory = repo
                    .get_memory(id)
                    .await
                    .map_err(|_| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(memory).unwrap())
            })
        });

        let repo = self.repository.clone();
        handler.add_method("memory.update", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let (id, request): (Uuid, UpdateMemoryRequest) = params.parse()?;
                let memory = repo
                    .update_memory(id, request)
                    .await
                    .map_err(|_| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(memory).unwrap())
            })
        });

        let repo = self.repository.clone();
        handler.add_method("memory.delete", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let (id,): (Uuid,) = params.parse()?;
                repo.delete_memory(id)
                    .await
                    .map_err(|_| jsonrpc_core::Error::internal_error())?;
                Ok(Value::Bool(true))
            })
        });

        let repo = self.repository.clone();
        let embedder = self.embedder.clone();
        let metrics2 = self.metrics.clone();
        let profiler2 = self.profiler.clone();
        handler.add_method("memory.search", move |params: Params| {
            let repo = repo.clone();
            let embedder = embedder.clone();
            let metrics = metrics2.clone();
            let profiler = profiler2.clone();
            Box::pin(async move {
                let _trace = profiler.start_trace("memory.search".to_string());
                let start_time = std::time::Instant::now();

                let mut request: SearchRequest = params.parse()?;

                // Generate query embedding if needed
                if let Some(ref query_text) = request.query_text {
                    if request.query_embedding.is_none() {
                        if let Ok(embedding) = embedder.generate_embedding(query_text).await {
                            request.query_embedding = Some(embedding);
                        }
                    }
                }

                let result = repo.search_memories(request).await;

                match &result {
                    Ok(response) => {
                        metrics.record_search(start_time, response.results.len(), false);
                    }
                    Err(_) => {
                        metrics.record_db_query(start_time, false);
                    }
                }

                let results = result.map_err(|_| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(results).unwrap())
            })
        });

        // Tier management
        let repo = self.repository.clone();
        handler.add_method("memory.migrate", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let (id, tier, reason): (Uuid, MemoryTier, Option<String>) = params.parse()?;
                let memory = repo
                    .migrate_memory(id, tier, reason)
                    .await
                    .map_err(|_| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(memory).unwrap())
            })
        });

        // Statistics
        let repo = self.repository.clone();
        handler.add_method("memory.statistics", move |_params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let stats = repo
                    .get_statistics()
                    .await
                    .map_err(|_| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(stats).unwrap())
            })
        });

        // Health check
        let health_checker = self.health_checker.clone();
        handler.add_method("health", move |_params| {
            let health_checker = health_checker.clone();
            Box::pin(async move {
                match health_checker.check_system_health().await {
                    Ok(health) => Ok(serde_json::to_value(health).unwrap()),
                    Err(_) => Ok(Value::Object(serde_json::Map::from_iter([
                        ("status".to_string(), Value::String("unhealthy".to_string())),
                        (
                            "timestamp".to_string(),
                            Value::String(chrono::Utc::now().to_rfc3339()),
                        ),
                    ]))),
                }
            })
        });

        // Prometheus metrics endpoint
        let metrics = self.metrics.clone();
        handler.add_method("metrics", move |_params| {
            let metrics = metrics.clone();
            Box::pin(async move {
                let metrics_text = metrics.gather_metrics();
                Ok(Value::String(metrics_text))
            })
        });

        // Performance summary
        let profiler = self.profiler.clone();
        handler.add_method("performance", move |_params| {
            let profiler = profiler.clone();
            Box::pin(async move {
                let summary = profiler.get_performance_summary();
                Ok(serde_json::to_value(summary).unwrap())
            })
        });

        // Active alerts
        let alert_manager = self.alert_manager.clone();
        handler.add_method("alerts", move |_params| {
            let alert_manager = alert_manager.clone();
            Box::pin(async move {
                let binding = alert_manager.read().await;
                let alerts = binding.get_active_alerts();
                Ok(serde_json::to_value(alerts).unwrap())
            })
        });

        handler
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest<T> {
    pub id: String,
    pub method: String,
    pub params: T,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse<T> {
    pub id: String,
    pub result: Option<T>,
    pub error: Option<McpError>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = McpServerConfig::default();
        assert_eq!(config.max_connections, 1000);
        assert_eq!(config.request_timeout_ms, 30000);
    }
}
