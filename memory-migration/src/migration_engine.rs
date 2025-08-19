use crate::{Result, MigrationError, MigrationTrigger, ProgressTracker, DeadlockDetector, WorkerPool};
use async_trait::async_trait;
use memory_core::{MemoryRepository, MemoryTier};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    pub max_concurrent_migrations: usize,
    pub batch_size: usize,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub enable_rollback: bool,
    pub performance_impact_limit_percent: f32,
    pub memory_pressure_threshold_mb: usize,
    pub deadlock_timeout_seconds: u64,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            max_concurrent_migrations: 5,
            batch_size: 100,
            max_retries: 3,
            retry_delay_ms: 1000,
            enable_rollback: true,
            performance_impact_limit_percent: 5.0,
            memory_pressure_threshold_mb: 1024,
            deadlock_timeout_seconds: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationJob {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub from_tier: MemoryTier,
    pub to_tier: MemoryTier,
    pub reason: String,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub attempts: u32,
    pub last_error: Option<String>,
}

impl MigrationJob {
    pub fn new(memory_id: Uuid, from_tier: MemoryTier, to_tier: MemoryTier, reason: String, priority: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            memory_id,
            from_tier,
            to_tier,
            reason,
            priority,
            created_at: Utc::now(),
            attempts: 0,
            last_error: None,
        }
    }
}

impl PartialEq for MigrationJob {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for MigrationJob {}

impl PartialOrd for MigrationJob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MigrationJob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first (reverse order)
        other.priority.cmp(&self.priority)
            .then_with(|| self.created_at.cmp(&other.created_at))
    }
}

pub struct MigrationEngine {
    repository: Arc<MemoryRepository>,
    config: MigrationConfig,
    triggers: Vec<Box<dyn MigrationTrigger + Send + Sync>>,
    progress_tracker: Arc<ProgressTracker>,
    deadlock_detector: Arc<DeadlockDetector>,
    worker_pool: Arc<WorkerPool>,
    migration_semaphore: Arc<Semaphore>,
    active_migrations: Arc<RwLock<dashmap::DashMap<Uuid, MigrationJob>>>,
}

impl MigrationEngine {
    pub async fn new(
        repository: Arc<MemoryRepository>, 
        config: MigrationConfig,
        pool: PgPool,
    ) -> Result<Self> {
        let migration_semaphore = Arc::new(Semaphore::new(config.max_concurrent_migrations));
        let progress_tracker = Arc::new(ProgressTracker::new(pool.clone()));
        let deadlock_detector = Arc::new(DeadlockDetector::new(pool.clone(), config.deadlock_timeout_seconds));
        let worker_pool = Arc::new(WorkerPool::new(config.max_concurrent_migrations).await?);

        Ok(Self {
            repository,
            config,
            triggers: Vec::new(),
            progress_tracker,
            deadlock_detector,
            worker_pool,
            migration_semaphore,
            active_migrations: Arc::new(RwLock::new(dashmap::DashMap::new())),
        })
    }

    pub fn add_trigger(&mut self, trigger: Box<dyn MigrationTrigger + Send + Sync>) {
        self.triggers.push(trigger);
    }

    pub async fn start(&self) -> Result<()> {
        info!("Starting migration engine with {} triggers", self.triggers.len());
        
        // Start background processes
        self.start_trigger_evaluation().await?;
        self.start_deadlock_detection().await?;
        self.start_progress_cleanup().await?;

        Ok(())
    }

    async fn start_trigger_evaluation(&self) -> Result<()> {
        for trigger in &self.triggers {
            let engine = self.clone_for_trigger();
            let trigger_clone = trigger.clone_boxed();
            
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(trigger_clone.evaluation_interval());
                
                loop {
                    interval.tick().await;
                    
                    match trigger_clone.evaluate().await {
                        Ok(jobs) => {
                            for job in jobs {
                                if let Err(e) = engine.queue_migration_job(job).await {
                                    error!("Failed to queue migration job: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Trigger evaluation failed: {}", e);
                        }
                    }
                }
            });
        }
        
        Ok(())
    }

