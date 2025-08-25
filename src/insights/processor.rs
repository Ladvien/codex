//! Insights Processor - Core orchestration engine for Codex Dreams
//!
//! This module implements the central orchestrator that coordinates the entire
//! insights generation pipeline: fetch → generate → store. It manages batch
//! and real-time processing, circuit breakers, error handling, and statistics.
//!
//! # Architecture
//! - Orchestrates MemoryRepository, OllamaClient, and InsightStorage
//! - Implements circuit breaker pattern for resilience
//! - Tracks processing statistics and health metrics
//! - Supports both batch and real-time processing modes
//! - Provides comprehensive error handling and recovery

#[cfg(feature = "codex-dreams")]
use super::models::{HealthStatus, Insight, ProcessingReport};
#[cfg(feature = "codex-dreams")]
use super::ollama_client::{OllamaClient, OllamaClientError};
#[cfg(feature = "codex-dreams")]
use super::storage::InsightStorage;
#[cfg(feature = "codex-dreams")]
use crate::memory::error::{MemoryError, Result};
#[cfg(feature = "codex-dreams")]
use crate::memory::{Memory, MemoryRepository, MemoryStatus};

#[cfg(feature = "codex-dreams")]
use chrono::{DateTime, Duration, Utc};
#[cfg(feature = "codex-dreams")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "codex-dreams")]
use std::collections::HashMap;
#[cfg(feature = "codex-dreams")]
use std::sync::Arc;
#[cfg(feature = "codex-dreams")]
use tokio::sync::Mutex;
#[cfg(feature = "codex-dreams")]
use tracing::{debug, error, info, instrument, warn};
#[cfg(feature = "codex-dreams")]
use uuid::Uuid;

/// Configuration for the insights processor
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorConfig {
    /// Number of memories to process per batch
    pub batch_size: usize,
    /// Maximum number of retry attempts for failed operations
    pub max_retries: u32,
    /// Timeout in seconds for individual operations (recommended: 900s+ for 20B models)
    pub timeout_seconds: u64,
    /// Circuit breaker: failures before opening circuit
    pub circuit_breaker_threshold: u32,
    /// Circuit breaker: time in seconds before attempting recovery
    pub circuit_breaker_recovery_timeout: u64,
    /// Minimum confidence score for storing insights
    pub min_confidence_threshold: f32,
    /// Maximum number of insights to generate per memory batch
    pub max_insights_per_batch: usize,
}

/// Processing result containing generated insights and statistics
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    /// Successfully generated insights
    pub insights: Vec<Insight>,
    /// Processing report with metrics
    pub report: ProcessingReport,
    /// Any warnings encountered during processing
    pub warnings: Vec<String>,
}

/// Circuit breaker states
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,   // Normal operation
    Open,     // Failing, reject requests
    HalfOpen, // Testing recovery
}

/// Circuit breaker implementation for fault tolerance
#[cfg(feature = "codex-dreams")]
#[derive(Debug)]
struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    threshold: u32,
    last_failure_time: Option<DateTime<Utc>>,
    recovery_timeout: Duration,
}

/// Processing statistics tracked by the processor
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingStats {
    /// Total memories processed since startup
    pub total_memories_processed: u64,
    /// Total insights generated since startup
    pub total_insights_generated: u64,
    /// Average processing time per memory (milliseconds)
    pub avg_processing_time_ms: f64,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f32,
    /// Last processing timestamp
    pub last_processed_at: Option<DateTime<Utc>>,
    /// Current circuit breaker state
    pub circuit_breaker_state: String,
    /// Number of circuit breaker trips
    pub circuit_breaker_trips: u32,
    /// Processing errors by type
    pub error_counts: HashMap<String, u64>,
}

/// Core insights processor orchestrating the entire pipeline
#[cfg(feature = "codex-dreams")]
pub struct InsightsProcessor {
    /// Memory repository for fetching source data
    memory_repository: Arc<MemoryRepository>,
    /// Ollama client for insight generation
    ollama_client: Arc<OllamaClient>,
    /// Storage layer for insights
    insight_storage: Arc<InsightStorage>,
    /// Circuit breaker for fault tolerance
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    /// Processing configuration
    config: ProcessorConfig,
    /// Runtime statistics
    stats: Arc<Mutex<ProcessingStats>>,
}

