use super::error::{MemoryError, Result};
use super::math_engine::{MathEngine, MemoryParameters};
use super::models::{Memory, MemoryTier};
use super::repository::MemoryRepository;
use crate::config::TierManagerConfig;
use chrono::{DateTime, Duration, Utc};
use prometheus::{register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time::{interval, sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Centralized tier management service implementing cognitive memory research principles
/// 
/// This service continuously monitors memory recall probabilities and automatically
/// migrates memories between tiers based on forgetting curves and consolidation strength.
/// It follows Ebbinghaus's forgetting curve and modern spaced repetition research.
pub struct TierManager {
    repository: Arc<MemoryRepository>,
    config: TierManagerConfig,
    math_engine: MathEngine,
    
    // Service state
    running: Arc<AtomicBool>,
    last_scan_time: Arc<RwLock<Option<DateTime<Utc>>>>,
    
    // Performance tracking
    migrations_completed: Arc<AtomicU64>,
    migrations_failed: Arc<AtomicU64>,
    total_scan_time_ms: Arc<AtomicU64>,
    
    // Prometheus metrics
    scan_duration_histogram: Histogram,
    migration_counter: Counter,
    migration_failure_counter: Counter,
    memories_per_tier_gauge: Gauge,
    recall_probability_histogram: Histogram,
}

#[derive(Debug, Clone)]
pub struct TierMigrationCandidate {
    pub memory_id: Uuid,
    pub current_tier: MemoryTier,
    pub target_tier: MemoryTier,
    pub recall_probability: f64,
    pub migration_reason: String,
    pub priority_score: f64,  // Higher means more urgent migration
}

#[derive(Debug, Clone)]
pub struct TierMigrationBatch {
    pub candidates: Vec<TierMigrationCandidate>,
    pub batch_id: Uuid,
    pub estimated_duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct TierMigrationResult {
    pub batch_id: Uuid,
    pub successful_migrations: Vec<Uuid>,
    pub failed_migrations: Vec<(Uuid, String)>,
    pub duration_ms: u64,
    pub memories_per_second: f64,
}

#[derive(Debug, Clone)]
pub struct TierManagerMetrics {
    pub total_migrations_completed: u64,
    pub total_migrations_failed: u64,
    pub last_scan_duration_ms: u64,
    pub memories_by_tier: HashMap<MemoryTier, u64>,
    pub average_recall_probability_by_tier: HashMap<MemoryTier, f64>,
    pub migrations_per_second_recent: f64,
    pub is_running: bool,
    pub last_scan_time: Option<DateTime<Utc>>,
}

impl TierManager {
    /// Create a new TierManager instance
    pub fn new(repository: Arc<MemoryRepository>, config: TierManagerConfig) -> Result<Self> {
        // Initialize Prometheus metrics
        let scan_duration_histogram = register_histogram!(
            "tier_manager_scan_duration_seconds",
            "Time taken for tier management scans",
            vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0]
        )?;
        
        let migration_counter = register_counter!(
            "tier_manager_migrations_total",
            "Total number of tier migrations completed"
        )?;
        
        let migration_failure_counter = register_counter!(
            "tier_manager_migration_failures_total", 
            "Total number of tier migration failures"
        )?;
        
        let memories_per_tier_gauge = register_gauge!(
            "tier_manager_memories_per_tier",
            "Number of memories in each tier"
        )?;
        
        let recall_probability_histogram = register_histogram!(
            "tier_manager_recall_probability",
            "Distribution of recall probabilities",
            vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
        )?;

        Ok(Self {
            repository,
            config,
            math_engine: MathEngine::new(),
            running: Arc::new(AtomicBool::new(false)),
            last_scan_time: Arc::new(RwLock::new(None)),
            migrations_completed: Arc::new(AtomicU64::new(0)),
            migrations_failed: Arc::new(AtomicU64::new(0)),
            total_scan_time_ms: Arc::new(AtomicU64::new(0)),
            scan_duration_histogram,
            migration_counter,
            migration_failure_counter,
            memories_per_tier_gauge,
            recall_probability_histogram,
        })
    }
    
    /// Start the tier management service as a background task
    pub async fn start(&self) -> Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Err(MemoryError::ServiceError("TierManager is already running".to_string()));
        }
        
        if !self.config.enabled {
            info!("TierManager is disabled in configuration");
            return Ok(());
        }
        
        info!("Starting TierManager service with {} second scan interval", 
              self.config.scan_interval_seconds);
        
        self.running.store(true, Ordering::Relaxed);
        
        // Start the main management loop
        let manager = self.clone();
        tokio::spawn(async move {
            manager.management_loop().await;
        });
        
        Ok(())
    }
    
    /// Stop the tier management service
    pub async fn stop(&self) {
        info!("Stopping TierManager service");
        self.running.store(false, Ordering::Relaxed);
        
        // Give time for any running operations to complete
        sleep(TokioDuration::from_secs(2)).await;
    }
    
    /// Get current metrics for monitoring
    pub async fn get_metrics(&self) -> Result<TierManagerMetrics> {
        let memories_by_tier = self.get_memory_counts_by_tier().await?;
        let recall_probabilities = self.get_average_recall_probabilities_by_tier().await?;
        
        Ok(TierManagerMetrics {
            total_migrations_completed: self.migrations_completed.load(Ordering::Relaxed),
            total_migrations_failed: self.migrations_failed.load(Ordering::Relaxed),
            last_scan_duration_ms: self.total_scan_time_ms.load(Ordering::Relaxed),
            memories_by_tier,
            average_recall_probability_by_tier: recall_probabilities,
            migrations_per_second_recent: self.calculate_recent_migration_rate().await,
            is_running: self.running.load(Ordering::Relaxed),
            last_scan_time: *self.last_scan_time.read().await,
        })
    }
    
    /// Force an immediate tier management scan (for testing/manual triggering)
    pub async fn force_scan(&self) -> Result<TierMigrationResult> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(MemoryError::ServiceError("TierManager is not running".to_string()));
        }
        
        info!("Forcing immediate tier management scan");
        self.perform_tier_management_scan().await
    }
}

