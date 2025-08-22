//! Forgetting and Memory Cleanup Job
//!
//! This module implements an automatic cleanup job for processing memory decay
//! and forgetting based on the Ebbinghaus forgetting curve. It extends the existing
//! consolidation job infrastructure with forgetting-specific logic.
//!
//! Key features:
//! - Ebbinghaus forgetting curve-based decay calculation
//! - Tier-specific decay rate multipliers
//! - Reinforcement learning for dynamic importance scoring
//! - Configurable hard deletion of forgotten memories
//! - Performance metrics and monitoring

use super::error::{MemoryError, Result};
use super::math_engine::{MathEngine, MemoryParameters};
use super::models::*;
use super::repository::MemoryRepository;
use crate::config::ForgettingConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the forgetting background job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgettingJobConfig {
    /// Forgetting configuration
    pub forgetting_config: ForgettingConfig,

    /// Maximum number of memories to process per batch
    pub batch_size: usize,

    /// Maximum number of batches to process per run
    pub max_batches_per_run: usize,

    /// Math engine configuration for decay calculations
    pub math_engine_config: super::math_engine::MathEngineConfig,
}

impl Default for ForgettingJobConfig {
    fn default() -> Self {
        Self {
            forgetting_config: ForgettingConfig::default(),
            batch_size: 1000,
            max_batches_per_run: 10,
            math_engine_config: super::math_engine::MathEngineConfig::default(),
        }
    }
}

/// Result of a forgetting job run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgettingJobResult {
    pub run_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub total_memories_processed: usize,
    pub decay_rates_updated: usize,
    pub importance_scores_updated: usize,
    pub memories_hard_deleted: usize,
    pub batches_processed: usize,
    pub errors_encountered: usize,
    pub performance_metrics: ForgettingPerformanceMetrics,
}

/// Performance metrics for forgetting processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgettingPerformanceMetrics {
    pub memories_per_second: f64,
    pub total_processing_time_ms: u64,
    pub decay_calculation_time_ms: u64,
    pub database_update_time_ms: u64,
    pub hard_deletion_time_ms: u64,
    pub reinforcement_learning_time_ms: u64,
}

/// Batch processing result for forgetting operations
#[derive(Debug, Clone)]
pub struct ForgettingBatchResult {
    pub processed_count: usize,
    pub decay_updates: Vec<(Uuid, f64)>, // (memory_id, new_decay_rate)
    pub importance_updates: Vec<(Uuid, f64)>, // (memory_id, new_importance_score)
    pub hard_deletion_candidates: Vec<Uuid>,
    pub processing_time_ms: u64,
}

/// Background forgetting job runner
pub struct ForgettingJob {
    config: ForgettingJobConfig,
    repository: Arc<MemoryRepository>,
    math_engine: MathEngine,
    is_running: std::sync::atomic::AtomicBool,
}

