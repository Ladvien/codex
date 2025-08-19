pub mod cache;
pub mod claude;
pub mod cost_tracker;
pub mod gpu;
pub mod local;
pub mod metrics;
pub mod pipeline;
pub mod router;

pub use pipeline::EmbeddingPipeline;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug, Clone)]
pub enum EmbeddingError {
    #[error("Claude API error: {0}")]
    ClaudeApi(String),
    
    #[error("GPU service error: {0}")]
    GpuService(String),
    
    #[error("Local embedder error: {0}")]
    LocalEmbedder(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
    
    #[error("All providers failed")]
    AllProvidersFailed,
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Cache error: {0}")]
    Cache(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, EmbeddingError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmbeddingProvider {
    Claude,
    Gpu,
    LocalCpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestPriority {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub id: Uuid,
    pub text: String,
    pub provider: Option<EmbeddingProvider>,
    pub priority: RequestPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub id: Uuid,
    pub embedding: Vec<f32>,
    pub provider: EmbeddingProvider,
    pub latency_ms: u64,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEmbeddingRequest {
    pub requests: Vec<EmbeddingRequest>,
    pub batch_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEmbeddingResponse {
    pub responses: Vec<EmbeddingResponse>,
    pub total_latency_ms: u64,
    pub total_cost_usd: Option<f64>,
}

#[async_trait]
pub trait EmbeddingProviderTrait: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
    fn name(&self) -> &'static str;
    fn supports_batch(&self) -> bool;
    fn max_batch_size(&self) -> usize;
}

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub claude_api_key: Option<String>,
    pub claude_api_url: String,
    pub claude_rate_limit: u32,
    pub gpu_service_url: String,
    pub gpu_batch_size: usize,
    pub local_model_path: Option<String>,
    pub cache_ttl: Duration,
    pub cache_max_size: usize,
    pub fallback_enabled: bool,
    pub cost_tracking_enabled: bool,
    pub metrics_enabled: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            claude_api_key: None,
            claude_api_url: "https://api.anthropic.com/v1/embeddings".to_string(),
            claude_rate_limit: 100,
            gpu_service_url: "http://localhost:8001".to_string(),
            gpu_batch_size: 32,
            local_model_path: None,
            cache_ttl: Duration::from_secs(3600),
            cache_max_size: 10000,
            fallback_enabled: true,
            cost_tracking_enabled: true,
            metrics_enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_latency_ms: u64,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub total_cost_usd: f64,
}

impl Default for ProviderMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_latency_ms: 0,
            avg_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            total_cost_usd: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.claude_rate_limit, 100);
        assert_eq!(config.gpu_batch_size, 32);
        assert!(config.fallback_enabled);
    }

    #[test]
    fn test_embedding_request_creation() {
        let request = EmbeddingRequest {
            id: Uuid::new_v4(),
            text: "Test text".to_string(),
            provider: Some(EmbeddingProvider::Claude),
            priority: RequestPriority::Normal,
        };
        
        assert_eq!(request.provider, Some(EmbeddingProvider::Claude));
        assert_eq!(request.priority, RequestPriority::Normal);
    }
}