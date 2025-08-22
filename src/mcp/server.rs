use crate::memory::{
    models::*, ConversationMessage, ImportanceAssessmentConfig, ImportanceAssessmentPipeline,
    MemoryRepository, SilentHarvesterService,
};
use crate::monitoring::{AlertManager, HealthChecker, MetricsCollector, PerformanceProfiler};
use crate::SimpleEmbedder;
use anyhow::Result;
use chrono::Utc;
use jsonrpc_core::{IoHandler, Params, Value};
use jsonrpc_tcp_server::ServerBuilder;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info};
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
    harvester_service: Arc<SilentHarvesterService>,
}

impl MCPServer {
    pub fn new(repository: Arc<MemoryRepository>, embedder: Arc<SimpleEmbedder>) -> Result<Self> {
        let metrics = Arc::new(MetricsCollector::new()?);
        let health_checker = Arc::new(HealthChecker::new(Arc::new(repository.pool().clone())));
        let alert_manager = Arc::new(tokio::sync::RwLock::new(AlertManager::new()));
        let profiler = Arc::new(PerformanceProfiler::new());

        // Initialize importance assessment pipeline
        let importance_config = ImportanceAssessmentConfig::default();
        let registry = metrics.registry();
        let importance_pipeline = Arc::new(ImportanceAssessmentPipeline::new(
            importance_config,
            embedder.clone(),
            &registry,
        )?);

        // Initialize silent harvester service
        let harvester_service = Arc::new(SilentHarvesterService::new(
            repository.clone(),
            importance_pipeline,
            embedder.clone(),
            None, // Use default config
            &registry,
        )?);

        Ok(Self {
            repository,
            embedder,
            metrics,
            health_checker,
            alert_manager,
            profiler,
            harvester_service,
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

        // Use tokio::spawn to handle the blocking wait call
        let server_handle = tokio::task::spawn_blocking(move || {
            server.wait();
        });

        // Wait for the server to finish
        server_handle
            .await
            .map_err(|e| anyhow::anyhow!("Server task failed: {}", e))?;
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
                serde_json::to_value(memory).map_err(|_| jsonrpc_core::Error::internal_error())
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
                serde_json::to_value(memory).map_err(|_| jsonrpc_core::Error::internal_error())
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
                serde_json::to_value(memory).map_err(|_| jsonrpc_core::Error::internal_error())
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
                serde_json::to_value(results).map_err(|_| jsonrpc_core::Error::internal_error())
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
                serde_json::to_value(memory).map_err(|_| jsonrpc_core::Error::internal_error())
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
                serde_json::to_value(stats).map_err(|_| jsonrpc_core::Error::internal_error())
            })
        });

        // Health check
        let health_checker = self.health_checker.clone();
        handler.add_method("health", move |_params| {
            let health_checker = health_checker.clone();
            Box::pin(async move {
                match health_checker.check_system_health().await {
                    Ok(health) => serde_json::to_value(health)
                        .map_err(|_| jsonrpc_core::Error::internal_error()),
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
                serde_json::to_value(summary).map_err(|_| jsonrpc_core::Error::internal_error())
            })
        });

        // Active alerts
        let alert_manager = self.alert_manager.clone();
        handler.add_method("alerts", move |_params| {
            let alert_manager = alert_manager.clone();
            Box::pin(async move {
                let binding = alert_manager.read().await;
                let alerts = binding.get_active_alerts();
                serde_json::to_value(alerts).map_err(|_| jsonrpc_core::Error::internal_error())
            })
        });

        // Silent harvester methods
        let harvester = self.harvester_service.clone();
        handler.add_method("background_memory_harvest", move |params: Params| {
            let harvester = harvester.clone();
            Box::pin(async move {
                #[derive(Debug, Deserialize)]
                struct HarvestRequest {
                    message: Option<String>,
                    context: Option<String>,
                    role: Option<String>,
                    silent_mode: Option<bool>,
                    force_harvest: Option<bool>,
                }

                let request: HarvestRequest = params.parse().unwrap_or(HarvestRequest {
                    message: None,
                    context: None,
                    role: None,
                    silent_mode: Some(true),
                    force_harvest: Some(false),
                });

                // If a message is provided, add it to the queue
                if let Some(message_content) = request.message {
                    let conversation_message = ConversationMessage {
                        id: Uuid::new_v4().to_string(),
                        content: message_content,
                        timestamp: Utc::now(),
                        role: request.role.unwrap_or_else(|| "user".to_string()),
                        context: request
                            .context
                            .unwrap_or_else(|| "conversation".to_string()),
                    };

                    if let Err(e) = harvester.add_message(conversation_message).await {
                        error!("Failed to add message to harvester: {}", e);
                        return Err(jsonrpc_core::Error::internal_error());
                    }
                }

                // If force_harvest is requested, trigger immediate harvest
                if request.force_harvest.unwrap_or(false) {
                    match harvester.force_harvest().await {
                        Ok(result) => {
                            if !request.silent_mode.unwrap_or(true) {
                                info!("Force harvest completed: {:?}", result);
                            }
                            serde_json::to_value(result)
                                .map_err(|_| jsonrpc_core::Error::internal_error())
                        }
                        Err(e) => {
                            error!("Force harvest failed: {}", e);
                            Err(jsonrpc_core::Error::internal_error())
                        }
                    }
                } else {
                    // Silent mode - return success without details
                    Ok(Value::Object(serde_json::Map::from_iter([
                        ("status".to_string(), Value::String("queued".to_string())),
                        (
                            "silent_mode".to_string(),
                            Value::Bool(request.silent_mode.unwrap_or(true)),
                        ),
                    ])))
                }
            })
        });

        // Harvester metrics
        let harvester = self.harvester_service.clone();
        handler.add_method("harvester.metrics", move |_params| {
            let harvester = harvester.clone();
            Box::pin(async move {
                let metrics = harvester.get_metrics().await;
                serde_json::to_value(metrics).map_err(|_| jsonrpc_core::Error::internal_error())
            })
        });

        // Query harvested memories
        let harvester = self.harvester_service.clone();
        handler.add_method("harvester.query", move |params: Params| {
            let harvester = harvester.clone();
            Box::pin(async move {
                #[derive(Debug, Deserialize)]
                struct QueryRequest {
                    query: Option<String>,
                }

                let request: QueryRequest = params.parse().unwrap_or(QueryRequest {
                    query: Some("what did you remember about me".to_string()),
                });

                // For now, return metrics summary - in future this could search stored memories
                let metrics = harvester.get_metrics().await;
                serde_json::to_value(serde_json::json!({
                    "status": "available",
                    "query": request.query.unwrap_or_else(|| "default".to_string()),
                    "metrics": metrics,
                    "message": "Memory harvesting is active. Use harvester.metrics for detailed information."
                })).map_err(|_| jsonrpc_core::Error::internal_error())
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