impl ForgettingJob {
    pub fn new(config: ForgettingJobConfig, repository: Arc<MemoryRepository>) -> Self {
        let math_engine = MathEngine::with_config(config.math_engine_config.clone());

        Self {
            config,
            repository,
            math_engine,
            is_running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Start the background forgetting job
    pub async fn start(&self) -> Result<()> {
        if self.is_running.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(MemoryError::InvalidRequest {
                message: "Forgetting job is already running".to_string(),
            });
        }

        if !self.config.forgetting_config.enabled {
            info!("Forgetting job is disabled in configuration");
            return Ok(());
        }

        self.is_running
            .store(true, std::sync::atomic::Ordering::Relaxed);
        info!("Starting forgetting background job");

        let mut interval = time::interval(Duration::from_secs(
            self.config.forgetting_config.cleanup_interval_seconds,
        ));

        while self.is_running.load(std::sync::atomic::Ordering::Relaxed) {
            interval.tick().await;

            match self.run_forgetting_cycle().await {
                Ok(result) => {
                    info!(
                        "Forgetting cycle completed: {} memories processed, {} decay updates, {} importance updates, {} hard deletions, {:.1} mem/sec",
                        result.total_memories_processed,
                        result.decay_rates_updated,
                        result.importance_scores_updated,
                        result.memories_hard_deleted,
                        result.performance_metrics.memories_per_second
                    );
                }
                Err(e) => {
                    error!("Forgetting cycle failed: {}", e);
                    // Continue running despite errors
                }
            }
        }

        Ok(())
    }

    /// Stop the background forgetting job
    pub fn stop(&self) {
        info!("Stopping forgetting background job");
        self.is_running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// Run a single forgetting cycle
    pub async fn run_forgetting_cycle(&self) -> Result<ForgettingJobResult> {
        let run_id = Uuid::new_v4();
        let started_at = Utc::now();
        let start_time = Instant::now();

        debug!("Starting forgetting cycle {}", run_id);

        let mut total_processed = 0;
        let mut decay_updates = 0;
        let mut importance_updates = 0;
        let mut hard_deletions = 0;
        let mut batches_processed = 0;
        let mut errors_encountered = 0;
        let mut decay_calc_time = 0u64;
        let mut db_update_time = 0u64;
        let mut hard_deletion_time = 0u64;
        let mut rl_time = 0u64;

        // Process batches across all tiers
        for tier in [
            MemoryTier::Working,
            MemoryTier::Warm,
            MemoryTier::Cold,
            MemoryTier::Frozen,
        ] {
            for _ in 0..self.config.max_batches_per_run {
                let batch_start = Instant::now();

                match self.process_forgetting_batch(tier).await {
                    Ok(batch_result) => {
                        if batch_result.processed_count == 0 {
                            break; // No more memories to process in this tier
                        }

                        // Apply decay rate updates
                        if !batch_result.decay_updates.is_empty() {
                            let update_start = Instant::now();
                            match self.apply_decay_updates(&batch_result.decay_updates).await {
                                Ok(_) => {
                                    db_update_time += update_start.elapsed().as_millis() as u64;
                                    decay_updates += batch_result.decay_updates.len();
                                }
                                Err(e) => {
                                    warn!("Failed to apply decay updates: {}", e);
                                    errors_encountered += 1;
                                }
                            }
                        }

                        // Apply importance updates (reinforcement learning)
                        if !batch_result.importance_updates.is_empty() {
                            let rl_start = Instant::now();
                            match self
                                .apply_importance_updates(&batch_result.importance_updates)
                                .await
                            {
                                Ok(_) => {
                                    rl_time += rl_start.elapsed().as_millis() as u64;
                                    importance_updates += batch_result.importance_updates.len();
                                }
                                Err(e) => {
                                    warn!("Failed to apply importance updates: {}", e);
                                    errors_encountered += 1;
                                }
                            }
                        }

                        // Process hard deletions if enabled
                        if self.config.forgetting_config.enable_hard_deletion
                            && !batch_result.hard_deletion_candidates.is_empty()
                        {
                            let deletion_start = Instant::now();
                            match self
                                .process_hard_deletions(&batch_result.hard_deletion_candidates)
                                .await
                            {
                                Ok(deleted_count) => {
                                    hard_deletion_time +=
                                        deletion_start.elapsed().as_millis() as u64;
                                    hard_deletions += deleted_count;
                                }
                                Err(e) => {
                                    warn!("Failed to process hard deletions: {}", e);
                                    errors_encountered += 1;
                                }
                            }
                        }

                        total_processed += batch_result.processed_count;
                        batches_processed += 1;
                        decay_calc_time += batch_result.processing_time_ms;

                        debug!(
                            "Processed forgetting batch for {:?}: {} memories, {} decay updates, {} importance updates, {} deletion candidates",
                            tier,
                            batch_result.processed_count,
                            batch_result.decay_updates.len(),
                            batch_result.importance_updates.len(),
                            batch_result.hard_deletion_candidates.len()
                        );
                    }
                    Err(e) => {
                        warn!("Failed to process forgetting batch for {:?}: {}", tier, e);
                        errors_encountered += 1;
                        break;
                    }
                }
            }
        }

        let completed_at = Utc::now();
        let total_time = start_time.elapsed();

        let performance_metrics = ForgettingPerformanceMetrics {
            memories_per_second: if total_time.as_secs_f64() > 0.0 {
                total_processed as f64 / total_time.as_secs_f64()
            } else {
                0.0
            },
            total_processing_time_ms: total_time.as_millis() as u64,
            decay_calculation_time_ms: decay_calc_time,
            database_update_time_ms: db_update_time,
            hard_deletion_time_ms: hard_deletion_time,
            reinforcement_learning_time_ms: rl_time,
        };

        Ok(ForgettingJobResult {
            run_id,
            started_at,
            completed_at,
            total_memories_processed: total_processed,
            decay_rates_updated: decay_updates,
            importance_scores_updated: importance_updates,
            memories_hard_deleted: hard_deletions,
            batches_processed,
            errors_encountered,
            performance_metrics,
        })
    }

    /// Process a batch of memories for forgetting operations
    async fn process_forgetting_batch(&self, tier: MemoryTier) -> Result<ForgettingBatchResult> {
        let start_time = Instant::now();

        // Get memories that haven't been updated recently
        let memories = self.get_memories_for_forgetting(tier).await?;

        if memories.is_empty() {
            return Ok(ForgettingBatchResult {
                processed_count: 0,
                decay_updates: Vec::new(),
                importance_updates: Vec::new(),
                hard_deletion_candidates: Vec::new(),
                processing_time_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        let mut decay_updates = Vec::new();
        let mut importance_updates = Vec::new();
        let mut hard_deletion_candidates = Vec::new();

        for memory in &memories {
            // Calculate new decay rate based on tier and access patterns
            let new_decay_rate = self.calculate_adaptive_decay_rate(memory, tier)?;
            if (new_decay_rate - memory.decay_rate).abs() > 0.01 {
                decay_updates.push((memory.id, new_decay_rate));
            }

            // Update importance score using reinforcement learning if enabled
            if self.config.forgetting_config.enable_reinforcement_learning {
                let new_importance = self.calculate_adaptive_importance(memory)?;
                if (new_importance - memory.importance_score).abs() > 0.01 {
                    importance_updates.push((memory.id, new_importance));
                }
            }

            // Check if memory should be hard deleted
            if self.config.forgetting_config.enable_hard_deletion {
                let params = self.create_memory_parameters(memory)?;
                let recall_result = self.math_engine.calculate_recall_probability(&params)?;

                if recall_result.recall_probability
                    < self.config.forgetting_config.hard_deletion_threshold
                {
                    // Check if memory is old enough for deletion
                    let age_days = (Utc::now() - memory.created_at).num_days();
                    if age_days >= self.config.forgetting_config.hard_deletion_retention_days as i64
                    {
                        hard_deletion_candidates.push(memory.id);
                    }
                }
            }
        }

        Ok(ForgettingBatchResult {
            processed_count: memories.len(),
            decay_updates,
            importance_updates,
            hard_deletion_candidates,
            processing_time_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    /// Calculate adaptive decay rate based on tier, access patterns, and importance
    fn calculate_adaptive_decay_rate(&self, memory: &Memory, tier: MemoryTier) -> Result<f64> {
        let base_rate = self.config.forgetting_config.base_decay_rate;

        // Apply tier-specific multiplier
        let tier_multiplier = match tier {
            MemoryTier::Working => self.config.forgetting_config.working_decay_multiplier,
            MemoryTier::Warm => self.config.forgetting_config.warm_decay_multiplier,
            MemoryTier::Cold => self.config.forgetting_config.cold_decay_multiplier,
            MemoryTier::Frozen => self.config.forgetting_config.cold_decay_multiplier * 1.5,
        };

        // Apply importance factor (higher importance = lower decay)
        let importance_factor =
            1.0 - (memory.importance_score * self.config.forgetting_config.importance_decay_factor);

        // Apply age-based scaling
        let age_days = (Utc::now() - memory.created_at).num_days() as f64;
        let age_factor = if age_days > 0.0 {
            1.0 + (age_days / 30.0)
                .min(self.config.forgetting_config.max_age_decay_multiplier - 1.0)
        } else {
            1.0
        };

        // Apply access frequency factor (more access = slower decay)
        let access_factor = if memory.access_count > 0 {
            1.0 / (1.0 + (memory.access_count as f64).ln())
        } else {
            1.0
        };

        let new_decay_rate =
            base_rate * tier_multiplier * importance_factor * age_factor * access_factor;

        // Apply bounds
        Ok(new_decay_rate
            .max(self.config.forgetting_config.min_decay_rate)
            .min(self.config.forgetting_config.max_decay_rate))
    }

    /// Calculate adaptive importance using reinforcement learning
    fn calculate_adaptive_importance(&self, memory: &Memory) -> Result<f64> {
        let learning_rate = self.config.forgetting_config.learning_rate;
        let current_importance = memory.importance_score;

        // Simple reinforcement learning based on access patterns
        let access_frequency = memory.access_count as f64;
        let recency_hours = memory
            .last_accessed_at
            .map(|last| (Utc::now() - last).num_seconds() as f64 / 3600.0)
            .unwrap_or(f64::MAX);

        // Reward frequent recent access, penalize infrequent old access
        let access_reward = if recency_hours < 24.0 {
            (access_frequency / (1.0 + recency_hours)).min(1.0)
        } else {
            0.0
        };

        // Apply reinforcement learning update
        let importance_delta = learning_rate * (access_reward - 0.5); // Center around 0.5
        let new_importance = (current_importance + importance_delta).max(0.0).min(1.0);

        Ok(new_importance)
    }

    /// Create memory parameters for math engine calculations
    fn create_memory_parameters(&self, memory: &Memory) -> Result<MemoryParameters> {
        Ok(MemoryParameters {
            consolidation_strength: memory.consolidation_strength,
            decay_rate: memory.decay_rate,
            last_accessed_at: memory.last_accessed_at,
            created_at: memory.created_at,
            access_count: memory.access_count,
            importance_score: memory.importance_score,
        })
    }

    /// Get memories that need forgetting processing
    async fn get_memories_for_forgetting(&self, tier: MemoryTier) -> Result<Vec<Memory>> {
        // Use repository method instead of direct SQL
        self.repository
            .get_memories_for_forgetting(tier, self.config.batch_size)
            .await
    }

    /// Apply decay rate updates to the database
    async fn apply_decay_updates(&self, updates: &[(Uuid, f64)]) -> Result<()> {
        // Use repository method instead of direct SQL
        self.repository.batch_update_decay_rates(updates).await?;
        Ok(())
    }

    /// Apply importance score updates to the database
    async fn apply_importance_updates(&self, updates: &[(Uuid, f64)]) -> Result<()> {
        // Use repository method instead of direct SQL
        self.repository
            .batch_update_importance_scores(updates)
            .await?;
        Ok(())
    }

    /// Process hard deletion of completely forgotten memories
    async fn process_hard_deletions(&self, candidates: &[Uuid]) -> Result<usize> {
        if candidates.is_empty() {
            return Ok(0);
        }

        // Use repository method instead of direct SQL
        let deleted_count = self
            .repository
            .batch_soft_delete_memories(candidates)
            .await?;

        info!(
            "Marked {} memories as deleted due to forgetting",
            deleted_count
        );
        Ok(deleted_count)
    }

    /// Check if the job is currently running
    pub fn is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Async function to run forgetting job in the background
pub async fn spawn_forgetting_job(
    config: ForgettingJobConfig,
    repository: Arc<MemoryRepository>,
) -> tokio::task::JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let job = ForgettingJob::new(config, repository);
        job.start().await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forgetting_job_config_defaults() {
        let config = ForgettingJobConfig::default();

        assert!(config.forgetting_config.enabled);
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.forgetting_config.cleanup_interval_seconds, 3600);
        assert!(config.forgetting_config.enable_reinforcement_learning);
        assert!(!config.forgetting_config.enable_hard_deletion); // Conservative default
    }

    #[test]
    fn test_adaptive_decay_rate_calculation() {
        let config = ForgettingJobConfig::default();

        // Test tier-specific multipliers
        assert_eq!(config.forgetting_config.working_decay_multiplier, 0.5);
        assert_eq!(config.forgetting_config.warm_decay_multiplier, 1.0);
        assert_eq!(config.forgetting_config.cold_decay_multiplier, 1.5);

        // Test bounds
        assert_eq!(config.forgetting_config.min_decay_rate, 0.1);
        assert_eq!(config.forgetting_config.max_decay_rate, 5.0);
    }

    #[test]
    fn test_reinforcement_learning_parameters() {
        let config = ForgettingJobConfig::default();

        assert!(config.forgetting_config.enable_reinforcement_learning);
        assert_eq!(config.forgetting_config.learning_rate, 0.1);
        assert_eq!(config.forgetting_config.importance_decay_factor, 0.5);
    }
}
