use crate::{EmbeddingError, EmbeddingProviderTrait, Result};
use async_trait::async_trait;
use std::time::Instant;
use tracing::{debug, info};

const LOCAL_EMBEDDING_DIMENSION: usize = 384; // MiniLM dimension

pub struct LocalCpuEmbeddingProvider {
    // In production, this would contain the actual model
    model_name: String,
}

impl LocalCpuEmbeddingProvider {
    pub fn new(model_name: Option<String>) -> Result<Self> {
        let model_name = model_name.unwrap_or_else(|| "sentence-transformers/all-MiniLM-L6-v2".to_string());
        info!("Initialized local CPU embedder with model: {}", model_name);
        
        Ok(Self { model_name })
    }
    
    fn compute_embedding(&self, text: &str) -> Vec<f32> {
        // Simplified embedding computation
        // In production, use actual model inference
        let mut embedding = vec![0.0; LOCAL_EMBEDDING_DIMENSION];
        
        // Create a deterministic but varied embedding based on text
        for (i, ch) in text.chars().enumerate() {
            let idx = i % LOCAL_EMBEDDING_DIMENSION;
            embedding[idx] += (ch as u32) as f32 / 1000.0;
        }
        
        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }
        
        embedding
    }
}

#[async_trait]
impl EmbeddingProviderTrait for LocalCpuEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let start = Instant::now();
        
        if text.is_empty() {
            return Err(EmbeddingError::InvalidInput("Text cannot be empty".to_string()));
        }
        
        let embedding = tokio::task::spawn_blocking({
            let text = text.to_string();
            let provider = self.clone();
            move || provider.compute_embedding(&text)
        })
        .await
        .map_err(|e| EmbeddingError::LocalEmbedder(format!("Task failed: {}", e)))?;
        
        let latency = start.elapsed();
        debug!("Local CPU embedding completed in {:?}", latency);
        
        Ok(embedding)
    }
    
    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let start = Instant::now();
        
        let text_count = texts.len();
        let embeddings = tokio::task::spawn_blocking({
            let provider = self.clone();
            move || {
                texts.iter()
                    .map(|text| provider.compute_embedding(text))
                    .collect::<Vec<_>>()
            }
        })
        .await
        .map_err(|e| EmbeddingError::LocalEmbedder(format!("Batch task failed: {}", e)))?;
        
        let latency = start.elapsed();
        debug!("Local CPU batch embedding of {} texts completed in {:?}", text_count, latency);
        
        Ok(embeddings)
    }
    
    fn dimension(&self) -> usize {
        LOCAL_EMBEDDING_DIMENSION
    }
    
    fn name(&self) -> &'static str {
        "LocalCPU"
    }
    
    fn supports_batch(&self) -> bool {
        true
    }
    
    fn max_batch_size(&self) -> usize {
        64 // Can handle larger batches on CPU
    }
}

impl Clone for LocalCpuEmbeddingProvider {
    fn clone(&self) -> Self {
        Self {
            model_name: self.model_name.clone(),
        }
    }
}