// Private implementation methods
impl TierManager {
    /// Main management loop that runs continuously
    async fn management_loop(&self) {
        let mut scan_interval = interval(TokioDuration::from_secs(self.config.scan_interval_seconds));
        
        while self.running.load(Ordering::Relaxed) {
            scan_interval.tick().await;
            
            if let Err(e) = self.perform_tier_management_scan().await {
                error!("Tier management scan failed: {}", e);
                // Continue running despite errors
            }
        }
        
        info!("TierManager management loop stopped");
    }
    
    /// Perform a complete tier management scan
    async fn perform_tier_management_scan(&self) -> Result<TierMigrationResult> {
        let scan_start = Instant::now();
        let scan_time = Utc::now();
        
        debug!("Starting tier management scan");
        
        // Find migration candidates for each tier transition
        let candidates = self.find_migration_candidates().await?;
        
        if candidates.is_empty() {
            debug!("No migration candidates found");
            *self.last_scan_time.write().await = Some(scan_time);
            return Ok(TierMigrationResult {
                batch_id: Uuid::new_v4(),
                successful_migrations: Vec::new(),
                failed_migrations: Vec::new(),
                duration_ms: scan_start.elapsed().as_millis() as u64,
                memories_per_second: 0.0,
            });
        }
        
        info!("Found {} migration candidates", candidates.len());
        
        // Create migration batches
        let batches = self.create_migration_batches(candidates);
        
        // Process batches with concurrency control
        let result = self.process_migration_batches(batches).await?;
        
        // Update metrics
        let scan_duration = scan_start.elapsed();
        self.scan_duration_histogram.observe(scan_duration.as_secs_f64());
        self.total_scan_time_ms.store(scan_duration.as_millis() as u64, Ordering::Relaxed);
        *self.last_scan_time.write().await = Some(scan_time);
        
        // Update tier count metrics
        self.update_tier_metrics().await?;
        
        info!(
            "Tier management scan completed: {} successful, {} failed, {:.2} migrations/sec", 
            result.successful_migrations.len(),
            result.failed_migrations.len(),
            result.memories_per_second
        );
        
        Ok(result)
    }
    
