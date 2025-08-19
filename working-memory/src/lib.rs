pub mod buffer;
pub mod embedder;
pub mod ffi;
pub mod storage;

use ahash::AHashMap;
use arc_swap::ArcSwap;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use lru::LruCache;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const CHUNK_SIZE: usize = 1024; // 1024 tokens per chunk
const CACHE_SIZE: usize = 10000;
const EVICTION_THRESHOLD: f64 = 0.85;

#[derive(Debug)]
pub struct MemoryChunk {
    pub id: Uuid,
    pub content: String,
    pub tokens: Vec<u32>,
    pub embedding: Option<Vec<f32>>,
    pub importance_score: f32,
    pub access_count: AtomicU64,
    pub last_accessed: AtomicU64,
    pub created_at: DateTime<Utc>,
}

impl MemoryChunk {
    pub fn new(content: String, tokens: Vec<u32>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content,
            tokens,
            embedding: None,
            importance_score: 0.5,
            access_count: AtomicU64::new(0),
            last_accessed: AtomicU64::new(0),
            created_at: Utc::now(),
        }
    }

    pub fn record_access(&self) {
        self.access_count.fetch_add(1, Ordering::Relaxed);
        self.last_accessed.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    pub fn calculate_eviction_score(&self) -> f64 {
        let access_count = self.access_count.load(Ordering::Relaxed) as f64;
        let last_accessed = self.last_accessed.load(Ordering::Relaxed);
        let age = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - last_accessed;
        
        // Higher score = more likely to evict
        let score = (1.0 / (access_count + 1.0)) * (age as f64 / 3600.0);
        score * (1.0 - self.importance_score as f64)
    }
}

pub struct WorkingMemory {
    // Lock-free concurrent hashmap for fast lookups
    chunks: Arc<DashMap<Uuid, Arc<MemoryChunk>, ahash::RandomState>>,
    
    // Lock-free queue for pending embeddings
    embedding_queue: Arc<SegQueue<Uuid>>,
    
    // LRU cache for frequently accessed chunks
    lru_cache: Arc<RwLock<LruCache<Uuid, Arc<MemoryChunk>>>>,
    
    // Memory-mapped circular buffer for zero-copy operations
    circular_buffer: Arc<buffer::CircularBuffer>,
    
    // Statistics
    total_chunks: AtomicUsize,
    total_bytes: AtomicUsize,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    
    // Configuration
    max_chunks: usize,
    eviction_threshold: f64,
}

impl WorkingMemory {
    pub fn new(max_chunks: usize) -> anyhow::Result<Self> {
        let circular_buffer = Arc::new(buffer::CircularBuffer::new(100 * 1024 * 1024)?); // 100MB
        let lru_cache = Arc::new(RwLock::new(LruCache::new(
            NonZeroUsize::new(CACHE_SIZE).unwrap(),
        )));

        Ok(Self {
            chunks: Arc::new(DashMap::with_hasher(ahash::RandomState::new())),
            embedding_queue: Arc::new(SegQueue::new()),
            lru_cache,
            circular_buffer,
            total_chunks: AtomicUsize::new(0),
            total_bytes: AtomicUsize::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            max_chunks,
            eviction_threshold: EVICTION_THRESHOLD,
        })
    }

    pub fn insert(&self, content: String) -> anyhow::Result<Uuid> {
        let start = Instant::now();
        
        // Check capacity
        if self.should_evict() {
            self.evict_lru()?;
        }

        // Tokenize content (would use actual tokenizer in production)
        let tokens = self.tokenize(&content);
        
        // Create chunk
        let chunk = Arc::new(MemoryChunk::new(content.clone(), tokens));
        let id = chunk.id;
        
        // Insert into storage
        self.chunks.insert(id, chunk.clone());
        
        // Add to LRU cache
        self.lru_cache.write().put(id, chunk.clone());
        
        // Queue for embedding generation
        self.embedding_queue.push(id);
        
        // Update statistics
        self.total_chunks.fetch_add(1, Ordering::Relaxed);
        self.total_bytes
            .fetch_add(content.len(), Ordering::Relaxed);
        
        let elapsed = start.elapsed();
        debug!("Inserted chunk {} in {:?}", id, elapsed);
        
        if elapsed > Duration::from_micros(100) {
            warn!("Slow insertion: {:?}", elapsed);
        }
        
        Ok(id)
    }

