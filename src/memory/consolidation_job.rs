//! Background Consolidation Job
//!
//! This module implements a background job for processing memory consolidation
//! in batches. Target performance: Process 1000 memories in < 1 second.

use super::error::{MemoryError, Result};
use super::models::*;
use super::repository::MemoryRepository;
use super::simple_consolidation::{
    ConsolidationBatchResult, ConsolidationProcessor, SimpleConsolidationConfig,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the consolidation background job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationJobConfig {
    /// Interval between job runs (in seconds)
    pub run_interval_seconds: u64,

    /// Maximum number of memories to process per batch
    pub batch_size: usize,

    /// Maximum number of batches to process per run
    pub max_batches_per_run: usize,

    /// Enable automatic migration of memories that should migrate
    pub enable_automatic_migration: bool,

    /// Minimum time between processing the same memory (in hours)
    pub min_processing_interval_hours: f64,

    /// Consolidation engine configuration
    pub consolidation_config: SimpleConsolidationConfig,
}

impl Default for ConsolidationJobConfig {
    fn default() -> Self {
        Self {
            run_interval_seconds: 300, // 5 minutes
            batch_size: 1000,
            max_batches_per_run: 10,
            enable_automatic_migration: true,
            min_processing_interval_hours: 1.0,
            consolidation_config: SimpleConsolidationConfig::default(),
        }
    }
}

/// Result of a consolidation job run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationJobResult {
    pub run_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub total_memories_processed: usize,
    pub total_memories_migrated: usize,
    pub batches_processed: usize,
    pub average_batch_time_ms: f64,
    pub errors_encountered: usize,
    pub performance_metrics: ConsolidationPerformanceMetrics,
}

/// Performance metrics for consolidation processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPerformanceMetrics {
    pub memories_per_second: f64,
    pub total_processing_time_ms: u64,
    pub database_update_time_ms: u64,
    pub migration_time_ms: u64,
    pub peak_memory_usage_mb: f64,
}

/// Background consolidation job runner
pub struct ConsolidationJob {
    config: ConsolidationJobConfig,
    repository: Arc<MemoryRepository>,
    processor: ConsolidationProcessor,
    is_running: std::sync::atomic::AtomicBool,
}