#[cfg(feature = "codex-dreams")]
impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            max_retries: 3,
            timeout_seconds: 900, // 15 minutes for large model processing
            circuit_breaker_threshold: 20, // High tolerance for cold starts and model loading
            circuit_breaker_recovery_timeout: 60, // Reasonable recovery time for Ollama cold starts
            min_confidence_threshold: 0.3,
            max_insights_per_batch: 50,
        }
    }
}

#[cfg(feature = "codex-dreams")]
impl CircuitBreaker {
    fn new(threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            threshold,
            last_failure_time: None,
            recovery_timeout,
        }
    }

    fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true,
            CircuitState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    if Utc::now() - last_failure > self.recovery_timeout {
                        info!("Circuit breaker transitioning to half-open for recovery test");
                        self.state = CircuitState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    // No last failure time, transition to half-open
                    self.state = CircuitState::HalfOpen;
                    true
                }
            }
        }
    }

    fn record_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                info!("Circuit breaker recovery successful, closing circuit");
                self.state = CircuitState::Closed;
                self.failure_count = 0;
                self.last_failure_time = None;
            }
            CircuitState::Closed => {
                // Already closed, no action needed
            }
            CircuitState::Open => {
                // Should not happen, but handle gracefully
                warn!("Unexpected success while circuit is open");
            }
        }
    }

    fn record_failure(&mut self) -> bool {
        self.failure_count += 1;
        self.last_failure_time = Some(Utc::now());

        match self.state {
            CircuitState::Closed => {
                if self.failure_count >= self.threshold {
                    error!("Circuit breaker threshold exceeded, opening circuit");
                    self.state = CircuitState::Open;
                    true // Circuit just opened
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                error!("Circuit breaker recovery failed, re-opening circuit");
                self.state = CircuitState::Open;
                true // Circuit re-opened
            }
            CircuitState::Open => false, // Already open
        }
    }
}

#[cfg(feature = "codex-dreams")]
impl Default for ProcessingStats {
    fn default() -> Self {
        Self {
            total_memories_processed: 0,
            total_insights_generated: 0,
            avg_processing_time_ms: 0.0,
            success_rate: 1.0,
            last_processed_at: None,
            circuit_breaker_state: "Closed".to_string(),
            circuit_breaker_trips: 0,
            error_counts: HashMap::new(),
        }
    }
}

#[cfg(feature = "codex-dreams")]
impl InsightsProcessor {
    /// Create a new insights processor with the given components
    pub fn new(
        memory_repository: Arc<MemoryRepository>,
        ollama_client: Arc<OllamaClient>,
        insight_storage: Arc<InsightStorage>,
        config: ProcessorConfig,
    ) -> Self {
        let circuit_breaker = Arc::new(Mutex::new(CircuitBreaker::new(
            config.circuit_breaker_threshold,
            Duration::seconds(config.circuit_breaker_recovery_timeout as i64),
        )));

        Self {
            memory_repository,
            ollama_client,
            insight_storage,
            circuit_breaker,
            config,
            stats: Arc::new(Mutex::new(ProcessingStats::default())),
        }
    }