    pub fn get(&self, id: Uuid) -> Option<Arc<MemoryChunk>> {
        let start = Instant::now();
        
        // Check LRU cache first
        if let Some(chunk) = self.lru_cache.write().get(&id) {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            chunk.record_access();
            
            let elapsed = start.elapsed();
            if elapsed > Duration::from_micros(100) {
                warn!("Slow cache hit: {:?}", elapsed);
            }
            
            return Some(chunk.clone());
        }
        
        // Cache miss - check main storage
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        
        if let Some(entry) = self.chunks.get(&id) {
            let chunk = entry.clone();
            chunk.record_access();
            
            // Update LRU cache
            self.lru_cache.write().put(id, chunk.clone());
            
            let elapsed = start.elapsed();
            if elapsed > Duration::from_millis(1) {
                warn!("Slow retrieval: {:?}", elapsed);
            }
            
            Some(chunk)
        } else {
            None
        }
    }

    pub fn search_similar(&self, embedding: &[f32], limit: usize) -> Vec<(Uuid, f32)> {
        let start = Instant::now();
        
        // Parallel search using rayon
        use rayon::prelude::*;
        
        let similarities: Vec<_> = self
            .chunks
            .iter()
            .par_bridge()
            .filter_map(|entry| {
                let chunk = entry.value();
                if let Some(ref chunk_embedding) = chunk.embedding {
                    let similarity = cosine_similarity(embedding, chunk_embedding);
                    Some((entry.key().clone(), similarity))
                } else {
                    None
                }
            })
            .collect();
        
        // Sort by similarity
        let mut similarities = similarities;
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        similarities.truncate(limit);
        
        let elapsed = start.elapsed();
        debug!("Search completed in {:?}", elapsed);
        
        similarities
    }

    fn should_evict(&self) -> bool {
        let current = self.total_chunks.load(Ordering::Relaxed);
        current as f64 >= self.max_chunks as f64 * self.eviction_threshold
    }

    fn evict_lru(&self) -> anyhow::Result<()> {
        let start = Instant::now();
        
        // Find chunks with highest eviction scores
        let mut eviction_candidates: Vec<_> = self
            .chunks
            .iter()
            .map(|entry| {
                let chunk = entry.value();
                let score = chunk.calculate_eviction_score();
                (entry.key().clone(), score)
            })
            .collect();
        
        eviction_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Evict top 10% of candidates
        let evict_count = (self.max_chunks as f64 * 0.1) as usize;
        for (id, _) in eviction_candidates.iter().take(evict_count) {
            if let Some((_, chunk)) = self.chunks.remove(id) {
                self.total_chunks.fetch_sub(1, Ordering::Relaxed);
                self.total_bytes
                    .fetch_sub(chunk.content.len(), Ordering::Relaxed);
                
                // Trigger summarization before eviction
                self.summarize_chunk(&chunk)?;
            }
        }
        
        let elapsed = start.elapsed();
        info!("Evicted {} chunks in {:?}", evict_count, elapsed);
        
        Ok(())
    }

    fn summarize_chunk(&self, chunk: &MemoryChunk) -> anyhow::Result<()> {
        // In production, would call summarization service
        debug!("Summarizing chunk {} before eviction", chunk.id);
        Ok(())
    }

    fn tokenize(&self, content: &str) -> Vec<u32> {
        // Placeholder - would use actual tokenizer
        content
            .chars()
            .take(CHUNK_SIZE)
            .map(|c| c as u32)
            .collect()
    }

    pub fn get_stats(&self) -> MemoryStats {
        MemoryStats {
            total_chunks: self.total_chunks.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            cache_hit_rate: self.calculate_cache_hit_rate(),
            pending_embeddings: self.embedding_queue.len(),
        }
    }

    fn calculate_cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed) as f64;
        let misses = self.cache_misses.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        
        if total > 0.0 {
            hits / total
        } else {
            0.0
        }
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a * norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_chunks: usize,
    pub total_bytes: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub pending_embeddings: usize,
}

// Safety: WorkingMemory is thread-safe
unsafe impl Send for WorkingMemory {}
unsafe impl Sync for WorkingMemory {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory_creation() {
        let memory = WorkingMemory::new(1000).unwrap();
        assert_eq!(memory.total_chunks.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_insert_and_retrieve() {
        let memory = WorkingMemory::new(1000).unwrap();
        let content = "Test memory content".to_string();
        
        let id = memory.insert(content.clone()).unwrap();
        let retrieved = memory.get(id).unwrap();
        
        assert_eq!(retrieved.content, content);
        assert_eq!(retrieved.access_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_cache_hit_rate() {
        let memory = WorkingMemory::new(1000).unwrap();
        
        let id = memory.insert("Test".to_string()).unwrap();
        
        // First access - cache miss
        memory.get(id);
        
        // Second access - cache hit
        memory.get(id);
        
        let stats = memory.get_stats();
        assert!(stats.cache_hit_rate > 0.0);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 1.0);
        
        let c = vec![0.0, 1.0, 0.0];
        assert_eq!(cosine_similarity(&a, &c), 0.0);
    }
}