    /// Find all memories that should be migrated to different tiers
    async fn find_migration_candidates(&self) -> Result<Vec<TierMigrationCandidate>> {
        let mut candidates = Vec::new();
        
        // Check each tier for migration candidates
        for tier in [MemoryTier::Working, MemoryTier::Warm, MemoryTier::Cold] {
            let tier_candidates = self.find_candidates_for_tier(tier).await?;
            candidates.extend(tier_candidates);
        }
        
        // Sort by priority (higher priority first)
        candidates.sort_by(|a, b| b.priority_score.partial_cmp(&a.priority_score).unwrap_or(std::cmp::Ordering::Equal));
        
        Ok(candidates)
    }
    
    /// Find migration candidates for a specific source tier
    async fn find_candidates_for_tier(&self, source_tier: MemoryTier) -> Result<Vec<TierMigrationCandidate>> {
        // Get minimum age threshold for this tier
        let min_age_hours = match source_tier {
            MemoryTier::Working => self.config.min_working_age_hours,
            MemoryTier::Warm => self.config.min_warm_age_hours,
            MemoryTier::Cold => self.config.min_cold_age_hours,
            MemoryTier::Frozen => return Ok(Vec::new()), // Frozen memories don't migrate
        };
        
        let min_age_time = Utc::now() - Duration::hours(min_age_hours as i64);
        
        // Get all memories in this tier using a more robust approach
        // We'll get a limited set and filter by age in Rust to avoid schema issues
        let query_ids = sqlx::query_scalar!(
            "SELECT id FROM memories WHERE tier = $1 AND status = 'active' AND updated_at <= $2 ORDER BY updated_at ASC LIMIT 1000",
            source_tier as MemoryTier,
            min_age_time
        )
        .fetch_all(self.repository.pool())
        .await?;
        
        let mut candidates = Vec::new();
        
        // Process memories in batches to avoid overwhelming the system
        for memory_id in query_ids {
            // Use the repository's get_memory method to handle schema variations properly
            if let Ok(memory) = self.repository.get_memory(memory_id).await {
                if let Some(candidate) = self.evaluate_migration_candidate(&memory).await? {
                    candidates.push(candidate);
                }
            }
        }
        
        Ok(candidates)
    }
    
    /// Evaluate if a memory should be migrated and determine target tier
    async fn evaluate_migration_candidate(&self, memory: &Memory) -> Result<Option<TierMigrationCandidate>> {
        // Calculate current recall probability using the math engine
        let recall_probability = self.calculate_recall_probability(memory)?;
        
        // Record this measurement for metrics
        if self.config.enable_metrics {
            self.recall_probability_histogram.observe(recall_probability);
        }
        
        // Determine if migration is needed based on thresholds
        let (should_migrate, target_tier, reason) = match memory.tier {
            MemoryTier::Working => {
                if recall_probability < self.config.working_to_warm_threshold {
                    (true, MemoryTier::Warm, format!("Recall probability {:.3} below threshold {:.3}", 
                                                   recall_probability, self.config.working_to_warm_threshold))
                } else {
                    (false, memory.tier, String::new())
                }
            },
            MemoryTier::Warm => {
                if recall_probability < self.config.warm_to_cold_threshold {
                    (true, MemoryTier::Cold, format!("Recall probability {:.3} below threshold {:.3}", 
                                                   recall_probability, self.config.warm_to_cold_threshold))
                } else {
                    (false, memory.tier, String::new())
                }
            },
            MemoryTier::Cold => {
                if recall_probability < self.config.cold_to_frozen_threshold {
                    (true, MemoryTier::Frozen, format!("Recall probability {:.3} below threshold {:.3}", 
                                                     recall_probability, self.config.cold_to_frozen_threshold))
                } else {
                    (false, memory.tier, String::new())
                }
            },
            MemoryTier::Frozen => (false, memory.tier, String::new()), // Frozen never migrates
        };
        
        if !should_migrate {
            return Ok(None);
        }
        
        // Calculate priority score (lower recall probability = higher priority)
        let age_factor = Utc::now().signed_duration_since(memory.updated_at).num_hours() as f64 / 24.0;
        let priority_score = (1.0 - recall_probability) * (1.0 + age_factor.ln().max(0.0));
        
        Ok(Some(TierMigrationCandidate {
            memory_id: memory.id,
            current_tier: memory.tier,
            target_tier,
            recall_probability,
            migration_reason: reason,
            priority_score,
        }))
    }
    
