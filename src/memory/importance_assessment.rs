use crate::embedding::EmbeddingService;
use crate::memory::MemoryError;
use anyhow::Result;
use chrono::{DateTime, Utc};
use prometheus::{Counter, Histogram, IntCounter, IntGauge, Registry};
use rand::Rng;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, LinkedList};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

#[derive(Debug, Error)]
pub enum ImportanceAssessmentError {
    #[error("Stage 1 pattern matching failed: {0}")]
    Stage1Failed(String),

    #[error("Stage 2 semantic analysis failed: {0}")]
    Stage2Failed(String),

    #[error("Stage 3 LLM scoring failed: {0}")]
    Stage3Failed(String),

    #[error("Circuit breaker is open: {0}")]
    CircuitBreakerOpen(String),

    #[error("Timeout exceeded: {0}")]
    Timeout(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Memory operation failed: {0}")]
    Memory(#[from] MemoryError),

    #[error("Cache operation failed: {0}")]
    Cache(String),
}

/// Configuration for the importance assessment pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceAssessmentConfig {
    /// Stage 1: Pattern matching configuration
    pub stage1: Stage1Config,

    /// Stage 2: Semantic similarity configuration
    pub stage2: Stage2Config,

    /// Stage 3: LLM scoring configuration
    pub stage3: Stage3Config,

    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,

    /// Performance thresholds
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage1Config {
    /// Confidence threshold to pass to Stage 2 (0.0-1.0)
    pub confidence_threshold: f64,

    /// Pattern library for keyword/phrase matching
    pub pattern_library: Vec<ImportancePattern>,

    /// Maximum processing time in milliseconds
    pub max_processing_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage2Config {
    /// Confidence threshold to pass to Stage 3 (0.0-1.0)
    pub confidence_threshold: f64,

    /// Maximum processing time in milliseconds
    pub max_processing_time_ms: u64,

    /// Cache TTL for embeddings in seconds
    pub embedding_cache_ttl_seconds: u64,

    /// Maximum cache size (number of entries)
    pub embedding_cache_max_size: usize,

    /// Cache eviction threshold (evict when this percentage full)
    pub cache_eviction_threshold: f64,

    /// Similarity threshold for semantic matching
    pub similarity_threshold: f32,

    /// Reference embeddings for importance patterns
    pub reference_embeddings: Vec<ReferenceEmbedding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage3Config {
    /// Maximum processing time in milliseconds
    pub max_processing_time_ms: u64,

    /// LLM endpoint configuration
    pub llm_endpoint: String,

    /// Maximum concurrent LLM requests
    pub max_concurrent_requests: usize,

    /// Prompt template for LLM scoring
    pub prompt_template: String,

    /// Target percentage of evaluations that should reach Stage 3
    pub target_usage_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold before opening the circuit
    pub failure_threshold: usize,

    /// Time window for failure counting in seconds
    pub failure_window_seconds: u64,

    /// Recovery timeout in seconds
    pub recovery_timeout_seconds: u64,

    /// Minimum requests before evaluating failures
    pub minimum_requests: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Stage 1 target time in milliseconds
    pub stage1_target_ms: u64,

    /// Stage 2 target time in milliseconds
    pub stage2_target_ms: u64,

    /// Stage 3 target time in milliseconds
    pub stage3_target_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportancePattern {
    /// Pattern name for metrics and debugging
    pub name: String,

    /// Regular expression pattern
    pub pattern: String,

    /// Weight/importance score for this pattern (0.0-1.0)
    pub weight: f64,

    /// Context words that boost the pattern's importance
    pub context_boosters: Vec<String>,

    /// Category of the pattern (e.g., "command", "preference", "memory")
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceEmbedding {
    /// Name of the reference pattern
    pub name: String,

    /// Pre-computed embedding vector
    pub embedding: Vec<f32>,

    /// Importance weight for this reference
    pub weight: f64,

    /// Category of the reference
    pub category: String,
}

/// Result of the importance assessment pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceAssessmentResult {
    /// Final importance score (0.0-1.0)
    pub importance_score: f64,

    /// Which stage provided the final score
    pub final_stage: AssessmentStage,

    /// Results from each stage
    pub stage_results: Vec<StageResult>,

    /// Total processing time in milliseconds
    pub total_processing_time_ms: u64,

    /// Assessment timestamp
    pub assessed_at: DateTime<Utc>,

    /// Confidence in the assessment (0.0-1.0)
    pub confidence: f64,

    /// Explanation of the assessment
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    /// Which stage this result is from
    pub stage: AssessmentStage,

    /// Score from this stage (0.0-1.0)
    pub score: f64,

    /// Confidence in this stage's result (0.0-1.0)
    pub confidence: f64,

    /// Processing time for this stage in milliseconds
    pub processing_time_ms: u64,

    /// Whether this stage passed its confidence threshold
    pub passed_threshold: bool,

    /// Stage-specific details
    pub details: StageDetails,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssessmentStage {
    Stage1PatternMatching,
    Stage2SemanticSimilarity,
    Stage3LLMScoring,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StageDetails {
    Stage1 {
        matched_patterns: Vec<MatchedPattern>,
        total_patterns_checked: usize,
    },
    Stage2 {
        similarity_scores: Vec<SimilarityScore>,
        cache_hit: bool,
        embedding_generation_time_ms: Option<u64>,
    },
    Stage3 {
        llm_response: String,
        prompt_tokens: Option<usize>,
        completion_tokens: Option<usize>,
        model_used: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchedPattern {
    pub pattern_name: String,
    pub pattern_category: String,
    pub match_text: String,
    pub match_position: usize,
    pub weight: f64,
    pub context_boost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarityScore {
    pub reference_name: String,
    pub reference_category: String,
    pub similarity: f32,
    pub weight: f64,
    pub weighted_score: f64,
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerState {
    Closed,
    Open(DateTime<Utc>),
    HalfOpen,
}

/// Thread-safe circuit breaker for LLM calls using atomic operations
#[derive(Debug)]
struct CircuitBreaker {
    state: RwLock<CircuitBreakerState>,
    config: CircuitBreakerConfig,
    failure_count: AtomicUsize,
    last_failure_time: RwLock<Option<DateTime<Utc>>>,
    request_count: AtomicUsize,
    consecutive_successes: AtomicUsize, // For half-open state management
}

impl CircuitBreaker {
    fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: RwLock::new(CircuitBreakerState::Closed),
            config,
            failure_count: AtomicUsize::new(0),
            last_failure_time: RwLock::new(None),
            request_count: AtomicUsize::new(0),
            consecutive_successes: AtomicUsize::new(0),
        }
    }

    async fn can_execute(&self) -> Result<bool, ImportanceAssessmentError> {
        let state = self.state.read().await;
        match *state {
            CircuitBreakerState::Closed => Ok(true),
            CircuitBreakerState::Open(opened_at) => {
                let now = Utc::now();
                let recovery_time = opened_at
                    + chrono::Duration::seconds(self.config.recovery_timeout_seconds as i64);

                if now >= recovery_time {
                    drop(state);
                    let mut state = self.state.write().await;
                    *state = CircuitBreakerState::HalfOpen;
                    Ok(true)
                } else {
                    Err(ImportanceAssessmentError::CircuitBreakerOpen(format!(
                        "Circuit breaker is open until {}",
                        recovery_time
                    )))
                }
            }
            CircuitBreakerState::HalfOpen => Ok(true),
        }
    }

    async fn record_success(&self) {
        // Increment consecutive successes atomically
        let consecutive_successes = self.consecutive_successes.fetch_add(1, Ordering::SeqCst);

        // Reset failure count atomically
        self.failure_count.store(0, Ordering::SeqCst);

        // In half-open state, require multiple successes before fully closing
        let current_state = {
            let state = self.state.read().await;
            state.clone()
        };

        match current_state {
            CircuitBreakerState::HalfOpen => {
                // Require at least 3 consecutive successes to close
                if consecutive_successes >= 2 {
                    let mut state = self.state.write().await;
                    *state = CircuitBreakerState::Closed;

                    let mut last_failure_time = self.last_failure_time.write().await;
                    *last_failure_time = None;

                    info!(
                        "Circuit breaker closed after {} consecutive successes",
                        consecutive_successes + 1
                    );
                }
            }
            CircuitBreakerState::Open(_) => {
                // This shouldn't happen, but handle gracefully
                warn!(
                    "Received success while circuit breaker is open - state inconsistency detected"
                );
            }
            CircuitBreakerState::Closed => {
                // Already closed, just reset failure time
                let mut last_failure_time = self.last_failure_time.write().await;
                *last_failure_time = None;
            }
        }
    }

    async fn record_failure(&self) {
        let now = Utc::now();

        // Reset consecutive successes on any failure
        self.consecutive_successes.store(0, Ordering::SeqCst);

        // Atomically increment request count
        let request_count = self.request_count.fetch_add(1, Ordering::SeqCst) + 1;

        // Handle failure window and count atomically where possible
        let should_open_circuit = {
            let mut last_failure_time = self.last_failure_time.write().await;

            // Check if we need to reset failure count due to time window
            if let Some(last_failure) = *last_failure_time {
                let window_start =
                    now - chrono::Duration::seconds(self.config.failure_window_seconds as i64);
                if last_failure < window_start {
                    // Reset failure count as we're outside the window
                    self.failure_count.store(0, Ordering::SeqCst);
                }
            }

            // Atomically increment failure count
            let failure_count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
            *last_failure_time = Some(now);

            // Check if we should open the circuit
            request_count >= self.config.minimum_requests
                && failure_count >= self.config.failure_threshold
        };

        if should_open_circuit {
            // Check current state before opening
            let current_state = {
                let state = self.state.read().await;
                state.clone()
            };

            match current_state {
                CircuitBreakerState::Closed | CircuitBreakerState::HalfOpen => {
                    let mut state = self.state.write().await;
                    // Double-check state hasn't changed while acquiring write lock
                    match *state {
                        CircuitBreakerState::Closed | CircuitBreakerState::HalfOpen => {
                            *state = CircuitBreakerState::Open(now);
                            let failure_count = self.failure_count.load(Ordering::SeqCst);
                            warn!(
                                "Circuit breaker opened due to {} failures out of {} requests",
                                failure_count, request_count
                            );
                        }
                        CircuitBreakerState::Open(_) => {
                            // Already open, nothing to do
                        }
                    }
                }
                CircuitBreakerState::Open(_) => {
                    // Already open, just log
                    debug!("Additional failure recorded while circuit breaker is open");
                }
            }
        }
    }
}

/// Cached embedding with TTL and LRU tracking
#[derive(Debug, Clone)]
struct CachedEmbedding {
    embedding: Vec<f32>,
    cached_at: DateTime<Utc>,
    ttl_seconds: u64,
    last_accessed: DateTime<Utc>,
}

/// LRU cache entry for tracking access order
#[derive(Debug, Clone)]
#[allow(dead_code)] // timestamp may be used for future LRU optimizations
struct LRUNode {
    key: String,
    timestamp: DateTime<Utc>,
}

/// Thread-safe LRU cache for embeddings with automatic eviction
#[derive(Debug)]
struct EmbeddingCache {
    cache: RwLock<HashMap<String, CachedEmbedding>>,
    lru_list: RwLock<LinkedList<LRUNode>>,
    current_size: AtomicUsize,
    max_size: usize,
    #[allow(dead_code)] // reserved for future adaptive eviction algorithms
    eviction_threshold: f64,
    eviction_count: AtomicU64,
    memory_pressure_threshold: usize,
}

impl CachedEmbedding {
    fn new(embedding: Vec<f32>, ttl_seconds: u64) -> Self {
        let now = Utc::now();
        Self {
            embedding,
            cached_at: now,
            ttl_seconds,
            last_accessed: now,
        }
    }

    fn is_expired(&self) -> bool {
        let now = Utc::now();
        let expiry = self.cached_at + chrono::Duration::seconds(self.ttl_seconds as i64);
        now >= expiry
    }

    fn touch(&mut self) {
        self.last_accessed = Utc::now();
    }
}

impl EmbeddingCache {
    fn new(max_size: usize, eviction_threshold: f64) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            lru_list: RwLock::new(LinkedList::new()),
            current_size: AtomicUsize::new(0),
            max_size,
            eviction_threshold,
            eviction_count: AtomicU64::new(0),
            memory_pressure_threshold: (max_size as f64 * eviction_threshold) as usize,
        }
    }

    async fn get(&self, key: &str) -> Option<CachedEmbedding> {
        let mut cache = self.cache.write().await;
        let mut lru_list = self.lru_list.write().await;

        if let Some(cached) = cache.get_mut(key) {
            if cached.is_expired() {
                cache.remove(key);
                self.current_size.fetch_sub(1, Ordering::Relaxed);
                // Remove from LRU list manually (retain is unstable)
                *lru_list = lru_list.iter().filter(|node| node.key != key).cloned().collect();
                return None;
            }

            cached.touch();
            // Move to front of LRU list manually (retain is unstable)
            *lru_list = lru_list.iter().filter(|node| node.key != key).cloned().collect();
            lru_list.push_front(LRUNode {
                key: key.to_string(),
                timestamp: cached.last_accessed,
            });

            Some(cached.clone())
        } else {
            None
        }
    }

    async fn insert(
        &self,
        key: String,
        value: CachedEmbedding,
    ) -> Result<(), ImportanceAssessmentError> {
        // Check if we need to evict before inserting
        let current_size = self.current_size.load(Ordering::Relaxed);
        if current_size >= self.memory_pressure_threshold {
            self.evict_lru_entries().await?;
        }

        let mut cache = self.cache.write().await;
        let mut lru_list = self.lru_list.write().await;

        // If key already exists, update it
        if cache.contains_key(&key) {
            *lru_list = lru_list.iter().filter(|node| node.key != key).cloned().collect();
        } else {
            self.current_size.fetch_add(1, Ordering::Relaxed);
        }

        cache.insert(key.clone(), value.clone());
        lru_list.push_front(LRUNode {
            key: key.clone(),
            timestamp: value.last_accessed,
        });

        Ok(())
    }

    async fn evict_lru_entries(&self) -> Result<(), ImportanceAssessmentError> {
        let mut cache = self.cache.write().await;
        let mut lru_list = self.lru_list.write().await;

        let target_size = (self.max_size as f64 * 0.7) as usize; // Evict to 70% capacity
        let current_size = cache.len();

        if current_size <= target_size {
            return Ok(());
        }

        let entries_to_remove = current_size - target_size;
        let mut removed_count = 0;

        // Remove oldest entries
        while removed_count < entries_to_remove && !lru_list.is_empty() {
            if let Some(node) = lru_list.pop_back() {
                cache.remove(&node.key);
                removed_count += 1;
                self.eviction_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        self.current_size.store(cache.len(), Ordering::Relaxed);

        if removed_count > 0 {
            info!(
                "Evicted {} entries from embedding cache due to memory pressure",
                removed_count
            );
        }

        Ok(())
    }

    async fn clear(&self) {
        let mut cache = self.cache.write().await;
        let mut lru_list = self.lru_list.write().await;

        cache.clear();
        lru_list.clear();
        self.current_size.store(0, Ordering::Relaxed);
    }

    fn len(&self) -> usize {
        self.current_size.load(Ordering::Relaxed)
    }

    fn eviction_count(&self) -> u64 {
        self.eviction_count.load(Ordering::Relaxed)
    }

    async fn cleanup_expired(&self) -> Result<usize, ImportanceAssessmentError> {
        let mut cache = self.cache.write().await;
        let mut lru_list = self.lru_list.write().await;

        let mut expired_keys = Vec::new();

        for (key, value) in cache.iter() {
            if value.is_expired() {
                expired_keys.push(key.clone());
            }
        }

        for key in &expired_keys {
            cache.remove(key);
            *lru_list = lru_list.iter().filter(|node| &node.key != key).cloned().collect();
        }

        let removed_count = expired_keys.len();
        self.current_size.store(cache.len(), Ordering::Relaxed);

        if removed_count > 0 {
            debug!(
                "Cleaned up {} expired entries from embedding cache",
                removed_count
            );
        }

        Ok(removed_count)
    }
}

/// Metrics for the importance assessment pipeline
#[derive(Debug)]
pub struct ImportanceAssessmentMetrics {
    // Stage progression counters
    pub stage1_executions: IntCounter,
    pub stage2_executions: IntCounter,
    pub stage3_executions: IntCounter,

    // Stage timing histograms
    pub stage1_duration: Histogram,
    pub stage2_duration: Histogram,
    pub stage3_duration: Histogram,

    // Pipeline completion counters
    pub completed_at_stage1: IntCounter,
    pub completed_at_stage2: IntCounter,
    pub completed_at_stage3: IntCounter,

    // Performance metrics
    pub stage1_threshold_violations: IntCounter,
    pub stage2_threshold_violations: IntCounter,
    pub stage3_threshold_violations: IntCounter,

    // Cache metrics
    pub embedding_cache_hits: IntCounter,
    pub embedding_cache_misses: IntCounter,
    pub embedding_cache_size: IntGauge,

    // Circuit breaker metrics
    pub circuit_breaker_opened: Counter,
    pub circuit_breaker_half_open: Counter,
    pub circuit_breaker_closed: Counter,
    pub llm_call_failures: IntCounter,
    pub llm_call_successes: IntCounter,

    // Quality metrics
    pub assessment_confidence: Histogram,
    pub final_importance_scores: Histogram,
}

impl ImportanceAssessmentMetrics {
    pub fn new(registry: &Registry) -> Result<Self> {
        let stage1_executions = IntCounter::new(
            "importance_assessment_stage1_executions_total",
            "Total number of Stage 1 executions",
        )?;
        registry.register(Box::new(stage1_executions.clone()))?;

        let stage2_executions = IntCounter::new(
            "importance_assessment_stage2_executions_total",
            "Total number of Stage 2 executions",
        )?;
        registry.register(Box::new(stage2_executions.clone()))?;

        let stage3_executions = IntCounter::new(
            "importance_assessment_stage3_executions_total",
            "Total number of Stage 3 executions",
        )?;
        registry.register(Box::new(stage3_executions.clone()))?;

        let stage1_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "importance_assessment_stage1_duration_seconds",
                "Duration of Stage 1 processing",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.02, 0.05, 0.1]),
        )?;
        registry.register(Box::new(stage1_duration.clone()))?;

        let stage2_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "importance_assessment_stage2_duration_seconds",
                "Duration of Stage 2 processing",
            )
            .buckets(vec![0.01, 0.05, 0.1, 0.2, 0.5, 1.0]),
        )?;
        registry.register(Box::new(stage2_duration.clone()))?;

        let stage3_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "importance_assessment_stage3_duration_seconds",
                "Duration of Stage 3 processing",
            )
            .buckets(vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0]),
        )?;
        registry.register(Box::new(stage3_duration.clone()))?;

        let completed_at_stage1 = IntCounter::new(
            "importance_assessment_completed_at_stage1_total",
            "Total assessments completed at Stage 1",
        )?;
        registry.register(Box::new(completed_at_stage1.clone()))?;

        let completed_at_stage2 = IntCounter::new(
            "importance_assessment_completed_at_stage2_total",
            "Total assessments completed at Stage 2",
        )?;
        registry.register(Box::new(completed_at_stage2.clone()))?;

        let completed_at_stage3 = IntCounter::new(
            "importance_assessment_completed_at_stage3_total",
            "Total assessments completed at Stage 3",
        )?;
        registry.register(Box::new(completed_at_stage3.clone()))?;

        let stage1_threshold_violations = IntCounter::new(
            "importance_assessment_stage1_threshold_violations_total",
            "Total Stage 1 performance threshold violations",
        )?;
        registry.register(Box::new(stage1_threshold_violations.clone()))?;

        let stage2_threshold_violations = IntCounter::new(
            "importance_assessment_stage2_threshold_violations_total",
            "Total Stage 2 performance threshold violations",
        )?;
        registry.register(Box::new(stage2_threshold_violations.clone()))?;

        let stage3_threshold_violations = IntCounter::new(
            "importance_assessment_stage3_threshold_violations_total",
            "Total Stage 3 performance threshold violations",
        )?;
        registry.register(Box::new(stage3_threshold_violations.clone()))?;

        let embedding_cache_hits = IntCounter::new(
            "importance_assessment_embedding_cache_hits_total",
            "Total embedding cache hits",
        )?;
        registry.register(Box::new(embedding_cache_hits.clone()))?;

        let embedding_cache_misses = IntCounter::new(
            "importance_assessment_embedding_cache_misses_total",
            "Total embedding cache misses",
        )?;
        registry.register(Box::new(embedding_cache_misses.clone()))?;

        let embedding_cache_size = IntGauge::new(
            "importance_assessment_embedding_cache_size",
            "Current size of embedding cache",
        )?;
        registry.register(Box::new(embedding_cache_size.clone()))?;

        let circuit_breaker_opened = Counter::new(
            "importance_assessment_circuit_breaker_opened_total",
            "Total times circuit breaker opened",
        )?;
        registry.register(Box::new(circuit_breaker_opened.clone()))?;

        let circuit_breaker_half_open = Counter::new(
            "importance_assessment_circuit_breaker_half_open_total",
            "Total times circuit breaker went half-open",
        )?;
        registry.register(Box::new(circuit_breaker_half_open.clone()))?;

        let circuit_breaker_closed = Counter::new(
            "importance_assessment_circuit_breaker_closed_total",
            "Total times circuit breaker closed",
        )?;
        registry.register(Box::new(circuit_breaker_closed.clone()))?;

        let llm_call_failures = IntCounter::new(
            "importance_assessment_llm_call_failures_total",
            "Total LLM call failures",
        )?;
        registry.register(Box::new(llm_call_failures.clone()))?;

        let llm_call_successes = IntCounter::new(
            "importance_assessment_llm_call_successes_total",
            "Total LLM call successes",
        )?;
        registry.register(Box::new(llm_call_successes.clone()))?;

        let assessment_confidence = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "importance_assessment_confidence",
                "Confidence scores of assessments",
            )
            .buckets(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]),
        )?;
        registry.register(Box::new(assessment_confidence.clone()))?;

        let final_importance_scores = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "importance_assessment_final_scores",
                "Final importance scores from assessments",
            )
            .buckets(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]),
        )?;
        registry.register(Box::new(final_importance_scores.clone()))?;

        Ok(Self {
            stage1_executions,
            stage2_executions,
            stage3_executions,
            stage1_duration,
            stage2_duration,
            stage3_duration,
            completed_at_stage1,
            completed_at_stage2,
            completed_at_stage3,
            stage1_threshold_violations,
            stage2_threshold_violations,
            stage3_threshold_violations,
            embedding_cache_hits,
            embedding_cache_misses,
            embedding_cache_size,
            circuit_breaker_opened,
            circuit_breaker_half_open,
            circuit_breaker_closed,
            llm_call_failures,
            llm_call_successes,
            assessment_confidence,
            final_importance_scores,
        })
    }
}