impl ConsolidationJob {
    pub fn new(config: ConsolidationJobConfig, repository: Arc<MemoryRepository>) -> Self {
        let processor =
            ConsolidationProcessor::new(config.consolidation_config.clone(), config.batch_size);

        Self {
            config,
            repository,
            processor,
            is_running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Start the background consolidation job
    pub async fn start(&self) -> Result<()> {
        if self.is_running.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(MemoryError::InvalidRequest {
                message: "Consolidation job is already running".to_string(),
            });
        }

        self.is_running
            .store(true, std::sync::atomic::Ordering::Relaxed);
        info!("Starting consolidation background job");

        let mut interval = time::interval(Duration::from_secs(self.config.run_interval_seconds));

        while self.is_running.load(std::sync::atomic::Ordering::Relaxed) {
            interval.tick().await;

            match self.run_consolidation_cycle().await {
                Ok(result) => {
                    info!(
                        "Consolidation cycle completed: {} memories processed, {} migrated, {:.1} mem/sec",
                        result.total_memories_processed,
                        result.total_memories_migrated,
                        result.performance_metrics.memories_per_second
                    );
                }
                Err(e) => {
                    error!("Consolidation cycle failed: {}", e);
                    // Continue running despite errors
                }
            }
        }

        Ok(())
    }

    /// Stop the background consolidation job
    pub fn stop(&self) {
        info!("Stopping consolidation background job");
        self.is_running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// Run a single consolidation cycle
    pub async fn run_consolidation_cycle(&self) -> Result<ConsolidationJobResult> {
        let run_id = Uuid::new_v4();
        let started_at = Utc::now();
        let start_time = Instant::now();

        debug!("Starting consolidation cycle {}", run_id);

        let mut total_processed = 0;
        let mut total_migrated = 0;
        let mut batches_processed = 0;
        let mut batch_times = Vec::new();
        let mut errors_encountered = 0;
        let mut db_update_time = 0u64;
        let mut migration_time = 0u64;

        // Process batches across all tiers
        for tier in [MemoryTier::Working, MemoryTier::Warm, MemoryTier::Cold] {
            for _ in 0..self.config.max_batches_per_run {
                let batch_start = Instant::now();

                match self
                    .processor
                    .process_consolidation_batch(&self.repository, Some(tier))
                    .await
                {
                    Ok(batch_result) => {
                        if batch_result.processed_count == 0 {
                            break; // No more memories to process in this tier
                        }

                        // Update consolidation values in database
                        let update_start = Instant::now();
                        match self.apply_consolidation_updates(&batch_result).await {
                            Ok(_) => {
                                db_update_time += update_start.elapsed().as_millis() as u64;
                            }
                            Err(e) => {
                                warn!("Failed to apply consolidation updates: {}", e);
                                errors_encountered += 1;
                            }
                        }

                        // Process migrations if enabled
                        if self.config.enable_automatic_migration
                            && !batch_result.migration_candidates.is_empty()
                        {
                            let migration_start = Instant::now();
                            match self
                                .process_migrations(&batch_result.migration_candidates)
                                .await
                            {
                                Ok(migrated_count) => {
                                    total_migrated += migrated_count;
                                    migration_time += migration_start.elapsed().as_millis() as u64;
                                }
                                Err(e) => {
                                    warn!("Failed to process migrations: {}", e);
                                    errors_encountered += 1;
                                }
                            }
                        }

                        total_processed += batch_result.processed_count;
                        batches_processed += 1;
                        batch_times.push(batch_start.elapsed().as_millis() as f64);

                        debug!(
                            "Processed batch for {:?}: {} memories, {} candidates for migration",
                            tier,
                            batch_result.processed_count,
                            batch_result.migration_candidates.len()
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to process consolidation batch for {:?}: {}",
                            tier, e
                        );
                        errors_encountered += 1;
                        break;
                    }
                }
            }
        }

        let completed_at = Utc::now();
        let total_time = start_time.elapsed();
        let avg_batch_time = if batch_times.is_empty() {
            0.0
        } else {
            batch_times.iter().sum::<f64>() / batch_times.len() as f64
        };

        let performance_metrics = ConsolidationPerformanceMetrics {
            memories_per_second: if total_time.as_secs_f64() > 0.0 {
                total_processed as f64 / total_time.as_secs_f64()
            } else {
                0.0
            },
            total_processing_time_ms: total_time.as_millis() as u64,
            database_update_time_ms: db_update_time,
            migration_time_ms: migration_time,
            peak_memory_usage_mb: self.get_peak_memory_usage(),
        };

        Ok(ConsolidationJobResult {
            run_id,
            started_at,
            completed_at,
            total_memories_processed: total_processed,
            total_memories_migrated: total_migrated,
            batches_processed,
            average_batch_time_ms: avg_batch_time,
            errors_encountered,
            performance_metrics,
        })
    }

    /// Apply consolidation updates to the database
    async fn apply_consolidation_updates(
        &self,
        batch_result: &ConsolidationBatchResult,
    ) -> Result<()> {
        if batch_result.consolidation_updates.is_empty() {
            return Ok(());
        }

        // Use repository method instead of direct SQL (clean architecture)
        self.repository
            .batch_update_consolidation(&batch_result.consolidation_updates)
            .await?;
        Ok(())
    }

    /// Process memory tier migrations
    async fn process_migrations(&self, candidates: &[(Uuid, MemoryTier)]) -> Result<usize> {
        if candidates.is_empty() {
            return Ok(0);
        }

        let mut migrated_count = 0;
        let mut tx = self.repository.pool().begin().await?;

        for (memory_id, target_tier) in candidates {
            match sqlx::query(
                r#"
                UPDATE memories 
                SET tier = $1, status = 'migrating', updated_at = NOW()
                WHERE id = $2 AND status = 'active'
                "#,
            )
            .bind(target_tier)
            .bind(memory_id)
            .execute(&mut *tx)
            .await
            {
                Ok(result) => {
                    if result.rows_affected() > 0 {
                        migrated_count += 1;

                        // Log migration
                        sqlx::query(
                            r#"
                            INSERT INTO migration_history (memory_id, from_tier, to_tier, migration_reason, migrated_at, success)
                            SELECT m.id, 
                                   CASE m.tier 
                                       WHEN 'working' THEN 'working'
                                       WHEN 'warm' THEN 'warm' 
                                       WHEN 'cold' THEN 'cold'
                                       ELSE 'frozen'
                                   END,
                                   $2, 
                                   'Automatic consolidation-based migration', 
                                   NOW(), 
                                   true
                            FROM memories m WHERE m.id = $1
                            "#,
                        )
                        .bind(memory_id)
                        .bind(target_tier)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
                Err(e) => {
                    warn!("Failed to migrate memory {}: {}", memory_id, e);
                }
            }
        }

        // Update status back to active for migrated memories
        sqlx::query(
            r#"
            UPDATE memories 
            SET status = 'active' 
            WHERE status = 'migrating'
            "#,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(migrated_count)
    }

    /// Get current memory usage (simplified implementation)
    fn get_peak_memory_usage(&self) -> f64 {
        // In a real implementation, you would use a memory profiler
        // For now, return a placeholder value
        0.0
    }

    /// Check if the job is currently running
    pub fn is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get job statistics
    pub async fn get_job_statistics(&self) -> Result<ConsolidationJobStatistics> {
        let stats = sqlx::query_as::<_, ConsolidationJobStatistics>(
            r#"
            SELECT 
                COUNT(*) as total_runs,
                AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) as avg_runtime_seconds,
                SUM(total_memories_processed) as total_memories_processed,
                SUM(total_memories_migrated) as total_memories_migrated,
                MAX(memories_per_second) as peak_throughput
            FROM consolidation_job_history
            WHERE started_at > NOW() - INTERVAL '24 hours'
            "#,
        )
        .fetch_one(self.repository.pool())
        .await
        .unwrap_or_default();

        Ok(stats)
    }
}

/// Statistics for consolidation job performance
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConsolidationJobStatistics {
    pub total_runs: Option<i64>,
    pub avg_runtime_seconds: Option<f64>,
    pub total_memories_processed: Option<i64>,
    pub total_memories_migrated: Option<i64>,
    pub peak_throughput: Option<f64>,
}

impl Default for ConsolidationJobStatistics {
    fn default() -> Self {
        Self {
            total_runs: Some(0),
            avg_runtime_seconds: Some(0.0),
            total_memories_processed: Some(0),
            total_memories_migrated: Some(0),
            peak_throughput: Some(0.0),
        }
    }
}

/// Async function to run consolidation job in the background
pub async fn spawn_consolidation_job(
    config: ConsolidationJobConfig,
    repository: Arc<MemoryRepository>,
) -> tokio::task::JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let job = ConsolidationJob::new(config, repository);
        job.start().await
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_consolidation_job_creation() {
        let config = ConsolidationJobConfig::default();

        // Mock repository would be needed for full testing
        // For now, just test configuration
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.run_interval_seconds, 300);
        assert!(config.enable_automatic_migration);
    }

    #[test]
    fn test_consolidation_job_config_defaults() {
        let config = ConsolidationJobConfig::default();

        assert!(config.batch_size > 0);
        assert!(config.run_interval_seconds > 0);
        assert!(config.max_batches_per_run > 0);
        assert!(config.min_processing_interval_hours > 0.0);
    }
}