    /// Process a batch of memories to generate insights
    #[instrument(skip(self), fields(batch_size = memory_ids.len()))]
    pub async fn process_batch(&self, memory_ids: Vec<Uuid>) -> Result<ProcessingResult> {
        let start_time = Utc::now();
        info!("Starting batch processing of {} memories", memory_ids.len());

        // Check circuit breaker
        {
            let mut circuit_breaker = self.circuit_breaker.lock().await;
            if !circuit_breaker.can_execute() {
                warn!("Circuit breaker is open, rejecting batch processing request");
                return Err(MemoryError::ServiceUnavailable(
                    "Circuit breaker is open".to_string(),
                ));
            }
        }

        let mut all_insights = Vec::new();
        let mut total_processed = 0;
        let mut total_errors = 0;
        let mut warnings = Vec::new();
        let mut errors_by_type: HashMap<String, u64> = HashMap::new();

        // Process in configured batch sizes
        for chunk in memory_ids.chunks(self.config.batch_size) {
            match self.process_memory_chunk(chunk).await {
                Ok(mut chunk_insights) => {
                    total_processed += chunk.len();
                    let insight_count = chunk_insights.len();
                    all_insights.append(&mut chunk_insights);

                    debug!(
                        "Processed chunk of {} memories, generated {} insights",
                        chunk.len(),
                        insight_count
                    );

                    // Record success in circuit breaker
                    {
                        let mut circuit_breaker = self.circuit_breaker.lock().await;
                        circuit_breaker.record_success();
                    }
                }
                Err(e) => {
                    total_errors += chunk.len();
                    let error_type = match &e {
                        MemoryError::Database(_) => "database_error",
                        MemoryError::NotFound { .. } => "not_found",
                        MemoryError::StorageExhausted { .. } => "storage_exhausted",
                        _ => "other_error",
                    };

                    *errors_by_type.entry(error_type.to_string()).or_insert(0) += 1;

                    warnings.push(format!("Failed to process chunk: {}", e));
                    error!("Chunk processing failed: {}", e);

                    // Record failure in circuit breaker
                    {
                        let mut circuit_breaker = self.circuit_breaker.lock().await;
                        if circuit_breaker.record_failure() {
                            let mut stats = self.stats.lock().await;
                            stats.circuit_breaker_trips += 1;
                        }
                    }

                    // Continue with next chunk unless circuit breaker opens
                    {
                        let circuit_breaker = self.circuit_breaker.lock().await;
                        if matches!(circuit_breaker.state, CircuitState::Open) {
                            warn!("Circuit breaker opened, stopping batch processing");
                            break;
                        }
                    }
                }
            }
        }

        let duration = Utc::now() - start_time;
        let duration_seconds = duration.num_milliseconds() as f64 / 1000.0;

        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.total_memories_processed += total_processed as u64;
            stats.total_insights_generated += all_insights.len() as u64;
            stats.last_processed_at = Some(Utc::now());

            // Update success rate using exponential moving average
            let current_success_rate = if memory_ids.len() > 0 {
                (total_processed as f32) / (memory_ids.len() as f32)
            } else {
                1.0
            };
            stats.success_rate = 0.9 * stats.success_rate + 0.1 * current_success_rate;

            // Update average processing time
            let batch_time_ms = duration.num_milliseconds() as f64;
            if stats.total_memories_processed > 0 {
                stats.avg_processing_time_ms = 0.9 * stats.avg_processing_time_ms
                    + 0.1 * (batch_time_ms / total_processed as f64);
            } else {
                stats.avg_processing_time_ms = batch_time_ms / total_processed as f64;
            }

            // Update error counts
            for (error_type, count) in errors_by_type.iter() {
                *stats.error_counts.entry(error_type.clone()).or_insert(0) += count;
            }

            // Update circuit breaker state
            let circuit_breaker = self.circuit_breaker.lock().await;
            stats.circuit_breaker_state = match circuit_breaker.state {
                CircuitState::Closed => "Closed".to_string(),
                CircuitState::Open => "Open".to_string(),
                CircuitState::HalfOpen => "HalfOpen".to_string(),
            };
        }

        let report = ProcessingReport {
            memories_processed: total_processed,
            insights_generated: all_insights.len(),
            duration_seconds,
            errors: warnings.clone(),
            success_rate: if memory_ids.len() > 0 {
                (total_processed as f32) / (memory_ids.len() as f32)
            } else {
                1.0
            },
        };

        info!(
            "Batch processing completed: {} insights from {} memories in {:.2}s",
            all_insights.len(),
            total_processed,
            duration_seconds
        );