/// Optimized pattern matcher with pre-compiled regex and fast string matching
#[derive(Debug)]
struct OptimizedPatternMatcher {
    // Pre-compiled regex patterns with their metadata
    regex_patterns: Vec<(Regex, ImportancePattern)>,
    // Fast string patterns for simple keyword matching
    keyword_patterns: Vec<(String, ImportancePattern)>,
    // Combined regex for single-pass matching when possible
    #[allow(dead_code)] // reserved for future optimization
    combined_regex: Option<Regex>,
}

impl OptimizedPatternMatcher {
    fn new(patterns: &[ImportancePattern]) -> Result<Self, ImportanceAssessmentError> {
        let mut regex_patterns = Vec::new();
        let mut keyword_patterns = Vec::new();

        for pattern in patterns {
            // Validate regex complexity to prevent ReDoS
            Self::validate_regex_complexity(&pattern.pattern)?;

            // Try to determine if this is a simple keyword pattern
            if Self::is_simple_keyword_pattern(&pattern.pattern) {
                // Extract the keyword from simple patterns like (?i)\b(word)\b
                if let Some(keyword) = Self::extract_keyword(&pattern.pattern) {
                    keyword_patterns.push((keyword.to_lowercase(), pattern.clone()));
                    continue;
                }
            }

            // Compile complex regex patterns
            match Regex::new(&pattern.pattern) {
                Ok(regex) => regex_patterns.push((regex, pattern.clone())),
                Err(e) => {
                    error!(
                        "Failed to compile regex pattern '{}': {}",
                        pattern.pattern, e
                    );
                    return Err(ImportanceAssessmentError::Configuration(format!(
                        "Invalid regex pattern '{}': {}",
                        pattern.pattern, e
                    )));
                }
            }
        }

        Ok(Self {
            regex_patterns,
            keyword_patterns,
            combined_regex: None, // Could be optimized further with a single combined regex
        })
    }

