use crate::{
    cache::EmbeddingCache,
    claude::ClaudeEmbeddingProvider,
    cost_tracker::CostTracker,
    gpu::GpuEmbeddingProvider,
    local::LocalCpuEmbeddingProvider,
    metrics::MetricsCollector,
    router::EmbeddingRouter,
    BatchEmbeddingRequest, BatchEmbeddingResponse, EmbeddingConfig,
    EmbeddingError, EmbeddingProvider as ProviderEnum, EmbeddingProviderTrait,
    EmbeddingRequest, EmbeddingResponse, Result,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info};

pub struct EmbeddingPipeline {
    router: Arc<EmbeddingRouter>,
    cache: Arc<EmbeddingCache>,
    cost_tracker: Arc<CostTracker>,
    metrics: Arc<MetricsCollector>,
    config: EmbeddingConfig,
}

impl EmbeddingPipeline {
    pub async fn new(config: EmbeddingConfig) -> Result<Self> {
        // Initialize providers
        let claude_provider = if let Some(api_key) = &config.claude_api_key {
            Some(Arc::new(
                ClaudeEmbeddingProvider::new(
                    api_key.clone(),
                    config.claude_api_url.clone(),
                    config.claude_rate_limit,
                )?,
            ) as Arc<dyn EmbeddingProviderTrait>)
        } else {
            None
        };
        
        let gpu_provider = match GpuEmbeddingProvider::new(
            config.gpu_service_url.clone(),
            config.gpu_batch_size,
        ) {
            Ok(provider) => {
                provider.start_batch_processor().await;
                Some(Arc::new(provider) as Arc<dyn EmbeddingProviderTrait>)
            }
            Err(e) => {
                error!("Failed to initialize GPU provider: {}", e);
                None
            }
        };
        
        let local_provider = Arc::new(
            LocalCpuEmbeddingProvider::new(config.local_model_path.clone())?
        ) as Arc<dyn EmbeddingProviderTrait>;
        
        // Initialize router
        let router = Arc::new(EmbeddingRouter::new(
            claude_provider,
            gpu_provider,
            local_provider,
            config.fallback_enabled,
        ));
        
        // Initialize cache
        let cache = Arc::new(EmbeddingCache::new(
            config.cache_max_size as u64,
            config.cache_ttl,
        ));
        
        // Initialize tracking
        let cost_tracker = Arc::new(CostTracker::new());
        let metrics = if config.metrics_enabled {
            Arc::new(
                MetricsCollector::new()
                    .map_err(|e| EmbeddingError::Unknown(e.to_string()))?
            )
        } else {
            Arc::new(MetricsCollector::new_mock())
        };
        
        info!("Embedding pipeline initialized");
        
        Ok(Self {
            router,
            cache,
            cost_tracker,
            metrics,
            config,
        })
    }
    
    pub async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let start = Instant::now();
        
        // Check cache first
        if let Some(cached_embedding) = self.cache.get(&request.text).await {
            debug!("Cache hit for request {}", request.id);
            return Ok(EmbeddingResponse {
                id: request.id,
                embedding: cached_embedding,
                provider: ProviderEnum::LocalCpu, // Cache doesn't track original provider
                latency_ms: start.elapsed().as_millis() as u64,
                cost_usd: Some(0.0),
            });
        }
        
        // Route to appropriate provider
        let (embedding, provider) = self.router.route(&request).await?;
        
        // Cache the result
        self.cache.insert(&request.text, embedding.clone()).await;
        
        let latency = start.elapsed();
        let latency_ms = latency.as_millis() as u64;
        
        // Calculate cost
        let cost_usd = self.calculate_cost(&request.text, provider);
        
        // Track metrics
        if self.config.metrics_enabled {
            self.metrics.record_request(provider, true, latency, cost_usd);
        }
        
        // Track cost
        if self.config.cost_tracking_enabled {
            if let Some(cost) = cost_usd {
                self.cost_tracker.track(provider, cost, request.text.len() as u32 / 4);
            }
        }
        
        Ok(EmbeddingResponse {
            id: request.id,
            embedding,
            provider,
            latency_ms,
            cost_usd,
        })
    }
    
    pub async fn embed_batch(&self, request: BatchEmbeddingRequest) -> Result<BatchEmbeddingResponse> {
        let start = Instant::now();
        let mut responses = Vec::new();
        let mut total_cost_usd = 0.0;
        
        // Process each request
        // In production, would batch by provider for efficiency
        for req in request.requests {
            match self.embed(req).await {
                Ok(response) => {
                    if let Some(cost) = response.cost_usd {
                        total_cost_usd += cost;
                    }
                    responses.push(response);
                }
                Err(e) => {
                    error!("Failed to process embedding request: {}", e);
                    // Continue processing other requests
                }
            }
        }
        
        Ok(BatchEmbeddingResponse {
            responses,
            total_latency_ms: start.elapsed().as_millis() as u64,
            total_cost_usd: Some(total_cost_usd),
        })
    }
    
    fn calculate_cost(&self, text: &str, provider: ProviderEnum) -> Option<f64> {
        match provider {
            ProviderEnum::Claude => {
                // Rough estimation: 4 chars per token, $0.0001 per 1K tokens
                let tokens = text.len() / 4;
                Some((tokens as f64 / 1000.0) * 0.0001)
            }
            ProviderEnum::Gpu => {
                // GPU cost based on compute time (example)
                Some(0.00001) // Fixed small cost per request
            }
            ProviderEnum::LocalCpu => {
                Some(0.0) // No cost for local
            }
        }
    }
    
    pub async fn get_stats(&self) -> PipelineStats {
        let cache_stats = self.cache.stats().await;
        let total_cost = self.cost_tracker.get_total_cost();
        let cost_breakdown = self.cost_tracker.get_cost_breakdown();
        let provider_metrics = self.metrics.get_all_metrics();
        
        PipelineStats {
            cache_entries: cache_stats.entry_count,
            total_cost_usd: total_cost,
            cost_by_provider: cost_breakdown,
            provider_metrics,
        }
    }
    
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
        info!("Embedding cache cleared");
    }
}

#[derive(Debug, Clone)]
pub struct PipelineStats {
    pub cache_entries: u64,
    pub total_cost_usd: f64,
    pub cost_by_provider: std::collections::HashMap<ProviderEnum, f64>,
    pub provider_metrics: std::collections::HashMap<ProviderEnum, crate::ProviderMetrics>,
}