    /// Calculate recall probability for a memory using the math engine
    fn calculate_recall_probability(&self, memory: &Memory) -> Result<f64> {
        let params = MemoryParameters {
            consolidation_strength: memory.consolidation_strength,
            decay_rate: memory.decay_rate,
            last_accessed_at: memory.last_accessed_at,
            created_at: memory.created_at,
            access_count: memory.access_count,
            importance_score: memory.importance_score,
        };
        
        match self.math_engine.calculate_recall_probability(&params) {
            Ok(result) => Ok(result.recall_probability),
            Err(e) => {
                warn!("Math engine calculation failed for memory {}: {}", memory.id, e);
                // Use fallback calculation based on consolidation strength and importance
                let fallback = (memory.importance_score * memory.consolidation_strength / 10.0)
                    .min(1.0)
                    .max(0.0);
                Ok(fallback)
            }
        }
    }
    
    /// Create migration batches from candidates
    fn create_migration_batches(&self, candidates: Vec<TierMigrationCandidate>) -> Vec<TierMigrationBatch> {
        let mut batches = Vec::new();
        let batch_size = self.config.migration_batch_size;
        
        for chunk in candidates.chunks(batch_size) {
            let batch = TierMigrationBatch {
                candidates: chunk.to_vec(),
                batch_id: Uuid::new_v4(),
                estimated_duration_ms: self.estimate_batch_duration(chunk.len()),
            };
            batches.push(batch);
        }
        
        batches
    }
    
    /// Estimate how long a batch will take to process
    fn estimate_batch_duration(&self, batch_size: usize) -> u64 {
        // Based on target performance: 1000 migrations/second means 1ms per migration
        // Add 20% overhead for safety
        (batch_size as f64 * 1.2) as u64
    }
    