    fn validate_regex_complexity(pattern: &str) -> Result<(), ImportanceAssessmentError> {
        // Check for potentially dangerous regex patterns that could cause ReDoS
        let dangerous_patterns = [
            "(.*)*",
            "(.+)+",
            "([^x]*)*",
            "(a|a)*",
            "(a|a)+",
            "(a*)*",
            "(a+)+",
            "(.{0,10000})*",
            "(.*){10,}",
            "(.+){10,}",
        ];

        for dangerous in &dangerous_patterns {
            if pattern.contains(dangerous) {
                return Err(ImportanceAssessmentError::Configuration(format!(
                    "Regex pattern contains potentially dangerous sequence '{}': {}",
                    dangerous, pattern
                )));
            }
        }

        // Check pattern length
        if pattern.len() > 1000 {
            return Err(ImportanceAssessmentError::Configuration(
                "Regex pattern too long (max 1000 characters)".to_string(),
            ));
        }

        // Check for excessive nested groups
        let open_parens = pattern.chars().filter(|&c| c == '(').count();
        if open_parens > 20 {
            return Err(ImportanceAssessmentError::Configuration(
                "Regex pattern has too many nested groups (max 20)".to_string(),
            ));
        }

        Ok(())
    }

    fn is_simple_keyword_pattern(pattern: &str) -> bool {
        // Match patterns like (?i)\b(word|other)\b or (?i)\b(word)\b
        pattern.starts_with("(?i)")
            && pattern.contains("\\b")
            && !pattern.contains(".*")
            && !pattern.contains(".+")
            && !pattern.contains("\\d")
            && !pattern.contains("\\s")
            && pattern.chars().filter(|&c| c == '(').count() <= 2
    }