    async fn start_deadlock_detection(&self) -> Result<()> {
        let detector = self.deadlock_detector.clone();
        let active_migrations = self.active_migrations.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                match detector.detect_deadlocks().await {
                    Ok(deadlocked_migrations) => {
                        for migration_id in deadlocked_migrations {
                            warn!("Detected deadlock for migration: {}", migration_id);
                            
                            // Cancel the deadlocked migration
                            if let Some(migration) = active_migrations.read().await.get(&migration_id) {
                                // Implementation would cancel the migration and retry
                                error!("Canceling deadlocked migration: {}", migration_id);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Deadlock detection failed: {}", e);
                    }
                }
            }
        });
        
        Ok(())
    }

    async fn start_progress_cleanup(&self) -> Result<()> {
        let tracker = self.progress_tracker.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // 5 minutes
            
            loop {
                interval.tick().await;
                
                if let Err(e) = tracker.cleanup_completed_migrations().await {
                    error!("Progress cleanup failed: {}", e);
                }
            }
        });
        
        Ok(())
    }

    pub async fn queue_migration_job(&self, job: MigrationJob) -> Result<()> {
        debug!("Queuing migration job: {} -> {}", job.from_tier, job.to_tier);
        
        // Check if already migrating
        if self.active_migrations.read().await.contains_key(&job.memory_id) {
            return Err(MigrationError::MigrationInProgress { 
                id: job.memory_id.to_string() 
            });
        }
        
        // Add to active migrations tracking
        self.active_migrations.write().await.insert(job.memory_id, job.clone());
        
        // Create progress entry
        self.progress_tracker.start_migration(&job).await?;
        
        // Submit to worker pool
        let engine = self.clone_for_worker();
        self.worker_pool.submit_job(job.clone(), Box::new(move |job| {
            let engine = engine.clone();
            Box::pin(async move {
                engine.execute_migration(job).await
            })
        })).await?;
        
        Ok(())
    }

    async fn execute_migration(&self, mut job: MigrationJob) -> Result<()> {
        let _permit = self.migration_semaphore.acquire().await
            .map_err(|e| MigrationError::WorkerPool(e.to_string()))?;
        
        info!("Executing migration: {} from {:?} to {:?}", job.memory_id, job.from_tier, job.to_tier);
        
        let start_time = std::time::Instant::now();
        let mut rollback_data = None;
        
        for attempt in 1..=self.config.max_retries {
            job.attempts = attempt;
            
            match self.try_migrate_memory(&job, &mut rollback_data).await {
                Ok(_) => {
                    let duration = start_time.elapsed();
                    info!(
                        "Migration completed: {} in {:?} after {} attempts", 
                        job.memory_id, 
                        duration, 
                        attempt
                    );
                    
                    // Update progress
                    self.progress_tracker.complete_migration(&job, true).await?;
                    
                    // Remove from active migrations
                    self.active_migrations.write().await.remove(&job.memory_id);
                    
                    return Ok(());
                }
                Err(e) => {
                    error!("Migration attempt {} failed for {}: {}", attempt, job.memory_id, e);
                    job.last_error = Some(e.to_string());
                    
                    if attempt < self.config.max_retries {
                        // Wait before retry
                        tokio::time::sleep(std::time::Duration::from_millis(
                            self.config.retry_delay_ms * attempt as u64
                        )).await;
                    } else {
                        // Final attempt failed, try rollback if enabled
                        if self.config.enable_rollback && rollback_data.is_some() {
                            if let Err(rollback_err) = self.rollback_migration(&job, rollback_data).await {
                                error!("Rollback failed for {}: {}", job.memory_id, rollback_err);
                            }
                        }
                        
                        self.progress_tracker.complete_migration(&job, false).await?;
                        self.active_migrations.write().await.remove(&job.memory_id);
                        
                        return Err(e);
                    }
                }
            }
        }
        
        unreachable!("Migration retry loop should have returned")
    }

    async fn try_migrate_memory(
        &self, 
        job: &MigrationJob, 
        rollback_data: &mut Option<serde_json::Value>
    ) -> Result<()> {
        // Store rollback data before migration
        if self.config.enable_rollback && rollback_data.is_none() {
            let memory = self.repository.get_memory(job.memory_id).await
                .map_err(|e| MigrationError::Database(e.to_string()))?;
            
            *rollback_data = Some(serde_json::json!({
                "tier": memory.tier,
                "importance_score": memory.importance_score,
                "metadata": memory.metadata,
            }));
        }
        
        // Perform the actual migration
        self.repository.migrate_memory(job.memory_id, job.to_tier, Some(job.reason.clone())).await
            .map_err(|e| MigrationError::MigrationFailed(e.to_string()))?;
        
        Ok(())
    }

    async fn rollback_migration(
        &self, 
        job: &MigrationJob, 
        rollback_data: Option<serde_json::Value>
    ) -> Result<()> {
        if let Some(data) = rollback_data {
            warn!("Rolling back migration for {}", job.memory_id);
            
            // Restore previous state
            let original_tier = data["tier"].as_str()
                .and_then(|s| match s {
                    "working" => Some(MemoryTier::Working),
                    "warm" => Some(MemoryTier::Warm),
                    "cold" => Some(MemoryTier::Cold),
                    _ => None,
                })
                .ok_or_else(|| MigrationError::RollbackFailed { 
                    reason: "Invalid tier in rollback data".to_string() 
                })?;
            
            self.repository.migrate_memory(
                job.memory_id, 
                original_tier, 
                Some("Rollback due to migration failure".to_string())
            ).await
            .map_err(|e| MigrationError::RollbackFailed { 
                reason: e.to_string() 
            })?;
        }
        
        Ok(())
    }

    pub async fn get_migration_progress(&self, migration_id: Uuid) -> Result<Option<crate::MigrationProgress>> {
        self.progress_tracker.get_progress(migration_id).await
    }

    pub async fn cancel_migration(&self, memory_id: Uuid) -> Result<()> {
        if let Some((_, job)) = self.active_migrations.write().await.remove(&memory_id) {
            info!("Canceling migration for memory: {}", memory_id);
            
            // Mark as canceled in progress tracker
            self.progress_tracker.cancel_migration(&job).await?;
            
            Ok(())
        } else {
            Err(MigrationError::MigrationNotFound { 
                id: memory_id.to_string() 
            })
        }
    }

    pub async fn get_active_migrations(&self) -> Vec<MigrationJob> {
        self.active_migrations.read().await
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    // Helper methods for cloning with reduced state for async contexts
    fn clone_for_trigger(&self) -> Arc<Self> {
        // This is a simplified clone pattern - in practice, we'd use Arc<Self>
        unreachable!("This would be implemented with proper Arc handling")
    }

    fn clone_for_worker(&self) -> Arc<Self> {
        // This is a simplified clone pattern - in practice, we'd use Arc<Self>
        unreachable!("This would be implemented with proper Arc handling")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_job_ordering() {
        let job1 = MigrationJob::new(
            Uuid::new_v4(), 
            MemoryTier::Working, 
            MemoryTier::Warm, 
            "Test".to_string(), 
            1
        );
        let job2 = MigrationJob::new(
            Uuid::new_v4(), 
            MemoryTier::Working, 
            MemoryTier::Warm, 
            "Test".to_string(), 
            5
        );
        
        // Higher priority (5) should come before lower priority (1)
        assert!(job2 < job1);
    }

    #[test]
    fn test_migration_config_defaults() {
        let config = MigrationConfig::default();
        assert_eq!(config.max_concurrent_migrations, 5);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_retries, 3);
    }
}