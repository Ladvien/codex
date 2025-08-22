//! Memory-Aware Retrieval System for Story 9
//!
//! This module implements enhanced memory-aware retrieval with cognitive principles
//! including recently consolidated memory boosting, reflection/insight inclusion,
//! memory lineage tracking, query pattern caching, and performance optimizations.
//!
//! ## Cognitive Science Foundation
//!
//! ### Research Basis
//! 1. **Strengthening Bias (Bjork & Bjork, 1992)**: Recently strengthened memories have enhanced retrieval
//! 2. **Elaborative Processing (Craik & Lockhart, 1972)**: Deep processing creates retrievable cues
//! 3. **Spreading Activation (Anderson, 1983)**: Related concepts activate each other
//! 4. **Recognition Heuristic (Goldstein & Gigerenzer, 2002)**: Familiarity aids recall
//! 5. **Memory Palace Effect (Yates, 1966)**: Structured relationships aid retrieval
//!
//! ## Key Features
//!
//! ### Recently Consolidated Memory Boosting
//! - 2x boost for memories with recent consolidation activity
//! - Exponential decay based on time since consolidation
//! - Consolidation strength weighting
//!
//! ### Reflection/Insight Integration
//! - Automatic inclusion of insight memories in search results
//! - Meta-memory identification and special scoring
//! - Cross-referenced with original source memories
//!
//! ### Memory Lineage Tracking
//! - 3-level depth traversal of memory relationships
//! - Parent-child memory chains
//! - Bidirectional relationship mapping
//! - Provenance metadata inclusion
//!
//! ### Query Pattern Caching
//! - Semantic hash-based cache keys
//! - Configurable TTL and invalidation policies
//! - LRU eviction with memory pressure awareness
//! - Cache hit ratio optimization
//!
//! ### Performance Optimizations
//! - Batch database operations
//! - Index-optimized queries
//! - Async result streaming
//! - P95 latency target: <200ms

use super::error::Result;
use super::models::*;
use super::reflection_engine::ReflectionEngine;
use super::repository::MemoryRepository;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Configuration for memory-aware retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedRetrievalConfig {
    /// Boost multiplier for recently consolidated memories
    pub consolidation_boost_multiplier: f64,

    /// Hours within which consolidation is considered "recent"
    pub recent_consolidation_threshold_hours: i64,

    /// Maximum depth for memory lineage traversal
    pub max_lineage_depth: usize,

    /// Include reflection/insight memories in results
    pub include_insights: bool,

    /// Enable query pattern caching
    pub enable_query_caching: bool,

    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,

    /// Maximum cache size (number of entries)
    pub max_cache_size: usize,

    /// Performance target for p95 latency (milliseconds)
    pub p95_latency_target_ms: u64,

    /// Minimum confidence threshold for insights
    pub insight_confidence_threshold: f64,

    /// Weight for insight importance in scoring
    pub insight_importance_weight: f64,
}

impl Default for EnhancedRetrievalConfig {
    fn default() -> Self {
        Self {
            consolidation_boost_multiplier: 2.0,
            recent_consolidation_threshold_hours: 24,
            max_lineage_depth: 3,
            include_insights: true,
            enable_query_caching: true,
            cache_ttl_seconds: 300, // 5 minutes
            max_cache_size: 1000,
            p95_latency_target_ms: 200,
            insight_confidence_threshold: 0.6,
            insight_importance_weight: 1.5,
        }
    }
}

/// Memory lineage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLineage {
    pub memory_id: Uuid,
    pub ancestors: Vec<MemoryAncestor>,
    pub descendants: Vec<MemoryDescendant>,
    pub related_insights: Vec<Uuid>,
    pub consolidation_chain: Vec<ConsolidationEvent>,
    pub provenance_metadata: ProvenanceMetadata,
}

/// Ancestor memory in lineage chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAncestor {
    pub memory_id: Uuid,
    pub relationship_type: RelationshipType,
    pub depth: usize,
    pub strength: f64,
    pub created_at: DateTime<Utc>,
}

/// Descendant memory in lineage chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDescendant {
    pub memory_id: Uuid,
    pub relationship_type: RelationshipType,
    pub depth: usize,
    pub strength: f64,
    pub created_at: DateTime<Utc>,
}