    fn extract_keyword(pattern: &str) -> Option<String> {
        // Extract keyword from patterns like (?i)\b(word)\b
        if let Some(start) = pattern.find('(') {
            if let Some(end) = pattern.rfind(')') {
                let keywords = &pattern[start + 1..end];
                // For simple single keyword patterns, return the first keyword
                if !keywords.contains('|') && keywords.chars().all(|c| c.is_alphabetic()) {
                    return Some(keywords.to_string());
                }
            }
        }
        None
    }

    fn find_matches(&self, content: &str, max_matches: usize) -> Vec<MatchedPattern> {
        let mut matches = Vec::new();
        let content_lower = content.to_lowercase();

        // Fast keyword matching first
        for (keyword, pattern) in &self.keyword_patterns {
            if matches.len() >= max_matches {
                break;
            }

            let mut start = 0;
            while let Some(pos) = content_lower[start..].find(keyword) {
                let absolute_pos = start + pos;

                // Check word boundaries manually for fast keyword matching
                let is_word_start = absolute_pos == 0
                    || !content_lower
                        .chars()
                        .nth(absolute_pos - 1)
                        .unwrap_or(' ')
                        .is_alphabetic();
                let is_word_end = absolute_pos + keyword.len() >= content_lower.len()
                    || !content_lower
                        .chars()
                        .nth(absolute_pos + keyword.len())
                        .unwrap_or(' ')
                        .is_alphabetic();

                if is_word_start && is_word_end {
                    let context_boost = self.calculate_context_boost(
                        content,
                        absolute_pos,
                        &pattern.context_boosters,
                    );

                    matches.push(MatchedPattern {
                        pattern_name: pattern.name.clone(),
                        pattern_category: pattern.category.clone(),
                        match_text: keyword.clone(),
                        match_position: absolute_pos,
                        weight: pattern.weight,
                        context_boost,
                    });
                }

                start = absolute_pos + 1;
            }
        }

        // Regex matching for complex patterns
        for (regex, pattern) in &self.regex_patterns {
            if matches.len() >= max_matches {
                break;
            }

            for mat in regex.find_iter(content).take(max_matches - matches.len()) {
                let match_text = mat.as_str().to_string();
                let match_position = mat.start();

                let context_boost = self.calculate_context_boost(
                    content,
                    match_position,
                    &pattern.context_boosters,
                );

                matches.push(MatchedPattern {
                    pattern_name: pattern.name.clone(),
                    pattern_category: pattern.category.clone(),
                    match_text,
                    match_position,
                    weight: pattern.weight,
                    context_boost,
                });
            }
        }

        matches
    }