        Ok(ProcessingResult {
            insights: all_insights,
            report,
            warnings,
        })
    }

    /// Process a single memory in real-time mode
    #[instrument(skip(self))]
    pub async fn process_realtime(&self, memory_id: Uuid) -> Result<Vec<Insight>> {
        info!("Processing memory {} in real-time mode", memory_id);

        // Check circuit breaker
        {
            let mut circuit_breaker = self.circuit_breaker.lock().await;
            if !circuit_breaker.can_execute() {
                warn!("Circuit breaker is open, rejecting real-time processing request");
                return Err(MemoryError::ServiceUnavailable(
                    "Circuit breaker is open".to_string(),
                ));
            }
        }

        match self.process_memory_chunk(&[memory_id]).await {
            Ok(insights) => {
                // Record success in circuit breaker
                {
                    let mut circuit_breaker = self.circuit_breaker.lock().await;
                    circuit_breaker.record_success();
                }

                // Update stats
                {
                    let mut stats = self.stats.lock().await;
                    stats.total_memories_processed += 1;
                    stats.total_insights_generated += insights.len() as u64;
                    stats.last_processed_at = Some(Utc::now());
                }

                info!("Real-time processing generated {} insights", insights.len());
                Ok(insights)
            }
            Err(e) => {
                // Record failure in circuit breaker
                {
                    let mut circuit_breaker = self.circuit_breaker.lock().await;
                    if circuit_breaker.record_failure() {
                        let mut stats = self.stats.lock().await;
                        stats.circuit_breaker_trips += 1;
                    }
                }

                error!("Real-time processing failed: {}", e);
                Err(e)
            }
        }
    }

    /// Get current health status of the processor and its components
    pub async fn health_check(&self) -> HealthStatus {
        let mut components = HashMap::new();

        // Check memory repository
        components.insert(
            "memory_repository".to_string(),
            self.memory_repository.health_check().await.is_ok(),
        );

        // Check ollama client
        components.insert(
            "ollama_client".to_string(),
            self.ollama_client.health_check().await,
        );

        // Check insight storage - for now assume healthy if we can create the component
        // In a full implementation, this would check database connectivity
        components.insert("insight_storage".to_string(), true);

        // Check circuit breaker
        let circuit_healthy = {
            let circuit_breaker = self.circuit_breaker.lock().await;
            !matches!(circuit_breaker.state, CircuitState::Open)
        };
        components.insert("circuit_breaker".to_string(), circuit_healthy);

        let overall_healthy = components.values().all(|&status| status);

        let stats = self.stats.lock().await;
        HealthStatus {
            healthy: overall_healthy,
            components,
            last_processed: stats.last_processed_at,
            next_scheduled: None, // This would be set by the scheduler in Story 9
        }
    }

    /// Get current processing statistics
    pub async fn get_stats(&self) -> ProcessingStats {
        self.stats.lock().await.clone()
    }

    /// Internal method to process a chunk of memories
    #[instrument(skip(self))]
    async fn process_memory_chunk(&self, memory_ids: &[Uuid]) -> Result<Vec<Insight>> {
        debug!("Processing chunk of {} memories", memory_ids.len());

        // Fetch memories from repository
        let memories = self.fetch_memories(memory_ids).await?;
        if memories.is_empty() {
            debug!("No valid memories found for processing");
            return Ok(Vec::new());
        }

        // Generate insights using Ollama
        let insight_requests = self.generate_insight_requests(&memories).await?;
        if insight_requests.is_empty() {
            debug!("No insights generated for memories");
            return Ok(Vec::new());
        }

        // Filter by confidence threshold
        let original_count = insight_requests.len();
        let filtered_insights: Vec<_> = insight_requests
            .into_iter()
            .filter(|insight| insight.confidence_score >= self.config.min_confidence_threshold)
            .collect();

        if filtered_insights.len() < original_count {
            debug!(
                "Filtered {} insights below confidence threshold {}",
                original_count - filtered_insights.len(),
                self.config.min_confidence_threshold
            );
        }

        // Store insights
        let mut stored_insights = Vec::new();
        for insight in filtered_insights {
            match self.insight_storage.store(insight.clone()).await {
                Ok(_insight_id) => {
                    stored_insights.push(insight);
                }
                Err(e) => {
                    error!("Failed to store insight: {}", e);
                    // Continue with other insights rather than failing the whole batch
                }
            }
        }

        debug!("Successfully stored {} insights", stored_insights.len());
        Ok(stored_insights)
    }

    /// Fetch memories from the repository with error handling
    async fn fetch_memories(&self, memory_ids: &[Uuid]) -> Result<Vec<Memory>> {
        let mut memories = Vec::new();

        for &memory_id in memory_ids {
            match self.memory_repository.get_memory_by_id(memory_id).await {
                Ok(memory) => {
                    // Only process active memories
                    if matches!(memory.status, MemoryStatus::Active) {
                        memories.push(memory);
                    } else {
                        debug!("Skipping inactive memory {}", memory_id);
                    }
                }
                Err(e) => {
                    error!("Failed to fetch memory {}: {}", memory_id, e);
                    // Continue with other memories
                }
            }
        }

        Ok(memories)
    }

    /// Generate insight requests using Ollama client
    async fn generate_insight_requests(&self, memories: &[Memory]) -> Result<Vec<Insight>> {
        let mut insights = Vec::new();

        for memory in memories {
            match self
                .ollama_client
                .generate_insights_batch(vec![memory.clone()])
                .await
            {
                Ok(insight_response) => {
                    // Convert InsightResponse to Insight
                    let insight = self
                        .convert_insight_response_to_insight(insight_response, memory)
                        .await;
                    insights.push(insight);
                }
                Err(OllamaClientError::Timeout) => {
                    warn!("Timeout generating insights for memory {} (20B models require longer processing time)", memory.id);
                    // Continue with other memories - don't fail the entire batch
                }
                Err(OllamaClientError::ServiceUnavailable(msg)) => {
                    error!("Ollama service unavailable: {}", msg);
                    return Err(MemoryError::ServiceUnavailable(msg));
                }
                Err(e) => {
                    error!(
                        "Failed to generate insights for memory {}: {}",
                        memory.id, e
                    );
                    return Err(MemoryError::OllamaError(e.to_string()));
                }
            }
        }

        // Limit insights per batch to prevent overwhelming the system
        if insights.len() > self.config.max_insights_per_batch {
            insights.truncate(self.config.max_insights_per_batch);
            warn!(
                "Truncated insights to maximum batch size of {}",
                self.config.max_insights_per_batch
            );
        }

        Ok(insights)
    }

    /// Convert InsightResponse from Ollama to Insight for storage
    async fn convert_insight_response_to_insight(
        &self,
        response: super::ollama_client::InsightResponse,
        source_memory: &Memory,
    ) -> Insight {
        Insight {
            id: response.id,
            content: response.content,
            insight_type: response.insight_type,
            confidence_score: response.confidence_score as f32,
            source_memory_ids: response.source_memory_ids,
            metadata: serde_json::json!({
                "generated_at": Utc::now(),
                "source_tier": source_memory.tier,
                "processing_version": "1.0"
            }),
            tags: Vec::new(),            // Could be extracted from content analysis
            tier: "working".to_string(), // Start in working tier
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            feedback_score: 0.0,
            version: 1,
            previous_version: None,
            previous_version_id: None,
            embedding: None, // Will be generated during storage
        }
    }
}

