use moka::future::Cache;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

#[derive(Clone)]
pub struct EmbeddingCache {
    cache: Arc<Cache<u64, Vec<f32>>>,
    ttl: Duration,
}

impl EmbeddingCache {
    pub fn new(max_capacity: u64, ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(ttl)
            .build();
        
        info!("Initialized embedding cache with capacity {} and TTL {:?}", max_capacity, ttl);
        
        Self {
            cache: Arc::new(cache),
            ttl,
        }
    }
    
    pub async fn get(&self, text: &str) -> Option<Vec<f32>> {
        let key = self.compute_key(text);
        let result = self.cache.get(&key).await;
        
        if result.is_some() {
            debug!("Cache hit for text hash {}", key);
        }
        
        result
    }
    
    pub async fn insert(&self, text: &str, embedding: Vec<f32>) {
        let key = self.compute_key(text);
        self.cache.insert(key, embedding).await;
        debug!("Cached embedding for text hash {}", key);
    }
    
    pub async fn invalidate(&self, text: &str) {
        let key = self.compute_key(text);
        self.cache.invalidate(&key).await;
        debug!("Invalidated cache for text hash {}", key);
    }
    
    pub async fn clear(&self) {
        self.cache.invalidate_all();
        info!("Cleared all cached embeddings");
    }
    
    pub async fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.cache.entry_count(),
            weighted_size: self.cache.weighted_size(),
        }
    }
    
    fn compute_key(&self, text: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entry_count: u64,
    pub weighted_size: u64,
}