    fn calculate_context_boost(
        &self,
        content: &str,
        match_position: usize,
        boosters: &[String],
    ) -> f64 {
        let window_size = 100;
        let start = match_position.saturating_sub(window_size);
        let end = (match_position + window_size).min(content.len());
        let context = &content[start..end].to_lowercase();

        let mut boost: f64 = 0.0;
        for booster in boosters {
            if context.contains(&booster.to_lowercase()) {
                boost += 0.1; // 10% boost per context word
            }
        }

        boost.min(0.5_f64) // Maximum 50% boost
    }
}

/// Main importance assessment pipeline
pub struct ImportanceAssessmentPipeline {
    config: ImportanceAssessmentConfig,
    pattern_matcher: OptimizedPatternMatcher,
    embedding_service: Arc<dyn EmbeddingService>,
    embedding_cache: EmbeddingCache,
    circuit_breaker: CircuitBreaker,
    metrics: ImportanceAssessmentMetrics,
    http_client: reqwest::Client,
}

impl ImportanceAssessmentPipeline {
    pub fn new(
        config: ImportanceAssessmentConfig,
        embedding_service: Arc<dyn EmbeddingService>,
        metrics_registry: &Registry,
    ) -> Result<Self> {
        // Initialize optimized pattern matcher
        let pattern_matcher = OptimizedPatternMatcher::new(&config.stage1.pattern_library)?;

        let metrics = ImportanceAssessmentMetrics::new(metrics_registry)?;

        let circuit_breaker = CircuitBreaker::new(config.circuit_breaker.clone());

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.stage3.max_processing_time_ms))
            .build()?;

        let embedding_cache = EmbeddingCache::new(
            config.stage2.embedding_cache_max_size,
            config.stage2.cache_eviction_threshold,
        );

        Ok(Self {
            config,
            pattern_matcher,
            embedding_service,
            embedding_cache,
            circuit_breaker,
            metrics,
            http_client,
        })
    }

    /// Assess the importance of a memory content string
    pub async fn assess_importance(
        &self,
        content: &str,
    ) -> Result<ImportanceAssessmentResult, ImportanceAssessmentError> {
        let assessment_start = Instant::now();
        let mut stage_results = Vec::new();

        info!(
            "Starting importance assessment for content length: {}",
            content.len()
        );

        // Stage 1: Pattern matching
        let stage1_result = self.execute_stage1(content).await?;
        let stage1_passed = stage1_result.passed_threshold;
        stage_results.push(stage1_result.clone());

        if stage1_passed {
            debug!("Stage 1 passed threshold, proceeding to Stage 2");

            // Stage 2: Semantic similarity
            let stage2_result = self.execute_stage2(content).await?;
            let stage2_passed = stage2_result.passed_threshold;
            stage_results.push(stage2_result.clone());

            if stage2_passed {
                debug!("Stage 2 passed threshold, proceeding to Stage 3");

                // Stage 3: LLM scoring
                let stage3_result = self.execute_stage3(content).await?;
                stage_results.push(stage3_result.clone());

                self.metrics.completed_at_stage3.inc();

                let final_score = stage3_result.score;
                let confidence = stage3_result.confidence;

                let result = ImportanceAssessmentResult {
                    importance_score: final_score,
                    final_stage: AssessmentStage::Stage3LLMScoring,
                    stage_results,
                    total_processing_time_ms: assessment_start.elapsed().as_millis() as u64,
                    assessed_at: Utc::now(),
                    confidence,
                    explanation: self.extract_explanation_from_stage3(&stage3_result),
                };

                self.record_final_metrics(&result);
                return Ok(result);
            } else {
                self.metrics.completed_at_stage2.inc();

                let final_score = stage2_result.score;
                let confidence = stage2_result.confidence;

                let result = ImportanceAssessmentResult {
                    importance_score: final_score,
                    final_stage: AssessmentStage::Stage2SemanticSimilarity,
                    stage_results,
                    total_processing_time_ms: assessment_start.elapsed().as_millis() as u64,
                    assessed_at: Utc::now(),
                    confidence,
                    explanation: Some(
                        "Assessment completed at Stage 2 based on semantic similarity".to_string(),
                    ),
                };

                self.record_final_metrics(&result);
                return Ok(result);
            }
        } else {
            self.metrics.completed_at_stage1.inc();

            let final_score = stage1_result.score;
            let confidence = stage1_result.confidence;

            let result = ImportanceAssessmentResult {
                importance_score: final_score,
                final_stage: AssessmentStage::Stage1PatternMatching,
                stage_results,
                total_processing_time_ms: assessment_start.elapsed().as_millis() as u64,
                assessed_at: Utc::now(),
                confidence,
                explanation: Some(
                    "Assessment completed at Stage 1 based on pattern matching".to_string(),
                ),
            };

            self.record_final_metrics(&result);
            return Ok(result);
        }
    }

    async fn execute_stage1(
        &self,
        content: &str,
    ) -> Result<StageResult, ImportanceAssessmentError> {
        let stage_start = Instant::now();
        self.metrics.stage1_executions.inc();

        let timeout_duration = Duration::from_millis(self.config.stage1.max_processing_time_ms);

        let result = timeout(timeout_duration, async {
            // Limit content length for Stage 1 to prevent performance issues
            let content_for_analysis = if content.len() > 10000 {
                warn!(
                    "Content length {} exceeds Stage 1 limit, truncating to 10000 chars",
                    content.len()
                );
                &content[..10000]
            } else {
                content
            };

            // Use optimized pattern matching with limits
            let max_matches = 50; // Limit total matches to prevent runaway processing
            let matched_patterns = self
                .pattern_matcher
                .find_matches(content_for_analysis, max_matches);

            let mut total_score = 0.0;
            let mut max_weight: f64 = 0.0;

            for pattern in &matched_patterns {
                let effective_weight = pattern.weight * (1.0 + pattern.context_boost);
                total_score += effective_weight;
                max_weight = max_weight.max(effective_weight);
            }

            // Normalize score to 0.0-1.0 range
            let normalized_score = if matched_patterns.is_empty() {
                0.0
            } else {
                (total_score / (matched_patterns.len() as f64)).min(1.0)
            };

            // Calculate confidence based on pattern diversity and strength
            let confidence = if matched_patterns.is_empty() {
                0.1 // Low confidence for no matches
            } else {
                let pattern_diversity = matched_patterns
                    .iter()
                    .map(|m| m.pattern_category.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .len() as f64;
                let pattern_count = self.config.stage1.pattern_library.len().max(1) as f64; // Avoid division by zero
                let base_confidence = (pattern_diversity / pattern_count).min(1.0);
                let strength_boost = (max_weight / 1.0_f64).min(0.3); // Max 30% boost from pattern strength
                (base_confidence + strength_boost).min(1.0)
            };

            let passed_threshold = confidence >= self.config.stage1.confidence_threshold;

            StageResult {
                stage: AssessmentStage::Stage1PatternMatching,
                score: normalized_score,
                confidence,
                processing_time_ms: stage_start.elapsed().as_millis() as u64,
                passed_threshold,
                details: StageDetails::Stage1 {
                    matched_patterns,
                    total_patterns_checked: self.config.stage1.pattern_library.len(),
                },
            }
        })
        .await;

        match result {
            Ok(stage_result) => {
                let duration_seconds = stage_start.elapsed().as_secs_f64();
                self.metrics.stage1_duration.observe(duration_seconds);

                // Check performance threshold
                if stage_result.processing_time_ms > self.config.performance.stage1_target_ms {
                    self.metrics.stage1_threshold_violations.inc();
                    warn!(
                        "Stage 1 exceeded target time: {}ms > {}ms",
                        stage_result.processing_time_ms, self.config.performance.stage1_target_ms
                    );
                }

                debug!(
                    "Stage 1 completed in {}ms with score {:.3} and confidence {:.3}",
                    stage_result.processing_time_ms, stage_result.score, stage_result.confidence
                );

                Ok(stage_result)
            }
            Err(_) => {
                self.metrics.stage1_threshold_violations.inc();
                Err(ImportanceAssessmentError::Timeout(format!(
                    "Stage 1 exceeded maximum processing time of {}ms",
                    self.config.stage1.max_processing_time_ms
                )))
            }
        }
    }

    async fn execute_stage2(
        &self,
        content: &str,
    ) -> Result<StageResult, ImportanceAssessmentError> {
        let stage_start = Instant::now();
        self.metrics.stage2_executions.inc();

        let timeout_duration = Duration::from_millis(self.config.stage2.max_processing_time_ms);

        let stage2_result = async {
            // Cleanup expired entries periodically (every 100th request)
            if self.metrics.stage2_executions.get() % 100 == 0 {
                if let Err(e) = self.embedding_cache.cleanup_expired().await {
                    warn!("Failed to cleanup expired cache entries: {}", e);
                }
            }

            // Generate secure hash of content using SHA-256
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            let content_hash = format!("{:x}", hasher.finalize());

            let (content_embedding, cache_hit, embedding_time) =
                if let Some(cached) = self.embedding_cache.get(&content_hash).await {
                    self.metrics.embedding_cache_hits.inc();
                    (cached.embedding, true, None)
                } else {
                    self.metrics.embedding_cache_misses.inc();
                    let embed_start = Instant::now();
                    let embedding = match self.embedding_service.generate_embedding(content).await {
                        Ok(emb) => emb,
                        Err(e) => {
                            return Err(ImportanceAssessmentError::Stage2Failed(format!(
                                "Embedding generation failed: {}",
                                e
                            )))
                        }
                    };
                    let embed_time = embed_start.elapsed().as_millis() as u64;

                    // Cache the new embedding with error handling
                    let cached_embedding = CachedEmbedding::new(
                        embedding.clone(),
                        self.config.stage2.embedding_cache_ttl_seconds,
                    );

                    if let Err(e) = self
                        .embedding_cache
                        .insert(content_hash, cached_embedding)
                        .await
                    {
                        warn!("Failed to cache embedding: {}", e);
                    }

                    self.metrics
                        .embedding_cache_size
                        .set(self.embedding_cache.len() as i64);
                    (embedding, false, Some(embed_time))
                };

            // Calculate similarity scores with reference embeddings
            let mut similarity_scores = Vec::new();
            let mut total_weighted_score = 0.0;
            let mut total_weight = 0.0;

            for reference in &self.config.stage2.reference_embeddings {
                let similarity =
                    self.calculate_cosine_similarity(&content_embedding, &reference.embedding);

                if similarity >= self.config.stage2.similarity_threshold {
                    let weighted_score = similarity as f64 * reference.weight;

                    similarity_scores.push(SimilarityScore {
                        reference_name: reference.name.clone(),
                        reference_category: reference.category.clone(),
                        similarity,
                        weight: reference.weight,
                        weighted_score,
                    });

                    total_weighted_score += weighted_score;
                    total_weight += reference.weight;
                }
            }

            // Normalize score to 0.0-1.0 range
            let normalized_score = if total_weight > 0.0 {
                (total_weighted_score / total_weight).min(1.0)
            } else {
                0.0
            };

            // Calculate confidence based on number of matches and their strength
            let confidence = if similarity_scores.is_empty() {
                0.1 // Low confidence for no semantic matches
            } else {
                let match_ratio = similarity_scores.len() as f64
                    / self.config.stage2.reference_embeddings.len() as f64;
                let avg_similarity = similarity_scores
                    .iter()
                    .map(|s| s.similarity as f64)
                    .sum::<f64>()
                    / similarity_scores.len() as f64;
                (match_ratio * 0.5 + avg_similarity * 0.5).min(1.0)
            };

            let passed_threshold = confidence >= self.config.stage2.confidence_threshold;

            Ok(StageResult {
                stage: AssessmentStage::Stage2SemanticSimilarity,
                score: normalized_score,
                confidence,
                processing_time_ms: stage_start.elapsed().as_millis() as u64,
                passed_threshold,
                details: StageDetails::Stage2 {
                    similarity_scores,
                    cache_hit,
                    embedding_generation_time_ms: embedding_time,
                },
            })
        };

        let result = timeout(timeout_duration, stage2_result).await;

        match result {
            Ok(Ok(stage_result)) => {
                let duration_seconds = stage_start.elapsed().as_secs_f64();
                self.metrics.stage2_duration.observe(duration_seconds);

                // Check performance threshold
                if stage_result.processing_time_ms > self.config.performance.stage2_target_ms {
                    self.metrics.stage2_threshold_violations.inc();
                    warn!(
                        "Stage 2 exceeded target time: {}ms > {}ms",
                        stage_result.processing_time_ms, self.config.performance.stage2_target_ms
                    );
                }

                debug!(
                    "Stage 2 completed in {}ms with score {:.3} and confidence {:.3}",
                    stage_result.processing_time_ms, stage_result.score, stage_result.confidence
                );

                Ok(stage_result)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                self.metrics.stage2_threshold_violations.inc();
                Err(ImportanceAssessmentError::Timeout(format!(
                    "Stage 2 exceeded maximum processing time of {}ms",
                    self.config.stage2.max_processing_time_ms
                )))
            }
        }
    }

    async fn execute_stage3(
        &self,
        content: &str,
    ) -> Result<StageResult, ImportanceAssessmentError> {
        let stage_start = Instant::now();
        self.metrics.stage3_executions.inc();

        // Check circuit breaker
        if !self.circuit_breaker.can_execute().await? {
            return Err(ImportanceAssessmentError::CircuitBreakerOpen(
                "LLM circuit breaker is open".to_string(),
            ));
        }

        let timeout_duration = Duration::from_millis(self.config.stage3.max_processing_time_ms);

        let result = timeout(timeout_duration, async {
            // Prepare LLM prompt with length limits
            let content_preview = if content.len() > 2000 {
                format!(
                    "{}... [truncated from {} chars]",
                    &content[..2000],
                    content.len()
                )
            } else {
                content.to_string()
            };

            let prompt = self
                .config
                .stage3
                .prompt_template
                .replace("{content}", &content_preview)
                .replace("{timestamp}", &Utc::now().to_rfc3339());

            // Make LLM request
            let llm_response = self.call_llm(&prompt).await?;

            // Parse LLM response to extract importance score and confidence
            let (importance_score, confidence) = self.parse_llm_response(&llm_response)?;

            let passed_threshold = true; // Stage 3 is the final stage

            Ok::<StageResult, ImportanceAssessmentError>(StageResult {
                stage: AssessmentStage::Stage3LLMScoring,
                score: importance_score,
                confidence,
                processing_time_ms: stage_start.elapsed().as_millis() as u64,
                passed_threshold,
                details: StageDetails::Stage3 {
                    llm_response,
                    prompt_tokens: Some(prompt.len() / 4), // Rough token estimate
                    completion_tokens: None,               // Would need to be provided by LLM API
                    model_used: "configured-model".to_string(),
                },
            })
        })
        .await;

        match result {
            Ok(Ok(stage_result)) => {
                let duration_seconds = stage_start.elapsed().as_secs_f64();
                self.metrics.stage3_duration.observe(duration_seconds);
                self.metrics.llm_call_successes.inc();
                self.circuit_breaker.record_success().await;

                // Check performance threshold
                if stage_result.processing_time_ms > self.config.performance.stage3_target_ms {
                    self.metrics.stage3_threshold_violations.inc();
                    warn!(
                        "Stage 3 exceeded target time: {}ms > {}ms",
                        stage_result.processing_time_ms, self.config.performance.stage3_target_ms
                    );
                }

                debug!(
                    "Stage 3 completed in {}ms with score {:.3} and confidence {:.3}",
                    stage_result.processing_time_ms, stage_result.score, stage_result.confidence
                );

                Ok(stage_result)
            }
            Ok(Err(e)) => {
                self.metrics.llm_call_failures.inc();
                self.circuit_breaker.record_failure().await;
                Err(e)
            }
            Err(_) => {
                self.metrics.stage3_threshold_violations.inc();
                self.metrics.llm_call_failures.inc();
                self.circuit_breaker.record_failure().await;
                Err(ImportanceAssessmentError::Timeout(format!(
                    "Stage 3 exceeded maximum processing time of {}ms",
                    self.config.stage3.max_processing_time_ms
                )))
            }
        }
    }

    fn calculate_cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }

    async fn call_llm(&self, prompt: &str) -> Result<String, ImportanceAssessmentError> {
        let sanitized_prompt = self.sanitize_llm_prompt(prompt)?;

        // Implement exponential backoff retry logic
        let mut attempts = 0;
        let max_retries = 3;
        let base_delay = Duration::from_millis(100);

        loop {
            match self.attempt_llm_call(&sanitized_prompt).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    attempts += 1;
                    if attempts > max_retries {
                        return Err(e);
                    }

                    // Exponential backoff with jitter
                    let delay = base_delay * (2_u32.pow(attempts - 1));
                    let jitter = Duration::from_millis(rand::thread_rng().gen_range(0..=100));
                    tokio::time::sleep(delay + jitter).await;

                    warn!(
                        "LLM call failed (attempt {}/{}), retrying in {:?}: {}",
                        attempts,
                        max_retries,
                        delay + jitter,
                        e
                    );
                }
            }
        }
    }

    async fn attempt_llm_call(&self, prompt: &str) -> Result<String, ImportanceAssessmentError> {
        // Validate input length to prevent resource exhaustion
        if prompt.len() > 50000 {
            // 50KB limit
            return Err(ImportanceAssessmentError::Stage3Failed(
                "Prompt exceeds maximum length".to_string(),
            ));
        }

        let request_body = serde_json::json!({
            "prompt": prompt,
            "max_tokens": 150,
            "temperature": 0.1,
            "stop": ["\n\n", "---"],
            "model": "text-davinci-003" // Default model
        });

        let response = self
            .http_client
            .post(&self.config.stage3.llm_endpoint)
            .header("Content-Type", "application/json")
            .header("User-Agent", "codex-memory/0.1.0")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ImportanceAssessmentError::Timeout(format!("LLM request timed out: {}", e))
                } else if e.is_connect() {
                    ImportanceAssessmentError::Stage3Failed(format!(
                        "Failed to connect to LLM service: {}",
                        e
                    ))
                } else {
                    ImportanceAssessmentError::Stage3Failed(format!("LLM request failed: {}", e))
                }
            })?;

        let status = response.status();
        let response_text = response.text().await.map_err(|e| {
            ImportanceAssessmentError::Stage3Failed(format!("Failed to read response body: {}", e))
        })?;

        if !status.is_success() {
            // Handle different HTTP error codes
            let error_msg = match status.as_u16() {
                400 => format!("Bad request to LLM service: {}", response_text),
                401 => "Unauthorized: Invalid API key or credentials".to_string(),
                403 => "Forbidden: Insufficient permissions".to_string(),
                429 => "Rate limit exceeded, will retry".to_string(),
                500..=599 => format!("LLM service error ({}): {}", status, response_text),
                _ => format!("LLM service returned status {}: {}", status, response_text),
            };

            return Err(ImportanceAssessmentError::Stage3Failed(error_msg));
        }

        // Parse response with proper error handling
        let response_json: serde_json::Value =
            serde_json::from_str(&response_text).map_err(|e| {
                ImportanceAssessmentError::Stage3Failed(format!(
                    "Failed to parse LLM response as JSON: {}",
                    e
                ))
            })?;

        // Extract response text with multiple fallback paths
        let response_content = response_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|choice| {
                // Try different response formats (OpenAI, local models, etc.)
                choice
                    .get("text")
                    .or_else(|| choice.get("message").and_then(|m| m.get("content")))
                    .or_else(|| choice.get("generated_text"))
            })
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ImportanceAssessmentError::Stage3Failed(format!(
                    "Invalid LLM response format. Expected 'choices[0].text' or similar. Got: {}",
                    serde_json::to_string_pretty(&response_json)
                        .unwrap_or_else(|_| "invalid JSON".to_string())
                ))
            })?
            .trim()
            .to_string();

        // Validate response length
        if response_content.is_empty() {
            return Err(ImportanceAssessmentError::Stage3Failed(
                "LLM returned empty response".to_string(),
            ));
        }

        if response_content.len() > 10000 {
            // 10KB response limit
            warn!("LLM response was truncated due to excessive length");
            return Ok(response_content[..10000].to_string());
        }

        Ok(response_content)
    }

    fn sanitize_llm_prompt(&self, prompt: &str) -> Result<String, ImportanceAssessmentError> {
        // Basic prompt injection protection
        let dangerous_patterns = [
            "ignore previous instructions",
            "ignore all previous",
            "disregard previous",
            "forget previous",
            "new instructions:",
            "system:",
            "assistant:",
            "user:",
            "<|endoftext|>",
            "###",
            "---\n",
        ];

        let lower_prompt = prompt.to_lowercase();
        for pattern in &dangerous_patterns {
            if lower_prompt.contains(pattern) {
                warn!("Potential prompt injection detected: {}", pattern);
                return Err(ImportanceAssessmentError::Stage3Failed(
                    "Prompt contains potentially malicious content".to_string(),
                ));
            }
        }

        // Remove or escape potentially dangerous characters
        let sanitized = prompt
            .replace('\0', "") // Remove null bytes
            .replace("\x1b", "") // Remove escape sequences
            .chars()
            .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
            .collect::<String>();

        // Validate final length
        if sanitized.len() > 10000 {
            return Err(ImportanceAssessmentError::Stage3Failed(
                "Prompt too long after sanitization".to_string(),
            ));
        }

        Ok(sanitized)
    }

    fn parse_llm_response(&self, response: &str) -> Result<(f64, f64), ImportanceAssessmentError> {
        // Parse LLM response to extract importance score and confidence
        // This is a simplified parser - in practice, you'd want more robust parsing

        let lines: Vec<&str> = response.lines().collect();
        let mut importance_score = 0.5; // Default
        let mut confidence = 0.7; // Default

        for line in lines {
            let line = line.trim().to_lowercase();

            // Look for importance score
            if line.contains("importance:") || line.contains("score:") {
                if let Some(score_str) = line.split(':').nth(1) {
                    if let Ok(score) = score_str.trim().parse::<f64>() {
                        importance_score = score.clamp(0.0, 1.0);
                    }
                }
            }

            // Look for confidence
            if line.contains("confidence:") {
                if let Some(conf_str) = line.split(':').nth(1) {
                    if let Ok(conf) = conf_str.trim().parse::<f64>() {
                        confidence = conf.clamp(0.0, 1.0);
                    }
                }
            }
        }

        Ok((importance_score, confidence))
    }

    fn extract_explanation_from_stage3(&self, stage_result: &StageResult) -> Option<String> {
        if let StageDetails::Stage3 { llm_response, .. } = &stage_result.details {
            Some(llm_response.clone())
        } else {
            None
        }
    }

    fn record_final_metrics(&self, result: &ImportanceAssessmentResult) {
        self.metrics
            .assessment_confidence
            .observe(result.confidence);
        self.metrics
            .final_importance_scores
            .observe(result.importance_score);

        info!(
            "Importance assessment completed: score={:.3}, confidence={:.3}, stage={:?}, time={}ms",
            result.importance_score,
            result.confidence,
            result.final_stage,
            result.total_processing_time_ms
        );
    }

    /// Get current pipeline statistics
    pub async fn get_statistics(&self) -> PipelineStatistics {
        let cache_size = self.embedding_cache.len();
        let eviction_count = self.embedding_cache.eviction_count();

        PipelineStatistics {
            cache_size,
            stage1_executions: self.metrics.stage1_executions.get(),
            stage2_executions: self.metrics.stage2_executions.get(),
            stage3_executions: self.metrics.stage3_executions.get(),
            completed_at_stage1: self.metrics.completed_at_stage1.get(),
            completed_at_stage2: self.metrics.completed_at_stage2.get(),
            completed_at_stage3: self.metrics.completed_at_stage3.get(),
            cache_hits: self.metrics.embedding_cache_hits.get(),
            cache_misses: self.metrics.embedding_cache_misses.get(),
            cache_evictions: eviction_count,
            circuit_breaker_state: format!("{:?}", *self.circuit_breaker.state.read().await),
            llm_success_rate: {
                let successes = self.metrics.llm_call_successes.get() as f64;
                let failures = self.metrics.llm_call_failures.get() as f64;
                let total = successes + failures;
                if total > 0.0 {
                    successes / total
                } else {
                    1.0
                }
            },
        }
    }

    /// Clear the embedding cache
    pub async fn clear_cache(&self) {
        self.embedding_cache.clear().await;
        self.metrics.embedding_cache_size.set(0);
        info!("Embedding cache cleared");
    }

    /// Get cache hit ratio
    pub fn get_cache_hit_ratio(&self) -> f64 {
        let hits = self.metrics.embedding_cache_hits.get() as f64;
        let misses = self.metrics.embedding_cache_misses.get() as f64;
        let total = hits + misses;
        if total > 0.0 {
            hits / total
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStatistics {
    pub cache_size: usize,
    pub stage1_executions: u64,
    pub stage2_executions: u64,
    pub stage3_executions: u64,
    pub completed_at_stage1: u64,
    pub completed_at_stage2: u64,
    pub completed_at_stage3: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_evictions: u64,
    pub circuit_breaker_state: String,
    pub llm_success_rate: f64,
}

impl Default for ImportanceAssessmentConfig {
    fn default() -> Self {
        Self {
            stage1: Stage1Config {
                confidence_threshold: 0.6,
                pattern_library: vec![
                    ImportancePattern {
                        name: "remember_command".to_string(),
                        pattern: r"(?i)\b(remember|recall|don't forget)\b".to_string(),
                        weight: 0.8,
                        context_boosters: vec!["important".to_string(), "critical".to_string()],
                        category: "memory".to_string(),
                    },
                    ImportancePattern {
                        name: "preference_statement".to_string(),
                        pattern: r"(?i)\b(prefer|like|want|choose)\b".to_string(),
                        weight: 0.7,
                        context_boosters: vec!["always".to_string(), "usually".to_string()],
                        category: "preference".to_string(),
                    },
                    ImportancePattern {
                        name: "decision_making".to_string(),
                        pattern: r"(?i)\b(decide|decision|choose|select)\b".to_string(),
                        weight: 0.75,
                        context_boosters: vec!["final".to_string(), "official".to_string()],
                        category: "decision".to_string(),
                    },
                    ImportancePattern {
                        name: "correction".to_string(),
                        pattern: r"(?i)\b(correct|fix|wrong|mistake|error)\b".to_string(),
                        weight: 0.6,
                        context_boosters: vec!["actually".to_string(), "should".to_string()],
                        category: "correction".to_string(),
                    },
                    ImportancePattern {
                        name: "importance_marker".to_string(),
                        pattern: r"(?i)\b(important|critical|crucial|vital|essential)\b".to_string(),
                        weight: 0.9,
                        context_boosters: vec!["very".to_string(), "extremely".to_string()],
                        category: "importance".to_string(),
                    },
                ],
                max_processing_time_ms: 10,
            },
            stage2: Stage2Config {
                confidence_threshold: 0.7,
                max_processing_time_ms: 100,
                embedding_cache_ttl_seconds: 3600, // 1 hour
                embedding_cache_max_size: 10000, // Maximum 10k cached embeddings
                cache_eviction_threshold: 0.8, // Start evicting at 80% capacity
                similarity_threshold: 0.7,
                reference_embeddings: vec![], // Would be populated with pre-computed embeddings
            },
            stage3: Stage3Config {
                max_processing_time_ms: 1000,
                llm_endpoint: "http://localhost:8080/generate".to_string(),
                max_concurrent_requests: 5,
                prompt_template: "Assess the importance of this content on a scale of 0.0 to 1.0. Consider context, user intent, and actionability.\n\nContent: {content}\n\nProvide your assessment as:\nImportance: [score]\nConfidence: [confidence]\nReasoning: [explanation]".to_string(),
                target_usage_percentage: 20.0,
            },
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 5,
                failure_window_seconds: 60,
                recovery_timeout_seconds: 30,
                minimum_requests: 3,
            },
            performance: PerformanceConfig {
                stage1_target_ms: 10,
                stage2_target_ms: 100,
                stage3_target_ms: 1000,
            },
        }
    }
}