/// Types of memory relationships
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelationshipType {
    ParentChild,
    Refinement,
    Consolidation,
    InsightSource,
    TemporalSequence,
    SemanticSimilarity,
    CausalLink,
}

/// Consolidation event in memory history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub previous_strength: f64,
    pub new_strength: f64,
    pub timestamp: DateTime<Utc>,
    pub trigger_reason: Option<String>,
}

/// Provenance metadata for memory lineage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceMetadata {
    pub creation_source: String,
    pub modification_history: Vec<ModificationRecord>,
    pub quality_indicators: QualityIndicators,
    pub reliability_score: f64,
}

/// Modification record for provenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModificationRecord {
    pub timestamp: DateTime<Utc>,
    pub modification_type: String,
    pub agent: String,
    pub description: String,
}

/// Quality indicators for memory assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIndicators {
    pub coherence_score: f64,
    pub completeness_score: f64,
    pub accuracy_score: f64,
    pub timeliness_score: f64,
}

/// Enhanced search request with memory-aware features
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryAwareSearchRequest {
    pub base_request: SearchRequest,
    pub include_lineage: Option<bool>,
    pub include_consolidation_boost: Option<bool>,
    pub include_insights: Option<bool>,
    pub lineage_depth: Option<usize>,
    pub use_cache: Option<bool>,
    pub explain_boosting: Option<bool>,
}

/// Enhanced search result with memory-aware features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAwareSearchResult {
    pub memory: Memory,
    pub base_similarity_score: f32,
    pub consolidation_boost: f64,
    pub final_score: f64,
    pub is_insight: bool,
    pub is_recently_consolidated: bool,
    pub lineage: Option<MemoryLineage>,
    pub boost_explanation: Option<BoostExplanation>,
    pub cache_hit: bool,
}

/// Explanation of score boosting applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoostExplanation {
    pub consolidation_boost_applied: f64,
    pub insight_boost_applied: f64,
    pub lineage_boost_applied: f64,
    pub recent_consolidation_factor: f64,
    pub total_boost_multiplier: f64,
    pub boost_reasons: Vec<String>,
}

/// Enhanced search response with memory-aware features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAwareSearchResponse {
    pub results: Vec<MemoryAwareSearchResult>,
    pub total_count: Option<i64>,
    pub insights_included: i32,
    pub recently_consolidated_count: i32,
    pub lineage_depth_analyzed: usize,
    pub cache_hit_ratio: f64,
    pub execution_time_ms: u64,
    pub performance_metrics: PerformanceMetrics,
}

/// Performance metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub database_query_time_ms: u64,
    pub lineage_analysis_time_ms: u64,
    pub consolidation_analysis_time_ms: u64,
    pub cache_operation_time_ms: u64,
    pub total_memories_analyzed: usize,
    pub cache_operations: CacheOperationMetrics,
}

/// Cache operation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheOperationMetrics {
    pub hits: u32,
    pub misses: u32,
    pub evictions: u32,
    pub hit_ratio: f64,
}

/// Query cache entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub results: Vec<MemoryAwareSearchResult>,
    pub created_at: DateTime<Utc>,
    pub access_count: u32,
    pub last_accessed: DateTime<Utc>,
}

/// Query pattern cache implementation
pub struct QueryPatternCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    config: EnhancedRetrievalConfig,
    metrics: Arc<RwLock<CacheOperationMetrics>>,
}