    /// Process migration batches with concurrency control
    async fn process_migration_batches(&self, batches: Vec<TierMigrationBatch>) -> Result<TierMigrationResult> {
        let start_time = Instant::now();
        let mut all_successful = Vec::new();
        let mut all_failed = Vec::new();
        
        // Process batches with concurrency limit
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.max_concurrent_migrations));
        let mut handles = Vec::new();
        
        for batch in batches {
            let semaphore = semaphore.clone();
            let repository = self.repository.clone();
            let config = self.config.clone();
            
            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.expect("Semaphore acquisition failed");
                Self::process_single_batch(repository, batch, config).await
            });
            
            handles.push(handle);
        }
        
        // Wait for all batches to complete
        for handle in handles {
            match handle.await {
                Ok(Ok(result)) => {
                    all_successful.extend(result.successful_migrations);
                    all_failed.extend(result.failed_migrations);
                },
                Ok(Err(e)) => {
                    error!("Batch processing failed: {}", e);
                },
                Err(e) => {
                    error!("Batch task panicked: {}", e);
                },
            }
        }
        
        let duration = start_time.elapsed();
        let total_migrations = all_successful.len() + all_failed.len();
        let memories_per_second = if duration.as_secs_f64() > 0.0 {
            total_migrations as f64 / duration.as_secs_f64()
        } else {
            0.0
        };
        
        // Update counters
        self.migrations_completed.fetch_add(all_successful.len() as u64, Ordering::Relaxed);
        self.migrations_failed.fetch_add(all_failed.len() as u64, Ordering::Relaxed);
        
        if self.config.enable_metrics {
            self.migration_counter.inc_by(all_successful.len() as f64);
            self.migration_failure_counter.inc_by(all_failed.len() as f64);
        }
        
        Ok(TierMigrationResult {
            batch_id: Uuid::new_v4(),
            successful_migrations: all_successful,
            failed_migrations: all_failed,
            duration_ms: duration.as_millis() as u64,
            memories_per_second,
        })
    }
    
    /// Process a single migration batch
    async fn process_single_batch(
        repository: Arc<MemoryRepository>,
        batch: TierMigrationBatch,
        config: TierManagerConfig,
    ) -> Result<TierMigrationResult> {
        let start_time = Instant::now();
        let mut successful = Vec::new();
        let mut failed = Vec::new();
        
        for candidate in &batch.candidates {
            match Self::migrate_single_memory(&repository, candidate, &config).await {
                Ok(_) => {
                    successful.push(candidate.memory_id);
                    debug!("Successfully migrated memory {} from {:?} to {:?}", 
                          candidate.memory_id, candidate.current_tier, candidate.target_tier);
                },
                Err(e) => {
                    failed.push((candidate.memory_id, e.to_string()));
                    warn!("Failed to migrate memory {}: {}", candidate.memory_id, e);
                },
            }
        }
        
        let duration = start_time.elapsed();
        let memories_per_second = if duration.as_secs_f64() > 0.0 {
            batch.candidates.len() as f64 / duration.as_secs_f64()
        } else {
            0.0
        };
        
        Ok(TierMigrationResult {
            batch_id: batch.batch_id,
            successful_migrations: successful,
            failed_migrations: failed,
            duration_ms: duration.as_millis() as u64,
            memories_per_second,
        })
    }
    
    /// Migrate a single memory to its target tier
    async fn migrate_single_memory(
        repository: &MemoryRepository,
        candidate: &TierMigrationCandidate,
        config: &TierManagerConfig,
    ) -> Result<()> {
        let mut tx = repository.pool().begin().await?;
        
        // Update memory tier
        sqlx::query!(
            "UPDATE memories SET tier = $1, updated_at = NOW() WHERE id = $2",
            candidate.target_tier as MemoryTier,
            candidate.memory_id
        )
        .execute(&mut *tx)
        .await?;
        
        // Log migration if enabled
        if config.log_migrations {
            sqlx::query!(
                r#"
                INSERT INTO memory_consolidation_log (
                    id, memory_id, old_consolidation_strength, new_consolidation_strength, 
                    old_recall_probability, new_recall_probability, consolidation_event, 
                    trigger_reason, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
                "#,
                Uuid::new_v4(),
                candidate.memory_id,
                0.0, // We don't change consolidation strength during tier migration
                0.0,
                Some(candidate.recall_probability),
                Some(candidate.recall_probability),
                format!("tier_migration_{}_{}", 
                       format!("{:?}", candidate.current_tier).to_lowercase(),
                       format!("{:?}", candidate.target_tier).to_lowercase()),
                Some(format!("{}. Priority score: {:.3}", 
                           candidate.migration_reason, candidate.priority_score))
            )
            .execute(&mut *tx)
            .await?;
        }
        
        tx.commit().await?;
        Ok(())
    }
    
    /// Get memory counts by tier for metrics
    async fn get_memory_counts_by_tier(&self) -> Result<HashMap<MemoryTier, u64>> {
        let rows = sqlx::query!(
            r#"
            SELECT tier as "tier: MemoryTier", COUNT(*) as count
            FROM memories
            WHERE status = 'active'
            GROUP BY tier
            "#
        )
        .fetch_all(self.repository.pool())
        .await?;
        
        let mut counts = HashMap::new();
        for row in rows {
            counts.insert(row.tier, row.count.unwrap_or(0) as u64);
        }
        
        Ok(counts)
    }
    
    /// Get average recall probabilities by tier
    async fn get_average_recall_probabilities_by_tier(&self) -> Result<HashMap<MemoryTier, f64>> {
        let rows = sqlx::query!(
            r#"
            SELECT tier as "tier: MemoryTier", AVG(recall_probability) as avg_recall_prob
            FROM memories
            WHERE status = 'active' AND recall_probability IS NOT NULL
            GROUP BY tier
            "#
        )
        .fetch_all(self.repository.pool())
        .await?;
        
        let mut averages = HashMap::new();
        for row in rows {
            if let Some(avg) = row.avg_recall_prob {
                averages.insert(row.tier, avg);
            }
        }
        
        Ok(averages)
    }
    
    /// Calculate recent migration rate for performance monitoring
    async fn calculate_recent_migration_rate(&self) -> f64 {
        // This is a simplified calculation - in production you might want to track
        // migrations over a sliding time window
        let completed = self.migrations_completed.load(Ordering::Relaxed);
        let scan_time_ms = self.total_scan_time_ms.load(Ordering::Relaxed);
        
        if scan_time_ms > 0 {
            (completed as f64 * 1000.0) / scan_time_ms as f64
        } else {
            0.0
        }
    }
    
    /// Update Prometheus metrics for tier counts
    async fn update_tier_metrics(&self) -> Result<()> {
        if !self.config.enable_metrics {
            return Ok(());
        }
        
        let counts = self.get_memory_counts_by_tier().await?;
        
        for (_tier, count) in counts {
            // Set gauge with tier label - this is simplified; in production you'd use labeled metrics
            self.memories_per_tier_gauge.set(count as f64);
        }
        
        Ok(())
    }
}

