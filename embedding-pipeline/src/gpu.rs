use crate::{EmbeddingError, EmbeddingProviderTrait, Result};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

const GPU_EMBEDDING_DIMENSION: usize = 768;
const DEFAULT_BATCH_SIZE: usize = 32;

#[derive(Clone)]
pub struct GpuEmbeddingProvider {
    batch_queue: Arc<RwLock<VecDeque<BatchItem>>>,
    batch_semaphore: Arc<Semaphore>,
    config: GpuConfig,
}

#[derive(Debug, Clone)]
struct GpuConfig {
    model_path: String,
    batch_size: usize,
    max_queue_size: usize,
    batch_timeout_ms: u64,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            model_path: "/models/sentence-transformers/all-MiniLM-L6-v2".to_string(),
            batch_size: DEFAULT_BATCH_SIZE,
            max_queue_size: 1000,
            batch_timeout_ms: 50,
        }
    }
}

struct BatchItem {
    text: String,
    sender: tokio::sync::oneshot::Sender<Result<Vec<f32>>>,
}

impl GpuEmbeddingProvider {
    pub fn new(model_path: String, batch_size: usize) -> Result<Self> {
        let config = GpuConfig {
            model_path,
            batch_size,
            ..Default::default()
        };

        info!("Initialized GPU embedding provider (simulated)");

        Ok(Self {
            batch_queue: Arc::new(RwLock::new(VecDeque::new())),
            batch_semaphore: Arc::new(Semaphore::new(config.max_queue_size)),
            config,
        })
    }

    async fn process_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let start = Instant::now();
        
        // Simulate GPU processing
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Generate mock embeddings
        let embeddings: Vec<Vec<f32>> = texts
            .iter()
            .map(|text| {
                let mut embedding = vec![0.0; GPU_EMBEDDING_DIMENSION];
                // Create varied embeddings based on text
                for (i, ch) in text.chars().enumerate() {
                    let idx = i % GPU_EMBEDDING_DIMENSION;
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
            })
            .collect();
        
        let latency = start.elapsed();
        debug!("GPU batch processed {} texts in {:?}", texts.len(), latency);
        
        Ok(embeddings)
    }

    pub async fn start_batch_processor(&self) {
        let queue = self.batch_queue.clone();
        let config = self.config.clone();
        let provider = self.clone();
        
        tokio::spawn(async move {
            let mut batch_buffer = Vec::new();
            let mut batch_senders = Vec::new();
            
            loop {
                let timeout = tokio::time::sleep(Duration::from_millis(config.batch_timeout_ms));
                tokio::pin!(timeout);
                
                tokio::select! {
                    _ = &mut timeout => {
                        if !batch_buffer.is_empty() {
                            let texts = std::mem::take(&mut batch_buffer);
                            let senders = std::mem::take(&mut batch_senders);
                            provider.process_and_respond(texts, senders).await;
                        }
                    }
                }
                
                // Check queue for new items
                let items_to_process = {
                    let mut queue = queue.write();
                    let mut items = Vec::new();
                    while let Some(item) = queue.pop_front() {
                        items.push(item);
                        if items.len() >= config.batch_size {
                            break;
                        }
                    }
                    items
                };
                
                for item in items_to_process {
                    batch_buffer.push(item.text);
                    batch_senders.push(item.sender);
                    
                    if batch_buffer.len() >= config.batch_size {
                        let texts = std::mem::take(&mut batch_buffer);
                        let senders = std::mem::take(&mut batch_senders);
                        provider.process_and_respond(texts, senders).await;
                        break;
                    }
                }
            }
        });
    }

    async fn process_and_respond(
        &self,
        texts: Vec<String>,
        senders: Vec<tokio::sync::oneshot::Sender<Result<Vec<f32>>>>,
    ) {
        match self.process_batch(texts).await {
            Ok(embeddings) => {
                for (sender, embedding) in senders.into_iter().zip(embeddings.into_iter()) {
                    let _ = sender.send(Ok(embedding));
                }
            }
            Err(e) => {
                for sender in senders {
                    let _ = sender.send(Err(e.clone()));
                }
            }
        }
    }
}

#[async_trait]
impl EmbeddingProviderTrait for GpuEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        
        let _permit = self.batch_semaphore
            .acquire()
            .await
            .map_err(|_| EmbeddingError::GpuService("Batch queue full".to_string()))?;
        
        self.batch_queue.write().push_back(BatchItem {
            text: text.to_string(),
            sender,
        });
        
        receiver
            .await
            .map_err(|_| EmbeddingError::GpuService("Batch processor dropped".to_string()))?
    }

    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        self.process_batch(texts).await
    }

    fn dimension(&self) -> usize {
        GPU_EMBEDDING_DIMENSION
    }

    fn name(&self) -> &'static str {
        "GPU"
    }

    fn supports_batch(&self) -> bool {
        true
    }

    fn max_batch_size(&self) -> usize {
        self.config.batch_size
    }
}