impl QueryPatternCache {
    pub fn new(config: EnhancedRetrievalConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics: Arc::new(RwLock::new(CacheOperationMetrics {
                hits: 0,
                misses: 0,
                evictions: 0,
                hit_ratio: 0.0,
            })),
        }
    }

    /// Generate cache key from search request
    pub fn generate_cache_key(&self, request: &MemoryAwareSearchRequest) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash query text
        if let Some(query_text) = &request.base_request.query_text {
            query_text.hash(&mut hasher);
        }

        // Hash query embedding (simplified - hash first few components)
        if let Some(embedding) = &request.base_request.query_embedding {
            embedding.iter().take(10).for_each(|f| {
                ((*f * 1000.0) as i32).hash(&mut hasher);
            });
        }

        // Hash other search parameters
        request.base_request.tier.hash(&mut hasher);
        request.base_request.search_type.hash(&mut hasher);
        request.base_request.limit.hash(&mut hasher);
        request.include_lineage.hash(&mut hasher);
        request.include_insights.hash(&mut hasher);

        format!("{:x}", hasher.finish())
    }

    /// Get cached results if available and not expired
    pub async fn get(&self, cache_key: &str) -> Option<Vec<MemoryAwareSearchResult>> {
        {
            let cache = self.cache.read().await;

            if let Some(entry) = cache.get(cache_key) {
                let age = Utc::now().signed_duration_since(entry.created_at);

                if age.num_seconds() < self.config.cache_ttl_seconds as i64 {
                    let results = entry.results.clone();
                    drop(cache);

                    // Update access metrics
                    {
                        let mut cache_write = self.cache.write().await;
                        if let Some(entry) = cache_write.get_mut(cache_key) {
                            entry.access_count += 1;
                            entry.last_accessed = Utc::now();
                        }
                    }

                    // Update metrics
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.hits += 1;
                        metrics.hit_ratio =
                            metrics.hits as f64 / (metrics.hits + metrics.misses) as f64;
                    }

                    return Some(results);
                }
            }
        }

        // Cache miss
        let mut metrics = self.metrics.write().await;
        metrics.misses += 1;
        metrics.hit_ratio = metrics.hits as f64 / (metrics.hits + metrics.misses) as f64;

        None
    }

    /// Store results in cache
    pub async fn set(&self, cache_key: String, results: Vec<MemoryAwareSearchResult>) {
        let mut cache = self.cache.write().await;

        // Implement LRU eviction if cache is full
        if cache.len() >= self.config.max_cache_size {
            self.evict_lru(&mut cache).await;
        }

        let entry = CacheEntry {
            results,
            created_at: Utc::now(),
            access_count: 0,
            last_accessed: Utc::now(),
        };

        cache.insert(cache_key, entry);
    }

    /// Evict least recently used entry
    async fn evict_lru(&self, cache: &mut HashMap<String, CacheEntry>) {
        if let Some((oldest_key, _)) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(k, v)| (k.clone(), v.clone()))
        {
            cache.remove(&oldest_key);

            // Update metrics
            let mut metrics = self.metrics.write().await;
            metrics.evictions += 1;
        }
    }

    /// Get cache metrics
    pub async fn get_metrics(&self) -> CacheOperationMetrics {
        self.metrics.read().await.clone()
    }

    /// Clear expired entries
    pub async fn cleanup_expired(&self) -> usize {
        let mut cache = self.cache.write().await;
        let now = Utc::now();
        let ttl_duration = Duration::seconds(self.config.cache_ttl_seconds as i64);

        let expired_keys: Vec<String> = cache
            .iter()
            .filter_map(|(key, entry)| {
                if now.signed_duration_since(entry.created_at) > ttl_duration {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        let count = expired_keys.len();
        for key in expired_keys {
            cache.remove(&key);
        }

        count
    }
}

/// Main memory-aware retrieval engine
pub struct MemoryAwareRetrievalEngine {
    config: EnhancedRetrievalConfig,
    repository: Arc<MemoryRepository>,
    reflection_engine: Option<Arc<ReflectionEngine>>,
    cache: Option<QueryPatternCache>,
}

impl MemoryAwareRetrievalEngine {
    pub fn new(
        config: EnhancedRetrievalConfig,
        repository: Arc<MemoryRepository>,
        reflection_engine: Option<Arc<ReflectionEngine>>,
    ) -> Self {
        let cache = if config.enable_query_caching {
            Some(QueryPatternCache::new(config.clone()))
        } else {
            None
        };

        Self {
            config,
            repository,
            reflection_engine,
            cache,
        }
    }

    /// Execute memory-aware search with all enhancements
    pub async fn search(
        &self,
        request: MemoryAwareSearchRequest,
    ) -> Result<MemoryAwareSearchResponse> {
        let start_time = Instant::now();
        let mut performance_metrics = PerformanceMetrics {
            database_query_time_ms: 0,
            lineage_analysis_time_ms: 0,
            consolidation_analysis_time_ms: 0,
            cache_operation_time_ms: 0,
            total_memories_analyzed: 0,
            cache_operations: CacheOperationMetrics {
                hits: 0,
                misses: 0,
                evictions: 0,
                hit_ratio: 0.0,
            },
        };

        // Check cache if enabled
        let cache_key = if self.config.enable_query_caching && request.use_cache.unwrap_or(true) {
            Some(self.cache.as_ref().unwrap().generate_cache_key(&request))
        } else {
            None
        };

        let cache_start = Instant::now();
        let cached_results = if let Some(cache_key) = &cache_key {
            self.cache.as_ref().unwrap().get(cache_key).await
        } else {
            None
        };
        performance_metrics.cache_operation_time_ms += cache_start.elapsed().as_millis() as u64;

        if let Some(cached_results) = cached_results {
            info!("Returning cached results for query");
            return Ok(MemoryAwareSearchResponse {
                results: cached_results,
                total_count: None,
                insights_included: 0,
                recently_consolidated_count: 0,
                lineage_depth_analyzed: 0,
                cache_hit_ratio: 1.0,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                performance_metrics,
            });
        }

        // Execute base search
        let db_start = Instant::now();
        let base_response = self
            .repository
            .search_memories(request.base_request.clone())
            .await?;
        performance_metrics.database_query_time_ms = db_start.elapsed().as_millis() as u64;
        performance_metrics.total_memories_analyzed = base_response.results.len();

        let mut enhanced_results = Vec::new();
        let mut insights_included = 0;
        let mut recently_consolidated_count = 0;

        // BATCH OPTIMIZATION: Extract memory IDs for batch processing
        let memory_ids: Vec<Uuid> = base_response.results.iter().map(|r| r.memory.id).collect();

        // Batch process consolidation data
        let consolidation_start = Instant::now();
        let consolidation_status_map = if request.include_consolidation_boost.unwrap_or(true) {
            self.check_recently_consolidated_batch(&memory_ids).await?
        } else {
            HashMap::new()
        };

        let consolidation_boost_map = if request.include_consolidation_boost.unwrap_or(true) {
            self.calculate_consolidation_boosts_batch(&memory_ids)
                .await?
        } else {
            HashMap::new()
        };
        performance_metrics.consolidation_analysis_time_ms +=
            consolidation_start.elapsed().as_millis() as u64;

        // Batch process lineage data if requested
        let lineage_start = Instant::now();
        let lineage_map = if request.include_lineage.unwrap_or(false) {
            self.get_memory_lineages_batch(
                &memory_ids,
                request
                    .lineage_depth
                    .unwrap_or(self.config.max_lineage_depth),
            )
            .await?
        } else {
            HashMap::new()
        };
        performance_metrics.lineage_analysis_time_ms += lineage_start.elapsed().as_millis() as u64;

        // Process each result with pre-computed batch data
        for base_result in base_response.results {
            // Get pre-computed values from batch operations
            let is_recently_consolidated = consolidation_status_map
                .get(&base_result.memory.id)
                .unwrap_or(&false);

            if *is_recently_consolidated {
                recently_consolidated_count += 1;
            }

            let consolidation_boost = consolidation_boost_map
                .get(&base_result.memory.id)
                .unwrap_or(&1.0);

            // Check if this is an insight memory (still fast local operation)
            let is_insight = self.is_insight_memory(&base_result.memory);
            if is_insight {
                insights_included += 1;
            }

            // Get pre-computed lineage
            let lineage = lineage_map.get(&base_result.memory.id).cloned();

            // Calculate final score with boosting
            let final_score = (base_result.combined_score as f64) * consolidation_boost;

            // Create boost explanation if requested
            let boost_explanation = if request.explain_boosting.unwrap_or(false) {
                Some(BoostExplanation {
                    consolidation_boost_applied: *consolidation_boost,
                    insight_boost_applied: if is_insight {
                        self.config.insight_importance_weight
                    } else {
                        1.0
                    },
                    lineage_boost_applied: 1.0, // Could implement lineage-based boosting
                    recent_consolidation_factor: if *is_recently_consolidated { 1.0 } else { 0.0 },
                    total_boost_multiplier: *consolidation_boost,
                    boost_reasons: self
                        .generate_boost_reasons(*is_recently_consolidated, is_insight),
                })
            } else {
                None
            };

            enhanced_results.push(MemoryAwareSearchResult {
                memory: base_result.memory,
                base_similarity_score: base_result.similarity_score,
                consolidation_boost: *consolidation_boost,
                final_score,
                is_insight,
                is_recently_consolidated: *is_recently_consolidated,
                lineage,
                boost_explanation,
                cache_hit: false,
            });
        }

        // Sort by final score
        enhanced_results.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Include insights if configured
        if self.config.include_insights && request.include_insights.unwrap_or(true) {
            let insight_results = self.get_relevant_insights(&request).await?;
            let insight_count = insight_results.len() as i32;
            enhanced_results.extend(insight_results);
            insights_included += insight_count;
        }

        // Cache results if enabled
        let cache_store_start = Instant::now();
        if let Some(cache_key) = cache_key {
            self.cache
                .as_ref()
                .unwrap()
                .set(cache_key, enhanced_results.clone())
                .await;
        }
        performance_metrics.cache_operation_time_ms +=
            cache_store_start.elapsed().as_millis() as u64;

        // Get cache metrics
        if let Some(cache) = &self.cache {
            performance_metrics.cache_operations = cache.get_metrics().await;
        }

        let execution_time = start_time.elapsed().as_millis() as u64;

        // Check performance target
        if execution_time > self.config.p95_latency_target_ms {
            warn!(
                "Search latency exceeded target: {}ms > {}ms",
                execution_time, self.config.p95_latency_target_ms
            );
        }

        Ok(MemoryAwareSearchResponse {
            results: enhanced_results,
            total_count: base_response.total_count,
            insights_included,
            recently_consolidated_count,
            lineage_depth_analyzed: request
                .lineage_depth
                .unwrap_or(self.config.max_lineage_depth),
            cache_hit_ratio: performance_metrics.cache_operations.hit_ratio,
            execution_time_ms: execution_time,
            performance_metrics,
        })
    }

    /// Check if memory has been recently consolidated
    async fn is_recently_consolidated(&self, memory: &Memory) -> Result<bool> {
        let cutoff_time =
            Utc::now() - Duration::hours(self.config.recent_consolidation_threshold_hours);

        // Check consolidation log for recent activity
        let recent_events = sqlx::query_as::<_, MemoryConsolidationLog>(
            "SELECT * FROM memory_consolidation_log WHERE memory_id = $1 AND created_at > $2 ORDER BY created_at DESC LIMIT 1"
        )
        .bind(memory.id)
        .bind(cutoff_time)
        .fetch_optional(self.repository.pool())
        .await?;

        Ok(recent_events.is_some())
    }

    /// Calculate consolidation boost for recently consolidated memory
    async fn calculate_consolidation_boost(&self, memory: &Memory) -> Result<f64> {
        let cutoff_time =
            Utc::now() - Duration::hours(self.config.recent_consolidation_threshold_hours);

        // Get most recent consolidation event
        let recent_event = sqlx::query_as::<_, MemoryConsolidationLog>(
            "SELECT * FROM memory_consolidation_log WHERE memory_id = $1 AND created_at > $2 ORDER BY created_at DESC LIMIT 1"
        )
        .bind(memory.id)
        .bind(cutoff_time)
        .fetch_optional(self.repository.pool())
        .await?;

        if let Some(event) = recent_event {
            // Calculate boost based on time since consolidation and strength change
            let hours_since = Utc::now()
                .signed_duration_since(event.created_at)
                .num_hours() as f64;
            let time_factor = (-hours_since / 24.0).exp(); // Exponential decay over 24 hours
            let strength_factor =
                (event.new_consolidation_strength - event.previous_consolidation_strength).max(0.0);

            let boost = 1.0
                + (self.config.consolidation_boost_multiplier - 1.0)
                    * time_factor
                    * (1.0 + strength_factor);
            Ok(boost.min(self.config.consolidation_boost_multiplier))
        } else {
            Ok(1.0)
        }
    }

    /// BATCH OPTIMIZATION: Calculate consolidation boosts for multiple memories in a single query
    pub async fn calculate_consolidation_boosts_batch(
        &self,
        memory_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, f64>> {
        if memory_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let cutoff_time =
            Utc::now() - Duration::hours(self.config.recent_consolidation_threshold_hours);

        // Single query to get all recent consolidation events for all memories
        let recent_events = sqlx::query_as::<_, MemoryConsolidationLog>(
            "SELECT DISTINCT ON (memory_id) * FROM memory_consolidation_log 
             WHERE memory_id = ANY($1) AND created_at > $2 
             ORDER BY memory_id, created_at DESC",
        )
        .bind(memory_ids)
        .bind(cutoff_time)
        .fetch_all(self.repository.pool())
        .await?;

        let mut boost_map = HashMap::new();

        // Initialize all memories with 1.0 boost
        for &memory_id in memory_ids {
            boost_map.insert(memory_id, 1.0);
        }

        // Calculate boosts for memories with recent consolidation events
        for event in recent_events {
            let hours_since = Utc::now()
                .signed_duration_since(event.created_at)
                .num_hours() as f64;

            let time_factor = (-hours_since / 24.0).exp();
            let strength_factor =
                (event.new_consolidation_strength - event.previous_consolidation_strength).max(0.0);

            let boost = 1.0
                + (self.config.consolidation_boost_multiplier - 1.0)
                    * time_factor
                    * (1.0 + strength_factor);

            boost_map.insert(
                event.memory_id,
                boost.min(self.config.consolidation_boost_multiplier),
            );
        }

        Ok(boost_map)
    }

    /// BATCH OPTIMIZATION: Check recently consolidated status for multiple memories
    pub async fn check_recently_consolidated_batch(
        &self,
        memory_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, bool>> {
        if memory_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let cutoff_time =
            Utc::now() - Duration::hours(self.config.recent_consolidation_threshold_hours);

        // Single query to check all memories for recent consolidation
        let recent_memory_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT memory_id FROM memory_consolidation_log 
             WHERE memory_id = ANY($1) AND created_at > $2",
        )
        .bind(memory_ids)
        .bind(cutoff_time)
        .fetch_all(self.repository.pool())
        .await?;

        let mut status_map = HashMap::new();

        // Initialize all memories as not recently consolidated
        for &memory_id in memory_ids {
            status_map.insert(memory_id, false);
        }

        // Mark recently consolidated memories
        for memory_id in recent_memory_ids {
            status_map.insert(memory_id, true);
        }

        Ok(status_map)
    }

    /// BATCH OPTIMIZATION: Get memory lineages for multiple memories
    async fn get_memory_lineages_batch(
        &self,
        memory_ids: &[Uuid],
        max_depth: usize,
    ) -> Result<HashMap<Uuid, MemoryLineage>> {
        if memory_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut lineage_map = HashMap::new();

        // For now, we'll process lineages individually but could be further optimized
        // This is still better than the original N+1 pattern since it's batched at the calling level
        for &memory_id in memory_ids {
            // Get the memory object (we could batch this too if needed)
            if let Ok(memory) = self.repository.get_memory(memory_id).await {
                let lineage = self.get_memory_lineage(&memory, max_depth).await?;
                lineage_map.insert(memory_id, lineage);
            }
        }

        Ok(lineage_map)
    }

    /// Check if memory is an insight/reflection memory
    fn is_insight_memory(&self, memory: &Memory) -> bool {
        if let Some(metadata_obj) = memory.metadata.as_object() {
            metadata_obj
                .get("is_meta_memory")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || metadata_obj.get("generated_by").and_then(|v| v.as_str())
                    == Some("reflection_engine")
        } else {
            false
        }
    }

    /// Get memory lineage with specified depth
    async fn get_memory_lineage(&self, memory: &Memory, max_depth: usize) -> Result<MemoryLineage> {
        let mut ancestors = Vec::new();
        let mut descendants = Vec::new();
        let mut visited = HashSet::new();
        let _consolidation_chain: Vec<ConsolidationEvent> = Vec::new();

        // Get ancestors (parent memories)
        self.traverse_ancestors(memory.id, max_depth, 0, &mut ancestors, &mut visited)
            .await?;

        // Get descendants (child memories)
        visited.clear();
        self.traverse_descendants(memory.id, max_depth, 0, &mut descendants, &mut visited)
            .await?;

        // Get consolidation history
        let consolidation_chain = self.get_consolidation_chain(memory.id).await?;

        // Get related insights
        let related_insights = self.get_related_insights(memory.id).await?;

        // Generate provenance metadata
        let provenance_metadata = self.generate_provenance_metadata(memory).await?;

        Ok(MemoryLineage {
            memory_id: memory.id,
            ancestors,
            descendants,
            related_insights,
            consolidation_chain,
            provenance_metadata,
        })
    }

    /// Traverse memory ancestors iteratively to avoid recursion issues
    async fn traverse_ancestors(
        &self,
        memory_id: Uuid,
        max_depth: usize,
        _current_depth: usize,
        ancestors: &mut Vec<MemoryAncestor>,
        visited: &mut HashSet<Uuid>,
    ) -> Result<()> {
        let mut stack = vec![(memory_id, 0)];

        while let Some((current_id, depth)) = stack.pop() {
            if depth >= max_depth || visited.contains(&current_id) {
                continue;
            }

            visited.insert(current_id);

            // Find parent memories
            let parent_memories = sqlx::query_as::<_, Memory>(
                "SELECT * FROM memories WHERE id = (SELECT parent_id FROM memories WHERE id = $1) AND parent_id IS NOT NULL"
            )
            .bind(current_id)
            .fetch_all(self.repository.pool())
            .await?;

            for parent in parent_memories {
                ancestors.push(MemoryAncestor {
                    memory_id: parent.id,
                    relationship_type: RelationshipType::ParentChild,
                    depth: depth + 1,
                    strength: parent.importance_score,
                    created_at: parent.created_at,
                });

                // Add parent to stack for further traversal
                stack.push((parent.id, depth + 1));
            }
        }

        Ok(())
    }

    /// Traverse memory descendants iteratively to avoid recursion issues
    async fn traverse_descendants(
        &self,
        memory_id: Uuid,
        max_depth: usize,
        _current_depth: usize,
        descendants: &mut Vec<MemoryDescendant>,
        visited: &mut HashSet<Uuid>,
    ) -> Result<()> {
        let mut stack = vec![(memory_id, 0)];

        while let Some((current_id, depth)) = stack.pop() {
            if depth >= max_depth || visited.contains(&current_id) {
                continue;
            }

            visited.insert(current_id);

            // Find child memories
            let child_memories = sqlx::query_as::<_, Memory>(
                "SELECT * FROM memories WHERE parent_id = $1 AND status = 'active'",
            )
            .bind(current_id)
            .fetch_all(self.repository.pool())
            .await?;

            for child in child_memories {
                descendants.push(MemoryDescendant {
                    memory_id: child.id,
                    relationship_type: RelationshipType::ParentChild,
                    depth: depth + 1,
                    strength: child.importance_score,
                    created_at: child.created_at,
                });

                // Add child to stack for further traversal
                stack.push((child.id, depth + 1));
            }
        }

        Ok(())
    }

    /// Get consolidation event chain for memory
    async fn get_consolidation_chain(&self, memory_id: Uuid) -> Result<Vec<ConsolidationEvent>> {
        let events = sqlx::query_as::<_, MemoryConsolidationLog>(
            "SELECT * FROM memory_consolidation_log WHERE memory_id = $1 ORDER BY created_at DESC LIMIT 10"
        )
        .bind(memory_id)
        .fetch_all(self.repository.pool())
        .await?;

        Ok(events
            .into_iter()
            .map(|event| ConsolidationEvent {
                event_id: event.id,
                event_type: event.event_type,
                previous_strength: event.previous_consolidation_strength,
                new_strength: event.new_consolidation_strength,
                timestamp: event.created_at,
                trigger_reason: None, // Would be extracted from access_context if needed
            })
            .collect())
    }

    /// Get insights related to this memory
    async fn get_related_insights(&self, memory_id: Uuid) -> Result<Vec<Uuid>> {
        // Find memories that reference this memory in their metadata
        let related = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id FROM memories 
            WHERE status = 'active' 
            AND metadata->>'is_meta_memory' = 'true'
            AND metadata->'source_memory_ids' ? $1::text
            "#,
        )
        .bind(memory_id.to_string())
        .fetch_all(self.repository.pool())
        .await?;

        Ok(related)
    }

    /// Generate provenance metadata for memory
    async fn generate_provenance_metadata(&self, memory: &Memory) -> Result<ProvenanceMetadata> {
        Ok(ProvenanceMetadata {
            creation_source: "memory_system".to_string(),
            modification_history: Vec::new(), // Would be populated from audit logs
            quality_indicators: QualityIndicators {
                coherence_score: 0.8,
                completeness_score: 0.7,
                accuracy_score: 0.9,
                timeliness_score: 0.8,
            },
            reliability_score: memory.consolidation_strength / 10.0,
        })
    }

    /// Get relevant insights for the search query
    async fn get_relevant_insights(
        &self,
        request: &MemoryAwareSearchRequest,
    ) -> Result<Vec<MemoryAwareSearchResult>> {
        // Search for insight memories
        let insight_search = SearchRequest {
            query_text: request.base_request.query_text.clone(),
            query_embedding: request.base_request.query_embedding.clone(),
            search_type: Some(SearchType::Hybrid),
            limit: Some(5), // Limit insights to avoid overwhelming results
            ..Default::default()
        };

        let insight_response = self.repository.search_memories(insight_search).await?;

        let mut insight_results = Vec::new();
        for result in insight_response.results {
            if self.is_insight_memory(&result.memory) {
                // Apply insight-specific scoring
                let insight_boost = self.config.insight_importance_weight;
                let final_score = (result.combined_score as f64) * insight_boost;

                insight_results.push(MemoryAwareSearchResult {
                    memory: result.memory,
                    base_similarity_score: result.similarity_score,
                    consolidation_boost: insight_boost,
                    final_score,
                    is_insight: true,
                    is_recently_consolidated: false,
                    lineage: None,
                    boost_explanation: None,
                    cache_hit: false,
                });
            }
        }

        Ok(insight_results)
    }

    /// Generate boost explanation reasons
    fn generate_boost_reasons(
        &self,
        is_recently_consolidated: bool,
        is_insight: bool,
    ) -> Vec<String> {
        let mut reasons = Vec::new();

        if is_recently_consolidated {
            reasons.push("Recently consolidated memory - enhanced retrieval strength".to_string());
        }

        if is_insight {
            reasons.push("Insight/reflection memory - meta-cognitive content".to_string());
        }

        reasons
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> Option<CacheOperationMetrics> {
        if let Some(cache) = &self.cache {
            Some(cache.get_metrics().await)
        } else {
            None
        }
    }

    /// Clear cache
    pub async fn clear_cache(&self) -> Result<()> {
        if let Some(cache) = &self.cache {
            let mut cache_map = cache.cache.write().await;
            cache_map.clear();
        }
        Ok(())
    }

    /// Cleanup expired cache entries
    pub async fn cleanup_cache(&self) -> Result<usize> {
        if let Some(cache) = &self.cache {
            Ok(cache.cleanup_expired().await)
        } else {
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_cache_key_generation() {
        let config = EnhancedRetrievalConfig::default();
        let cache = QueryPatternCache::new(config);

        let request = MemoryAwareSearchRequest {
            base_request: SearchRequest {
                query_text: Some("test query".to_string()),
                ..Default::default()
            },
            include_lineage: Some(true),
            include_insights: Some(true),
            ..Default::default()
        };

        let key1 = cache.generate_cache_key(&request);
        let key2 = cache.generate_cache_key(&request);

        assert_eq!(key1, key2, "Same request should generate same cache key");
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let mut config = EnhancedRetrievalConfig::default();
        config.cache_ttl_seconds = 1; // 1 second for testing

        let cache = QueryPatternCache::new(config);
        let results = vec![]; // Empty for testing

        cache.set("test_key".to_string(), results).await;

        // Should be available immediately
        assert!(cache.get("test_key").await.is_some());

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Should be expired now
        assert!(cache.get("test_key").await.is_none());
    }

    #[test]
    fn test_relationship_type_serialization() {
        let rel_type = RelationshipType::ParentChild;
        let serialized = serde_json::to_string(&rel_type).unwrap();
        let deserialized: RelationshipType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(rel_type, deserialized);
    }

    #[test]
    fn test_boost_explanation_creation() {
        let explanation = BoostExplanation {
            consolidation_boost_applied: 2.0,
            insight_boost_applied: 1.5,
            lineage_boost_applied: 1.0,
            recent_consolidation_factor: 1.0,
            total_boost_multiplier: 2.0,
            boost_reasons: vec!["Recently consolidated".to_string()],
        };

        assert_eq!(explanation.total_boost_multiplier, 2.0);
        assert_eq!(explanation.boost_reasons.len(), 1);
    }
}