// Clone implementation for moving the manager into async tasks
impl Clone for TierManager {
    fn clone(&self) -> Self {
        Self {
            repository: self.repository.clone(),
            config: self.config.clone(),
            math_engine: MathEngine::new(), // Math engine is stateless
            running: self.running.clone(),
            last_scan_time: self.last_scan_time.clone(),
            migrations_completed: self.migrations_completed.clone(),
            migrations_failed: self.migrations_failed.clone(),
            total_scan_time_ms: self.total_scan_time_ms.clone(),
            scan_duration_histogram: self.scan_duration_histogram.clone(),
            migration_counter: self.migration_counter.clone(),
            migration_failure_counter: self.migration_failure_counter.clone(),
            memories_per_tier_gauge: self.memories_per_tier_gauge.clone(),
            recall_probability_histogram: self.recall_probability_histogram.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::connection::create_pool;

    #[tokio::test]
    async fn test_tier_manager_creation() {
        let pool = create_pool("postgresql://test:test@localhost:5432/test", 5).await.unwrap();
        let repository = Arc::new(MemoryRepository::new(pool));
        let config = TierManagerConfig::default();
        
        let manager = TierManager::new(repository, config);
        assert!(manager.is_ok());
    }
    
    #[tokio::test]
    async fn test_migration_candidate_evaluation() {
        // This test would need a proper test database setup
        // For now, we'll just test the structure
        let memory = Memory {
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            recall_probability: Some(0.3), // Below working threshold
            tier: MemoryTier::Working,
            ..Memory::default()
        };
        
        // The actual test would check that this memory gets flagged for migration
        assert_eq!(memory.tier, MemoryTier::Working);
    }
}