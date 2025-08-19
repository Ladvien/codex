use async_trait::async_trait;
use memory_core::{
    CreateMemoryRequest, Memory, MemoryError, MemoryRepository, MemoryTier, SearchRequest,
    SearchResult, UpdateMemoryRequest,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

#[async_trait]
pub trait MemoryHandler: Send + Sync {
    async fn create_memory(&self, request: CreateMemoryRequest) -> Result<Memory, MemoryError>;
    async fn get_memory(&self, id: Uuid) -> Result<Memory, MemoryError>;
    async fn update_memory(
        &self,
        id: Uuid,
        request: UpdateMemoryRequest,
    ) -> Result<Memory, MemoryError>;
    async fn delete_memory(&self, id: Uuid) -> Result<(), MemoryError>;
    async fn search_memories(&self, request: SearchRequest) -> Result<Vec<SearchResult>, MemoryError>;
    async fn migrate_memory(
        &self,
        id: Uuid,
        to_tier: MemoryTier,
        reason: Option<String>,
    ) -> Result<Memory, MemoryError>;
}

pub struct MemoryHandlerImpl {
    repository: Arc<MemoryRepository>,
    circuit_breaker: Arc<crate::circuit_breaker::CircuitBreaker>,
    retry_policy: Arc<crate::retry::RetryPolicy>,
}

impl MemoryHandlerImpl {
    pub fn new(
        repository: Arc<MemoryRepository>,
        circuit_breaker: Arc<crate::circuit_breaker::CircuitBreaker>,
        retry_policy: Arc<crate::retry::RetryPolicy>,
    ) -> Self {
        Self {
            repository,
            circuit_breaker,
            retry_policy,
        }
    }
}

#[async_trait]
impl MemoryHandler for MemoryHandlerImpl {
    #[instrument(skip(self))]
    async fn create_memory(&self, request: CreateMemoryRequest) -> Result<Memory, MemoryError> {
        debug!("Creating memory with content length: {}", request.content.len());
        
        let repo = self.repository.clone();
        let result = self
            .retry_policy
            .execute(|| async {
                repo.create_memory(request.clone()).await
            })
            .await;

        match &result {
            Ok(memory) => info!("Created memory {} in tier {:?}", memory.id, memory.tier),
            Err(e) => error!("Failed to create memory: {}", e),
        }

        result
    }

    #[instrument(skip(self))]
    async fn get_memory(&self, id: Uuid) -> Result<Memory, MemoryError> {
        debug!("Getting memory {}", id);
        
        let repo = self.repository.clone();
        let result = self
            .retry_policy
            .execute(|| async {
                repo.get_memory(id).await
            })
            .await;

        match &result {
            Ok(memory) => debug!("Retrieved memory {} from tier {:?}", id, memory.tier),
            Err(e) => warn!("Failed to get memory {}: {}", id, e),
        }

        result
    }

    #[instrument(skip(self))]
    async fn update_memory(
        &self,
        id: Uuid,
        request: UpdateMemoryRequest,
    ) -> Result<Memory, MemoryError> {
        debug!("Updating memory {}", id);
        
        let repo = self.repository.clone();
        let result = self
            .retry_policy
            .execute(|| async {
                repo.update_memory(id, request.clone()).await
            })
            .await;

        match &result {
            Ok(memory) => info!("Updated memory {}", memory.id),
            Err(e) => error!("Failed to update memory {}: {}", id, e),
        }

        result
    }

    #[instrument(skip(self))]
    async fn delete_memory(&self, id: Uuid) -> Result<(), MemoryError> {
        debug!("Deleting memory {}", id);
        
        let repo = self.repository.clone();
        let result = self
            .retry_policy
            .execute(|| async {
                repo.delete_memory(id).await
            })
            .await;

        match &result {
            Ok(_) => info!("Deleted memory {}", id),
            Err(e) => error!("Failed to delete memory {}: {}", id, e),
        }

        result
    }

    #[instrument(skip(self, request))]
    async fn search_memories(&self, request: SearchRequest) -> Result<Vec<SearchResult>, MemoryError> {
        debug!("Searching memories with limit {:?}", request.limit);
        
        let repo = self.repository.clone();
        let result = self
            .retry_policy
            .execute(|| async {
                repo.search_memories(request.clone()).await
            })
            .await;

        match &result {
            Ok(results) => info!("Found {} memories matching search", results.len()),
            Err(e) => error!("Failed to search memories: {}", e),
        }

        result
    }

    #[instrument(skip(self))]
    async fn migrate_memory(
        &self,
        id: Uuid,
        to_tier: MemoryTier,
        reason: Option<String>,
    ) -> Result<Memory, MemoryError> {
        debug!("Migrating memory {} to tier {:?}", id, to_tier);
        
        let repo = self.repository.clone();
        let result = self
            .retry_policy
            .execute(|| async {
                repo.migrate_memory(id, to_tier, reason.clone()).await
            })
            .await;

        match &result {
            Ok(memory) => info!("Migrated memory {} to tier {:?}", id, memory.tier),
            Err(e) => error!("Failed to migrate memory {}: {}", id, e),
        }

        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: String,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResponse {
    pub total_requests: u64,
    pub active_connections: u32,
    pub memory_count: u64,
    pub error_rate: f64,
    pub avg_response_time_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
    use crate::retry::{RetryConfig, RetryPolicy};

    #[test]
    fn test_health_check_response_serialization() {
        let response = HealthCheckResponse {
            status: "healthy".to_string(),
            timestamp: chrono::Utc::now(),
            version: "1.0.0".to_string(),
            uptime_seconds: 3600,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("1.0.0"));
    }

    #[test]
    fn test_metrics_response_serialization() {
        let response = MetricsResponse {
            total_requests: 1000,
            active_connections: 10,
            memory_count: 500,
            error_rate: 0.01,
            avg_response_time_ms: 25.5,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: MetricsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_requests, 1000);
    }
}