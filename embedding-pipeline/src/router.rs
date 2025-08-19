use crate::{
    EmbeddingError, EmbeddingProviderTrait,
    EmbeddingProvider as ProviderEnum, EmbeddingRequest, Result,
};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub struct EmbeddingRouter {
    claude_provider: Option<Arc<dyn EmbeddingProviderTrait>>,
    gpu_provider: Option<Arc<dyn EmbeddingProviderTrait>>,
    local_provider: Arc<dyn EmbeddingProviderTrait>,
    fallback_enabled: bool,
}

impl EmbeddingRouter {
    pub fn new(
        claude_provider: Option<Arc<dyn EmbeddingProviderTrait>>,
        gpu_provider: Option<Arc<dyn EmbeddingProviderTrait>>,
        local_provider: Arc<dyn EmbeddingProviderTrait>,
        fallback_enabled: bool,
    ) -> Self {
        Self {
            claude_provider,
            gpu_provider,
            local_provider,
            fallback_enabled,
        }
    }
    
    pub async fn route(&self, request: &EmbeddingRequest) -> Result<(Vec<f32>, ProviderEnum)> {
        // Determine provider based on importance and preferences
        let provider_order = self.determine_provider_order(request);
        
        let mut last_error = None;
        
        for provider in provider_order {
            debug!("Trying provider: {:?}", provider);
            
            let result = match provider {
                ProviderEnum::Claude => {
                    if let Some(ref claude) = self.claude_provider {
                        claude.embed(&request.text).await
                    } else {
                        continue;
                    }
                }
                ProviderEnum::Gpu => {
                    if let Some(ref gpu) = self.gpu_provider {
                        gpu.embed(&request.text).await
                    } else {
                        continue;
                    }
                }
                ProviderEnum::LocalCpu => {
                    self.local_provider.embed(&request.text).await
                }
            };
            
            match result {
                Ok(embedding) => {
                    info!("Successfully generated embedding with provider: {:?}", provider);
                    return Ok((embedding, provider));
                }
                Err(e) => {
                    warn!("Provider {:?} failed: {}", provider, e);
                    last_error = Some(e);
                    
                    if !self.fallback_enabled {
                        break;
                    }
                }
            }
        }
        
        error!("All providers failed for request {}", request.id);
        Err(last_error.unwrap_or(EmbeddingError::AllProvidersFailed))
    }
    
    fn determine_provider_order(&self, request: &EmbeddingRequest) -> Vec<ProviderEnum> {
        let mut order = Vec::new();
        
        use crate::RequestPriority;
        
        // If specific provider requested, try it first
        if let Some(preferred) = request.provider {
            order.push(preferred);
        }
        
        // Route based on priority
        match request.priority {
            RequestPriority::High => {
                // High priority: Claude -> GPU -> Local
                if !order.contains(&ProviderEnum::Claude) && self.claude_provider.is_some() {
                    order.push(ProviderEnum::Claude);
                }
                if !order.contains(&ProviderEnum::Gpu) && self.gpu_provider.is_some() {
                    order.push(ProviderEnum::Gpu);
                }
                if !order.contains(&ProviderEnum::LocalCpu) {
                    order.push(ProviderEnum::LocalCpu);
                }
            }
            RequestPriority::Normal => {
                // Normal priority: GPU -> Local -> Claude
                if !order.contains(&ProviderEnum::Gpu) && self.gpu_provider.is_some() {
                    order.push(ProviderEnum::Gpu);
                }
                if !order.contains(&ProviderEnum::LocalCpu) {
                    order.push(ProviderEnum::LocalCpu);
                }
                if !order.contains(&ProviderEnum::Claude) && self.claude_provider.is_some() {
                    order.push(ProviderEnum::Claude);
                }
            }
            RequestPriority::Low => {
                // Low priority: Local -> GPU -> Claude
                if !order.contains(&ProviderEnum::LocalCpu) {
                    order.push(ProviderEnum::LocalCpu);
                }
                if !order.contains(&ProviderEnum::Gpu) && self.gpu_provider.is_some() {
                    order.push(ProviderEnum::Gpu);
                }
                if !order.contains(&ProviderEnum::Claude) && self.claude_provider.is_some() {
                    order.push(ProviderEnum::Claude);
                }
            }
        }
        
        order
    }
}