#[cfg(all(feature = "codex-dreams", test))]
mod tests {
    use super::*;
    // use crate::memory::test_utils::create_test_memory;
    use tokio;

    // Mock implementations for testing would go here
    // This would typically be in a separate test module

    #[tokio::test]
    async fn test_circuit_breaker_basic_operation() {
        let mut breaker = CircuitBreaker::new(3, Duration::seconds(60));

        // Should be closed initially
        assert!(breaker.can_execute());
        assert_eq!(breaker.state, CircuitState::Closed);

        // Record failures
        assert!(!breaker.record_failure()); // 1st failure
        assert!(!breaker.record_failure()); // 2nd failure
        assert!(breaker.record_failure()); // 3rd failure - should open circuit

        assert_eq!(breaker.state, CircuitState::Open);
        assert!(!breaker.can_execute());
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let mut breaker = CircuitBreaker::new(2, Duration::seconds(1));

        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state, CircuitState::Open);

        // Wait for recovery period
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Should transition to half-open
        assert!(breaker.can_execute());
        assert_eq!(breaker.state, CircuitState::HalfOpen);

        // Success should close it
        breaker.record_success();
        assert_eq!(breaker.state, CircuitState::Closed);
    }

    #[test]
    fn test_processor_config_default() {
        let config = ProcessorConfig::default();
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.timeout_seconds, 300);
        assert_eq!(config.circuit_breaker_threshold, 5);
        assert_eq!(config.min_confidence_threshold, 0.3);
    }

    #[test]
    fn test_processing_stats_default() {
        let stats = ProcessingStats::default();
        assert_eq!(stats.total_memories_processed, 0);
        assert_eq!(stats.total_insights_generated, 0);
        assert_eq!(stats.avg_processing_time_ms, 0.0);
        assert_eq!(stats.success_rate, 1.0);
        assert_eq!(stats.circuit_breaker_state, "Closed");
        assert_eq!(stats.circuit_breaker_trips, 0);
    }
}
