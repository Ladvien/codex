use anyhow::Result;
use jsonrpc_core::{IoHandler, Params, Value};
use jsonrpc_tcp_server::{Server as TcpServer, ServerBuilder};
use memory_core::MemoryRepository;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
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

pub struct McpServer {
    config: McpServerConfig,
    repository: Arc<MemoryRepository>,
    handler: Arc<RwLock<IoHandler>>,
    correlation_counter: Arc<RwLock<u64>>,
}

impl McpServer {
    pub fn new(config: McpServerConfig, pool: PgPool) -> Self {
        let repository = Arc::new(MemoryRepository::new(pool));
        let handler = Arc::new(RwLock::new(IoHandler::new()));
        let correlation_counter = Arc::new(RwLock::new(0));

        Self {
            config,
            repository,
            handler,
            correlation_counter,
        }
    }

    pub async fn initialize(&self) -> Result<()> {
        self.register_handlers().await?;
        info!("MCP server initialized with {} handlers", self.handler.read().await.iter().count());
        Ok(())
    }

    async fn register_handlers(&self) -> Result<()> {
        let mut handler = self.handler.write().await;
        
        // Memory operations
        let repo = self.repository.clone();
        handler.add_method("memory.create", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let request: memory_core::CreateMemoryRequest = params.parse()?;
                let memory = repo.create_memory(request).await
                    .map_err(|e| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(memory).unwrap())
            })
        });

        let repo = self.repository.clone();
        handler.add_method("memory.get", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let (id,): (Uuid,) = params.parse()?;
                let memory = repo.get_memory(id).await
                    .map_err(|e| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(memory).unwrap())
            })
        });

        let repo = self.repository.clone();
        handler.add_method("memory.update", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let (id, request): (Uuid, memory_core::UpdateMemoryRequest) = params.parse()?;
                let memory = repo.update_memory(id, request).await
                    .map_err(|e| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(memory).unwrap())
            })
        });

        let repo = self.repository.clone();
        handler.add_method("memory.delete", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let (id,): (Uuid,) = params.parse()?;
                repo.delete_memory(id).await
                    .map_err(|e| jsonrpc_core::Error::internal_error())?;
                Ok(Value::Bool(true))
            })
        });

        let repo = self.repository.clone();
        handler.add_method("memory.search", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let request: memory_core::SearchRequest = params.parse()?;
                let results = repo.search_memories(request).await
                    .map_err(|e| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(results).unwrap())
            })
        });

        // Tier management
        let repo = self.repository.clone();
        handler.add_method("memory.migrate", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let (id, tier, reason): (Uuid, memory_core::MemoryTier, Option<String>) = params.parse()?;
                let memory = repo.migrate_memory(id, tier, reason).await
                    .map_err(|e| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(memory).unwrap())
            })
        });

        // Statistics
        let repo = self.repository.clone();
        handler.add_method("memory.statistics", move |params: Params| {
            let repo = repo.clone();
            Box::pin(async move {
                let stats = repo.get_statistics().await
                    .map_err(|e| jsonrpc_core::Error::internal_error())?;
                Ok(serde_json::to_value(stats).unwrap())
            })
        });

        // Health check
        handler.add_method("health", |_params| {
            Box::pin(async move {
                Ok(Value::Object(serde_json::Map::from_iter([
                    ("status".to_string(), Value::String("healthy".to_string())),
                    ("timestamp".to_string(), Value::String(chrono::Utc::now().to_rfc3339())),
                ])))
            })
        });

        Ok(())
    }

    pub async fn run_tcp(&self) -> Result<()> {
        let handler = self.handler.read().await.clone();
        
        let server = ServerBuilder::new(handler)
            .start(&self.config.tcp_addr)
            .map_err(|e| anyhow::anyhow!("Failed to start TCP server: {}", e))?;

        info!("MCP TCP server listening on {}", self.config.tcp_addr);
        
        // Keep the server running
        server.wait();
        Ok(())
    }

    pub async fn run_unix_socket(&self) -> Result<()> {
        if let Some(path) = &self.config.unix_socket_path {
            // Unix socket implementation would go here
            // For now, we'll just log that it's not implemented
            warn!("Unix socket support not yet implemented for path: {}", path);
        }
        Ok(())
    }

    async fn generate_correlation_id(&self) -> String {
        let mut counter = self.correlation_counter.write().await;
        *counter += 1;
        format!("mcp-{}-{}", Uuid::new_v4(), *counter)
    }

    pub async fn shutdown(&self) {
        info!("Shutting down MCP server");
        // Cleanup would go here
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