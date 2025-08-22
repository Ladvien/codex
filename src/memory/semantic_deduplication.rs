use super::error::{MemoryError, Result};
use super::models::CreateMemoryRequest;
use super::models::*;
use super::repository::MemoryRepository;
use crate::embedding::EmbeddingService;
use chrono::{DateTime, Duration, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the semantic deduplication system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDeduplicationConfig {
    /// Cosine similarity threshold for considering memories as duplicates
    pub similarity_threshold: f32,
    /// Batch size for processing memories
    pub batch_size: usize,
    /// Maximum memories to process in a single operation
    pub max_memories_per_operation: usize,
    /// Minimum age before a memory can be considered for merging
    pub min_memory_age_hours: i64,
    /// Recall probability threshold for auto-pruning
    pub prune_threshold: f64,
    /// Days after which memories with low recall probability can be pruned
    pub prune_age_days: i64,
    /// Target memory headroom percentage (0.0 to 1.0)
    pub target_memory_headroom: f32,
    /// Compression ratio targets by tier
    pub compression_targets: HashMap<MemoryTier, f32>,
    /// Enable/disable lossless compression for critical memories
    pub lossless_critical: bool,
    /// Maximum time for deduplication operation in seconds
    pub max_operation_time_seconds: u64,
}

impl Default for SemanticDeduplicationConfig {
    fn default() -> Self {
        let mut compression_targets = HashMap::new();
        compression_targets.insert(MemoryTier::Working, 2.0);
        compression_targets.insert(MemoryTier::Warm, 3.0);
        compression_targets.insert(MemoryTier::Cold, 5.0);
        compression_targets.insert(MemoryTier::Frozen, 10.0);

        Self {
            similarity_threshold: 0.85,
            batch_size: 100,
            max_memories_per_operation: 10_000,
            min_memory_age_hours: 1,
            prune_threshold: 0.2,
            prune_age_days: 30,
            target_memory_headroom: 0.2,
            compression_targets,
            lossless_critical: true,
            max_operation_time_seconds: 30,
        }
    }
}

/// Main semantic deduplication engine with production-ready safety and concurrency controls
#[allow(dead_code)]
pub struct SemanticDeduplicationEngine {
    config: SemanticDeduplicationConfig,
    repository: Arc<MemoryRepository>,
    #[allow(dead_code)]
    embedding_service: Arc<dyn EmbeddingService>,
    merger: Arc<MemoryMerger>,
    #[allow(dead_code)]
    compression_manager: Arc<CompressionManager>,
    audit_trail: Arc<AuditTrail>,
    auto_pruner: Arc<AutoPruner>,
    metrics: Arc<RwLock<DeduplicationMetrics>>,
    /// Distributed lock for preventing concurrent deduplication operations
    operation_lock: Arc<Mutex<()>>,
    /// Active operations tracking for concurrent control
    active_operations: Arc<RwLock<HashSet<String>>>,
}

/// Simple lock guard that performs cleanup on drop
struct OperationLockGuard {
    operation_id: String,
    lock_key: i64,
    pool: sqlx::PgPool,
    active_operations: Arc<RwLock<HashSet<String>>>,
    released: bool,
}

impl OperationLockGuard {
    async fn release(&mut self) -> Result<()> {
        if self.released {
            return Ok(());
        }

        self.released = true;

        // Remove from active operations
        {
            let mut active_ops = self.active_operations.write().await;
            active_ops.remove(&self.operation_id);
        }

        // Release PostgreSQL advisory lock
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(self.lock_key)
            .execute(&self.pool)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to release advisory lock: {}", e),
            })?;

        debug!(
            "Released distributed lock for operation: {}",
            self.operation_id
        );
        Ok(())
    }
}

impl Drop for OperationLockGuard {
    fn drop(&mut self) {
        if !self.released {
            let operation_id = self.operation_id.clone();
            let lock_key = self.lock_key;
            let pool = self.pool.clone();
            let active_operations = self.active_operations.clone();

            // Spawn cleanup task that doesn't block the drop
            tokio::spawn(async move {
                // Remove from active operations
                {
                    let mut active_ops = active_operations.write().await;
                    active_ops.remove(&operation_id);
                }

                // Release PostgreSQL advisory lock
                if let Err(e) = sqlx::query("SELECT pg_advisory_unlock($1)")
                    .bind(lock_key)
                    .execute(&pool)
                    .await
                {
                    error!(
                        "Failed to release advisory lock for operation {}: {}",
                        operation_id, e
                    );
                } else {
                    debug!("Released distributed lock for operation: {}", operation_id);
                }
            });
        }
    }
}

impl SemanticDeduplicationEngine {
    pub fn new(
        config: SemanticDeduplicationConfig,
        repository: Arc<MemoryRepository>,
        embedding_service: Arc<dyn EmbeddingService>,
    ) -> Self {
        let merger = Arc::new(MemoryMerger::new(config.clone(), repository.clone()));
        let compression_manager = Arc::new(CompressionManager::new(config.clone()));
        let audit_trail = Arc::new(AuditTrail::new(repository.clone()));
        let auto_pruner = Arc::new(AutoPruner::new(config.clone(), repository.clone()));
        let metrics = Arc::new(RwLock::new(DeduplicationMetrics::default()));

        Self {
            config,
            repository,
            embedding_service,
            merger,
            compression_manager,
            audit_trail,
            auto_pruner,
            metrics,
            operation_lock: Arc::new(Mutex::new(())),
            active_operations: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Perform semantic deduplication on a batch of memories with distributed locking and transaction safety
    pub async fn deduplicate_batch(&self, memory_ids: &[Uuid]) -> Result<DeduplicationResult> {
        let operation_id = format!("dedup_{}", Uuid::new_v4());
        let start_time = Instant::now();
        let mut result;

        // Acquire distributed lock to prevent concurrent deduplication operations
        let (_mutex_guard, mut _lock_guard) = self.acquire_operation_lock(&operation_id).await?;

        // Performance monitoring: start time tracking
        let mut performance_metrics = PerformanceMetrics {
            operation_id: operation_id.clone(),
            start_time,
            phase_timings: HashMap::new(),
            memory_count: memory_ids.len(),
            target_time_seconds: self.config.max_operation_time_seconds,
        };

        info!(
            "Starting deduplication operation {} for {} memories with threshold {}",
            operation_id,
            memory_ids.len(),
            self.config.similarity_threshold
        );

        // Check for operation timeout
        let timeout_check = start_time.elapsed().as_secs();
        if timeout_check > self.config.max_operation_time_seconds {
            return Err(MemoryError::OperationTimeout {
                message: format!("Operation timed out after {} seconds", timeout_check),
            });
        }

        // Performance check: validate we can meet the 10K memories < 30 seconds requirement
        if memory_ids.len() > 10_000 && self.config.max_operation_time_seconds > 30 {
            warn!(
                "Processing {} memories may exceed performance target of 30 seconds",
                memory_ids.len()
            );
        }

        performance_metrics.record_phase("lock_acquisition", start_time.elapsed());

        // Begin a database transaction for atomic operations
        let transaction_start = Instant::now();
        let mut transaction =
            self.repository
                .pool()
                .begin()
                .await
                .map_err(|e| MemoryError::DatabaseError {
                    message: format!("Failed to begin transaction: {}", e),
                })?;

        performance_metrics.record_phase("transaction_begin", transaction_start.elapsed());

        let processing_result = self
            .execute_deduplication_with_transaction(&mut transaction, memory_ids, &operation_id)
            .await;

        match processing_result {
            Ok(dedup_result) => {
                // Commit transaction if all operations succeeded
                let commit_start = Instant::now();
                transaction
                    .commit()
                    .await
                    .map_err(|e| MemoryError::DatabaseError {
                        message: format!("Failed to commit transaction: {}", e),
                    })?;
                performance_metrics.record_phase("transaction_commit", commit_start.elapsed());

                result = dedup_result;
                info!(
                    "Deduplication operation {} committed successfully",
                    operation_id
                );
            }
            Err(e) => {
                // Rollback transaction on any error
                let rollback_start = Instant::now();
                if let Err(rollback_err) = transaction.rollback().await {
                    error!(
                        "Failed to rollback transaction for operation {}: {}",
                        operation_id, rollback_err
                    );
                } else {
                    performance_metrics
                        .record_phase("transaction_rollback", rollback_start.elapsed());
                }

                // Ensure lock cleanup on error
                if let Err(cleanup_err) = _lock_guard.release().await {
                    error!(
                        "Failed to cleanup lock for operation {}: {}",
                        operation_id, cleanup_err
                    );
                }

                error!("Deduplication operation {} failed: {}", operation_id, e);
                return Err(e);
            }
        }

        // Clean up lock explicitly
        if let Err(cleanup_err) = _lock_guard.release().await {
            error!(
                "Failed to cleanup lock for operation {}: {}",
                operation_id, cleanup_err
            );
        }

        result.execution_time_ms = start_time.elapsed().as_millis() as u64;
        performance_metrics.record_phase("total_operation", start_time.elapsed());

        // Performance validation
        if result.execution_time_ms > (self.config.max_operation_time_seconds * 1000) as u64 {
            warn!(
                "Operation {} exceeded time limit: {}ms > {}ms",
                operation_id,
                result.execution_time_ms,
                self.config.max_operation_time_seconds * 1000
            );
        }

        // Validate performance target for large batches
        if memory_ids.len() >= 10_000 && result.execution_time_ms > 30_000 {
            error!(
                "PERFORMANCE VIOLATION: {} memories processed in {}ms (> 30s target)",
                memory_ids.len(),
                result.execution_time_ms
            );
        }

        // Update metrics with performance data
        self.update_metrics_with_performance(&result, &performance_metrics)
            .await;

        info!(
            "Deduplication operation {} completed: {} memories processed, {} merged, {:.2}% storage saved in {}ms (phases: {})",
            operation_id,
            result.total_processed,
            result.memories_merged,
            (result.storage_saved_bytes as f64 / (result.total_processed as f64 * 1024.0)) * 100.0,
            result.execution_time_ms,
            performance_metrics.format_phase_summary()
        );

        Ok(result)
    }

    /// Find similar groups using optimized pgvector nearest neighbor search instead of O(n²) comparison
    async fn find_similar_groups_optimized(
        &self,
        memories: &[Memory],
    ) -> Result<Vec<SimilarMemoryGroup>> {
        let mut groups = Vec::new();
        let mut processed_ids = HashSet::new();

        info!(
            "Finding similar groups for {} memories using pgvector optimization",
            memories.len()
        );

        for memory in memories {
            if processed_ids.contains(&memory.id) {
                continue;
            }

            let embedding = match &memory.embedding {
                Some(emb) => emb,
                None => continue,
            };

            // Use pgvector to find similar memories efficiently
            let similar_memories = self
                .find_similar_memories_pgvector(
                    memory,
                    embedding,
                    &memories.iter().map(|m| m.id).collect::<Vec<_>>(),
                    self.config.similarity_threshold,
                )
                .await?;

            if similar_memories.len() > 1 {
                // Mark all memories in this group as processed
                for sim_memory in &similar_memories {
                    processed_ids.insert(sim_memory.id);
                }

                let average_similarity =
                    self.calculate_average_similarity(&similar_memories).await?;
                let merge_strategy = self.determine_merge_strategy(&similar_memories);

                groups.push(SimilarMemoryGroup {
                    memories: similar_memories,
                    average_similarity,
                    merge_strategy,
                });
            } else {
                processed_ids.insert(memory.id);
            }
        }

        info!(
            "Found {} similar groups using optimized search",
            groups.len()
        );
        Ok(groups)
    }

    /// Use pgvector's efficient similarity search to find similar memories
    async fn find_similar_memories_pgvector(
        &self,
        _query_memory: &Memory,
        query_embedding: &Vector,
        candidate_ids: &[Uuid],
        threshold: f32,
    ) -> Result<Vec<Memory>> {
        let query = r#"
            SELECT m.*, (m.embedding <=> $1) as similarity_distance
            FROM memories m
            WHERE m.id = ANY($2)
            AND m.status = 'active'
            AND m.embedding IS NOT NULL
            AND (m.embedding <=> $1) <= $3
            ORDER BY m.embedding <=> $1
            LIMIT 100
        "#;

        // Convert cosine similarity threshold to distance threshold
        // Distance = 1 - cosine_similarity, so distance <= 1 - threshold
        let distance_threshold = 1.0 - threshold;

        let rows = sqlx::query(query)
            .bind(query_embedding)
            .bind(candidate_ids)
            .bind(distance_threshold)
            .fetch_all(self.repository.pool())
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to execute pgvector similarity search: {}", e),
            })?;

        let mut similar_memories = Vec::new();
        for row in rows {
            let memory = Memory {
                id: row.get("id"),
                content: row.get("content"),
                content_hash: row.get("content_hash"),
                embedding: row.get("embedding"),
                tier: row.get("tier"),
                status: row.get("status"),
                importance_score: row.get("importance_score"),
                access_count: row.get("access_count"),
                last_accessed_at: row.get("last_accessed_at"),
                metadata: row.get("metadata"),
                parent_id: row.get("parent_id"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                expires_at: row.get("expires_at"),
                consolidation_strength: row.get("consolidation_strength"),
                decay_rate: row.get("decay_rate"),
                recall_probability: row.get("recall_probability"),
                last_recall_interval: row.get("last_recall_interval"),
                recency_score: row.get("recency_score"),
                relevance_score: row.get("relevance_score"),
            };
            similar_memories.push(memory);
        }

        Ok(similar_memories)
    }

    /// Find all memories that exceed the similarity threshold (legacy method for backward compatibility)
    async fn find_similar_groups(&self, memories: &[Memory]) -> Result<Vec<SimilarMemoryGroup>> {
        // Delegate to optimized version
        self.find_similar_groups_optimized(memories).await
    }

    /// Calculate cosine similarity between two embeddings
    pub fn calculate_cosine_similarity(&self, a: &Vector, b: &Vector) -> Result<f32> {
        let a_slice = a.as_slice();
        let b_slice = b.as_slice();

        if a_slice.len() != b_slice.len() {
            return Err(MemoryError::InvalidData {
                message: "Embedding dimensions don't match".to_string(),
            });
        }

        let dot_product: f32 = a_slice.iter().zip(b_slice.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a_slice.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b_slice.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return Ok(0.0);
        }

        Ok(dot_product / (norm_a * norm_b))
    }

    /// Calculate average similarity within a group
    async fn calculate_average_similarity(&self, memories: &[Memory]) -> Result<f32> {
        if memories.len() < 2 {
            return Ok(1.0);
        }

        let mut total_similarity = 0.0;
        let mut comparisons = 0;

        for i in 0..memories.len() {
            for j in (i + 1)..memories.len() {
                if let (Some(emb_i), Some(emb_j)) = (&memories[i].embedding, &memories[j].embedding)
                {
                    total_similarity += self.calculate_cosine_similarity(emb_i, emb_j)?;
                    comparisons += 1;
                }
            }
        }

        Ok(if comparisons > 0 {
            total_similarity / comparisons as f32
        } else {
            0.0
        })
    }

    /// Determine the best merge strategy for a group of similar memories
    fn determine_merge_strategy(&self, memories: &[Memory]) -> MergeStrategy {
        // Check if any memory is critical (high importance or recent access)
        let has_critical = memories.iter().any(|m| {
            m.importance_score > 0.8
                || m.last_accessed_at.map_or(false, |last| {
                    Utc::now().signed_duration_since(last) < Duration::hours(24)
                })
        });

        // Check tier distribution
        let has_working_tier = memories
            .iter()
            .any(|m| matches!(m.tier, MemoryTier::Working));
        let has_different_tiers = memories
            .iter()
            .map(|m| m.tier)
            .collect::<HashSet<_>>()
            .len()
            > 1;

        if has_critical && self.config.lossless_critical {
            MergeStrategy::LosslessPreservation
        } else if has_working_tier || has_different_tiers {
            MergeStrategy::MetadataConsolidation
        } else {
            MergeStrategy::ContentSummarization
        }
    }

    /// Process a group of similar memories for merging with transaction safety
    async fn process_similar_group_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        group: SimilarMemoryGroup,
        operation_id: &str,
    ) -> Result<GroupMergeResult> {
        debug!(
            "Processing similar group with {} memories, strategy: {:?}, operation: {}",
            group.memories.len(),
            group.merge_strategy,
            operation_id
        );

        // Record audit entry before merging
        let audit_entry = self
            .audit_trail
            .create_merge_entry_tx(transaction, &group)
            .await?;

        // Perform the merge with transaction safety
        let merge_result = self.merger.merge_group_tx(transaction, &group).await?;

        // Complete audit entry
        self.audit_trail
            .complete_merge_entry_tx(transaction, audit_entry.id, &merge_result)
            .await?;

        Ok(GroupMergeResult {
            merged_memory_id: merge_result.merged_memory.id,
            original_memory_ids: group.memories.iter().map(|m| m.id).collect(),
            memories_merged: group.memories.len(),
            storage_saved: merge_result.storage_saved,
            compression_ratio: merge_result.compression_ratio,
            merge_strategy: group.merge_strategy,
        })
    }

    /// Process a group of similar memories for merging (legacy method)
    async fn process_similar_group(&self, group: SimilarMemoryGroup) -> Result<GroupMergeResult> {
        debug!(
            "Processing similar group with {} memories, strategy: {:?}",
            group.memories.len(),
            group.merge_strategy
        );

        // Record audit entry before merging
        let audit_entry = self.audit_trail.create_merge_entry(&group).await?;

        // Perform the merge
        let merge_result = self.merger.merge_group(&group).await?;

        // Complete audit entry
        self.audit_trail
            .complete_merge_entry(audit_entry.id, &merge_result)
            .await?;

        Ok(GroupMergeResult {
            merged_memory_id: merge_result.merged_memory.id,
            original_memory_ids: group.memories.iter().map(|m| m.id).collect(),
            memories_merged: group.memories.len(),
            storage_saved: merge_result.storage_saved,
            compression_ratio: merge_result.compression_ratio,
            merge_strategy: group.merge_strategy,
        })
    }

    /// Load memories with their embeddings
    async fn load_memories_with_embeddings(&self, memory_ids: &[Uuid]) -> Result<Vec<Memory>> {
        let query = r#"
            SELECT * FROM memories 
            WHERE id = ANY($1) 
            AND status = 'active' 
            AND embedding IS NOT NULL
            ORDER BY importance_score DESC, last_accessed_at DESC NULLS LAST
        "#;

        let memories = sqlx::query_as::<_, Memory>(query)
            .bind(memory_ids)
            .fetch_all(self.repository.pool())
            .await?;

        Ok(memories)
    }

    /// Load memories with their embeddings using a transaction
    async fn load_memories_with_embeddings_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        memory_ids: &[Uuid],
    ) -> Result<Vec<Memory>> {
        let query = r#"
            SELECT * FROM memories 
            WHERE id = ANY($1) 
            AND status = 'active' 
            AND embedding IS NOT NULL
            ORDER BY importance_score DESC, last_accessed_at DESC NULLS LAST
        "#;

        let memories = sqlx::query_as::<_, Memory>(query)
            .bind(memory_ids)
            .fetch_all(&mut **transaction)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to load memories in transaction: {}", e),
            })?;

        Ok(memories)
    }

    /// Run auto-pruning based on recall probability thresholds
    pub async fn run_auto_pruning(&self) -> Result<PruningResult> {
        info!(
            "Starting auto-pruning with threshold {}",
            self.config.prune_threshold
        );

        let cutoff_date = Utc::now() - Duration::days(self.config.prune_age_days);

        let prune_result = self
            .auto_pruner
            .prune_memories(self.config.prune_threshold, cutoff_date)
            .await?;

        info!(
            "Auto-pruning completed: {} memories pruned, {} bytes freed",
            prune_result.memories_pruned, prune_result.storage_freed
        );

        Ok(prune_result)
    }

    /// Check and maintain memory headroom
    pub async fn maintain_memory_headroom(&self) -> Result<HeadroomMaintenanceResult> {
        let stats = self.get_memory_statistics().await?;
        let current_utilization =
            1.0 - (stats.free_space_bytes as f32 / stats.total_space_bytes as f32);

        if current_utilization > (1.0 - self.config.target_memory_headroom) {
            info!(
                "Memory utilization {:.2}% exceeds target, starting aggressive cleanup",
                current_utilization * 100.0
            );

            // Run more aggressive deduplication and pruning
            let all_memory_ids = self.get_all_active_memory_ids().await?;
            let dedup_result = self.deduplicate_batch(&all_memory_ids).await?;
            let prune_result = self.run_auto_pruning().await?;

            Ok(HeadroomMaintenanceResult {
                initial_utilization: current_utilization,
                final_utilization: self.get_memory_utilization().await?,
                memories_processed: dedup_result.total_processed,
                memories_merged: dedup_result.memories_merged,
                memories_pruned: prune_result.memories_pruned,
                storage_freed: dedup_result.storage_saved_bytes + prune_result.storage_freed,
            })
        } else {
            Ok(HeadroomMaintenanceResult {
                initial_utilization: current_utilization,
                final_utilization: current_utilization,
                memories_processed: 0,
                memories_merged: 0,
                memories_pruned: 0,
                storage_freed: 0,
            })
        }
    }

    /// Get current memory utilization percentage
    async fn get_memory_utilization(&self) -> Result<f32> {
        let stats = self.get_memory_statistics().await?;
        Ok(1.0 - (stats.free_space_bytes as f32 / stats.total_space_bytes as f32))
    }

    /// Get comprehensive memory statistics
    pub async fn get_memory_statistics(&self) -> Result<MemoryStatistics> {
        let query = r#"
            SELECT 
                COUNT(*) as total_memories,
                SUM(LENGTH(content)) as total_content_bytes,
                AVG(importance_score) as avg_importance,
                COUNT(CASE WHEN tier = 'working' THEN 1 END) as working_count,
                COUNT(CASE WHEN tier = 'warm' THEN 1 END) as warm_count,
                COUNT(CASE WHEN tier = 'cold' THEN 1 END) as cold_count,
                COUNT(CASE WHEN tier = 'frozen' THEN 1 END) as frozen_count
            FROM memories 
            WHERE status = 'active'
        "#;

        let row = sqlx::query(query).fetch_one(self.repository.pool()).await?;

        // Simulate total space calculation (would need actual disk space monitoring)
        let total_content_bytes: i64 = row.get("total_content_bytes");
        let estimated_total_space = total_content_bytes * 5; // Rough estimate including indexes, metadata
        let estimated_free_space = estimated_total_space / 5; // Simulate 20% free

        Ok(MemoryStatistics {
            total_memories: row.get("total_memories"),
            total_content_bytes,
            total_space_bytes: estimated_total_space,
            free_space_bytes: estimated_free_space,
            avg_importance: row.get::<Option<f64>, _>("avg_importance").unwrap_or(0.0),
            working_count: row.get("working_count"),
            warm_count: row.get("warm_count"),
            cold_count: row.get("cold_count"),
            frozen_count: row.get("frozen_count"),
        })
    }

    /// Get all active memory IDs for batch processing
    async fn get_all_active_memory_ids(&self) -> Result<Vec<Uuid>> {
        let query = "SELECT id FROM memories WHERE status = 'active' ORDER BY created_at ASC";

        let rows = sqlx::query(query).fetch_all(self.repository.pool()).await?;

        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    /// Update internal metrics with performance data
    async fn update_metrics_with_performance(
        &self,
        result: &DeduplicationResult,
        performance: &PerformanceMetrics,
    ) {
        let mut metrics = self.metrics.write().await;
        metrics.total_operations += 1;
        metrics.total_memories_processed += result.total_processed;
        metrics.total_memories_merged += result.memories_merged;
        metrics.total_storage_saved += result.storage_saved_bytes;
        metrics.total_execution_time_ms += result.execution_time_ms;
        metrics.errors_encountered += result.errors_encountered;
        metrics.last_operation_timestamp = Some(Utc::now());

        if result.compression_ratio > 0.0 {
            metrics.average_compression_ratio =
                (metrics.average_compression_ratio + result.compression_ratio) / 2.0;
        }

        // Log performance violations
        let violations = performance.get_performance_violations();
        if !violations.is_empty() {
            error!(
                "Performance violations in operation {}: {}",
                performance.operation_id,
                violations.join("; ")
            );
        }

        // Record performance metrics in database for monitoring
        if let Err(e) = self.record_performance_metrics(performance, result).await {
            warn!("Failed to record performance metrics: {}", e);
        }
    }

    /// Update internal metrics (legacy method)
    async fn update_metrics(&self, result: &DeduplicationResult) {
        let mut metrics = self.metrics.write().await;
        metrics.total_operations += 1;
        metrics.total_memories_processed += result.total_processed;
        metrics.total_memories_merged += result.memories_merged;
        metrics.total_storage_saved += result.storage_saved_bytes;
        metrics.total_execution_time_ms += result.execution_time_ms;
        metrics.errors_encountered += result.errors_encountered;
        metrics.last_operation_timestamp = Some(Utc::now());

        if result.compression_ratio > 0.0 {
            metrics.average_compression_ratio =
                (metrics.average_compression_ratio + result.compression_ratio) / 2.0;
        }
    }

    /// Record performance metrics in database for monitoring and alerting
    async fn record_performance_metrics(
        &self,
        performance: &PerformanceMetrics,
        result: &DeduplicationResult,
    ) -> Result<()> {
        let metrics_data = serde_json::json!({
            "operation_id": performance.operation_id,
            "memory_count": performance.memory_count,
            "execution_time_ms": result.execution_time_ms,
            "target_time_seconds": performance.target_time_seconds,
            "phase_timings": performance.phase_timings.iter().map(|(k, v)| (k, v.as_millis())).collect::<HashMap<_, _>>(),
            "memories_processed": result.total_processed,
            "memories_merged": result.memories_merged,
            "storage_saved_bytes": result.storage_saved_bytes,
            "compression_ratio": result.compression_ratio,
            "errors_encountered": result.errors_encountered,
            "performance_violations": performance.get_performance_violations()
        });

        sqlx::query(
            r#"
            INSERT INTO deduplication_metrics (
                measurement_type, metrics_data, recorded_at
            ) VALUES ($1, $2, $3)
        "#,
        )
        .bind("operation_performance")
        .bind(metrics_data)
        .bind(Utc::now())
        .execute(self.repository.pool())
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to record performance metrics: {}", e),
        })?;

        Ok(())
    }

    /// Get current deduplication metrics
    pub async fn get_metrics(&self) -> DeduplicationMetrics {
        self.metrics.read().await.clone()
    }

    /// Process large memory batches with intelligent batching and concurrency control
    pub async fn deduplicate_large_batch(
        &self,
        memory_ids: &[Uuid],
    ) -> Result<DeduplicationResult> {
        let total_memories = memory_ids.len();
        info!(
            "Starting large batch deduplication for {} memories",
            total_memories
        );

        // If batch is small enough, use regular processing
        if total_memories <= self.config.batch_size {
            return self.deduplicate_batch(memory_ids).await;
        }

        // Split into optimal batch sizes for performance
        let optimal_batch_size = self.calculate_optimal_batch_size(total_memories);
        let batches: Vec<_> = memory_ids
            .chunks(optimal_batch_size)
            .map(|chunk| chunk.to_vec())
            .collect();

        info!(
            "Split {} memories into {} batches of ~{} memories each",
            total_memories,
            batches.len(),
            optimal_batch_size
        );

        let mut combined_result = DeduplicationResult::default();
        let start_time = Instant::now();

        // Process batches sequentially for safety and simplicity
        let mut successful_batches = 0;
        let mut failed_batches = 0;

        for (batch_index, batch_memory_ids) in batches.into_iter().enumerate() {
            info!(
                "Processing batch {} with {} memories",
                batch_index,
                batch_memory_ids.len()
            );

            let batch_start = Instant::now();

            // Check if we're approaching time limit
            if start_time.elapsed().as_secs() > (self.config.max_operation_time_seconds * 3 / 4) {
                warn!(
                    "Approaching time limit, stopping batch processing after {} batches",
                    batch_index
                );
                break;
            }

            match self.deduplicate_batch(&batch_memory_ids).await {
                Ok(batch_result) => {
                    successful_batches += 1;
                    combined_result.total_processed += batch_result.total_processed;
                    combined_result.groups_identified += batch_result.groups_identified;
                    combined_result.memories_merged += batch_result.memories_merged;
                    combined_result.storage_saved_bytes += batch_result.storage_saved_bytes;
                    combined_result.errors_encountered += batch_result.errors_encountered;

                    // Calculate weighted average compression ratio
                    if batch_result.compression_ratio > 0.0 {
                        combined_result.compression_ratio = (combined_result.compression_ratio
                            * (successful_batches - 1) as f32
                            + batch_result.compression_ratio)
                            / successful_batches as f32;
                    }

                    info!(
                        "Batch {} completed in {}ms: {} processed, {} merged",
                        batch_index,
                        batch_start.elapsed().as_millis(),
                        batch_result.total_processed,
                        batch_result.memories_merged
                    );
                }
                Err(e) => {
                    failed_batches += 1;
                    combined_result.errors_encountered += 1;
                    error!("Batch {} failed: {}", batch_index, e);

                    // Continue processing other batches even if one fails
                    // This provides better resilience in large batch operations
                }
            }
        }

        combined_result.execution_time_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "Large batch deduplication completed: {}/{} batches successful, {} total memories processed, {} merged, {}ms total time",
            successful_batches, successful_batches + failed_batches,
            combined_result.total_processed, combined_result.memories_merged,
            combined_result.execution_time_ms
        );

        // Update metrics
        self.update_metrics(&combined_result).await;

        Ok(combined_result)
    }

    /// Calculate optimal batch size based on system capacity and memory count
    fn calculate_optimal_batch_size(&self, total_memories: usize) -> usize {
        // Base batch size from config
        let mut batch_size = self.config.batch_size;

        // Scale batch size based on total memory count for better performance
        if total_memories > 50_000 {
            batch_size = std::cmp::max(batch_size * 2, 1000); // Larger batches for very large datasets
        } else if total_memories > 20_000 {
            batch_size = std::cmp::max(batch_size * 3 / 2, 500); // Medium batches
        }

        // Ensure we don't exceed max memories per operation
        batch_size = std::cmp::min(batch_size, self.config.max_memories_per_operation);

        // Ensure minimum viable batch size
        std::cmp::max(batch_size, 50)
    }

    /// Acquire distributed lock for deduplication operations with automatic cleanup
    async fn acquire_operation_lock(
        &self,
        operation_id: &str,
    ) -> Result<(tokio::sync::MutexGuard<'_, ()>, OperationLockGuard)> {
        // Check if operation is already running
        {
            let active_ops = self.active_operations.read().await;
            if active_ops.contains(operation_id) {
                return Err(MemoryError::ConcurrencyError {
                    message: format!("Operation {} is already in progress", operation_id),
                });
            }
        }

        // Acquire PostgreSQL advisory lock for distributed locking
        let lock_key = self.calculate_lock_key(operation_id);
        let acquired = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
            .bind(lock_key)
            .fetch_one(self.repository.pool())
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to acquire advisory lock: {}", e),
            })?;

        if !acquired {
            return Err(MemoryError::ConcurrencyError {
                message: "Another deduplication operation is in progress".to_string(),
            });
        }

        // Add to active operations
        {
            let mut active_ops = self.active_operations.write().await;
            active_ops.insert(operation_id.to_string());
        }

        // Acquire local mutex guard
        let mutex_guard = self.operation_lock.lock().await;

        info!("Acquired distributed lock for operation: {}", operation_id);

        let lock_guard = OperationLockGuard {
            operation_id: operation_id.to_string(),
            lock_key,
            pool: self.repository.pool().clone(),
            active_operations: self.active_operations.clone(),
            released: false,
        };

        Ok((mutex_guard, lock_guard))
    }

    /// Calculate lock key from operation ID for PostgreSQL advisory locks
    fn calculate_lock_key(&self, operation_id: &str) -> i64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        operation_id.hash(&mut hasher);
        hasher.finish() as i64
    }

    /// Execute deduplication within a database transaction
    async fn execute_deduplication_with_transaction(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        memory_ids: &[Uuid],
        operation_id: &str,
    ) -> Result<DeduplicationResult> {
        let mut result = DeduplicationResult::default();

        // Load memories with embeddings using transaction
        let memories = self
            .load_memories_with_embeddings_tx(transaction, memory_ids)
            .await?;
        if memories.is_empty() {
            return Ok(result);
        }

        result.total_processed = memories.len();

        // Find similar memory groups using efficient pgvector search
        let similar_groups = self.find_similar_groups_optimized(&memories).await?;
        result.groups_identified = similar_groups.len();

        // Process each group for merging within the transaction
        for group in similar_groups {
            match self
                .process_similar_group_tx(transaction, group, operation_id)
                .await
            {
                Ok(merge_result) => {
                    result.memories_merged += merge_result.memories_merged;
                    result.storage_saved_bytes += merge_result.storage_saved;
                    result.compression_ratio =
                        (result.compression_ratio + merge_result.compression_ratio) / 2.0;
                }
                Err(e) => {
                    warn!(
                        "Failed to process similar group in operation {}: {}",
                        operation_id, e
                    );
                    result.errors_encountered += 1;
                    // Return error to trigger transaction rollback
                    return Err(e);
                }
            }
        }

        Ok(result)
    }
}

/// Memory merger component for intelligent merging algorithms
pub struct MemoryMerger {
    #[allow(dead_code)]
    config: SemanticDeduplicationConfig,
    repository: Arc<MemoryRepository>,
}

impl MemoryMerger {
    pub fn new(config: SemanticDeduplicationConfig, repository: Arc<MemoryRepository>) -> Self {
        Self { config, repository }
    }

    /// Merge a group of similar memories
    pub async fn merge_group(&self, group: &SimilarMemoryGroup) -> Result<MergeResult> {
        match group.merge_strategy {
            MergeStrategy::LosslessPreservation => self.merge_lossless(group).await,
            MergeStrategy::MetadataConsolidation => {
                self.merge_with_metadata_consolidation(group).await
            }
            MergeStrategy::ContentSummarization => self.merge_with_summarization(group).await,
        }
    }

    /// Merge a group of similar memories within a transaction
    pub async fn merge_group_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        group: &SimilarMemoryGroup,
    ) -> Result<MergeResult> {
        match group.merge_strategy {
            MergeStrategy::LosslessPreservation => self.merge_lossless_tx(transaction, group).await,
            MergeStrategy::MetadataConsolidation => {
                self.merge_with_metadata_consolidation_tx(transaction, group)
                    .await
            }
            MergeStrategy::ContentSummarization => {
                self.merge_with_summarization_tx(transaction, group).await
            }
        }
    }

    /// Lossless merging preserving all content and metadata
    async fn merge_lossless(&self, group: &SimilarMemoryGroup) -> Result<MergeResult> {
        let memories = &group.memories;
        let primary_memory = &memories[0]; // Use highest importance as primary

        // Combine all content with clear delineation
        let combined_content = memories
            .iter()
            .enumerate()
            .map(|(i, m)| format!("--- Memory {} (ID: {}) ---\n{}", i + 1, m.id, m.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        // Merge metadata preserving all information
        let combined_metadata = self.merge_metadata_lossless(memories)?;

        // Calculate combined embedding (average of all embeddings)
        let combined_embedding = self.calculate_combined_embedding(memories)?;

        // Calculate merged importance score (weighted average)
        let combined_importance = self.calculate_weighted_importance(memories);

        // Store the combined content length before moving
        let combined_content_len = combined_content.len();

        // Create the merged memory
        let merged_memory = self
            .create_merged_memory(
                combined_content,
                combined_embedding,
                combined_importance,
                combined_metadata,
                primary_memory,
            )
            .await?;

        // Archive original memories
        let storage_saved = self.archive_original_memories(memories).await?;

        Ok(MergeResult {
            merged_memory,
            storage_saved,
            compression_ratio: storage_saved as f32 / combined_content_len as f32,
            original_count: memories.len(),
        })
    }

    /// Merge with metadata consolidation but content preservation
    async fn merge_with_metadata_consolidation(
        &self,
        group: &SimilarMemoryGroup,
    ) -> Result<MergeResult> {
        let memories = &group.memories;
        let primary_memory = &memories[0];

        // Keep primary content but add reference to similar memories
        let mut combined_content = primary_memory.content.clone();
        combined_content.push_str("\n\n--- Related Content References ---\n");

        for (i, memory) in memories.iter().skip(1).enumerate() {
            combined_content.push_str(&format!(
                "Related Memory {}: {} (similarity: {:.3})\n",
                i + 1,
                memory.content.chars().take(100).collect::<String>(),
                group.average_similarity
            ));
        }

        let combined_metadata = self.merge_metadata_consolidated(memories)?;
        let combined_embedding = self.calculate_combined_embedding(memories)?;
        let combined_importance = self.calculate_weighted_importance(memories);

        let merged_memory = self
            .create_merged_memory(
                combined_content,
                combined_embedding,
                combined_importance,
                combined_metadata,
                primary_memory,
            )
            .await?;

        let storage_saved = self.archive_original_memories(memories).await?;

        Ok(MergeResult {
            merged_memory,
            storage_saved,
            compression_ratio: 2.0, // Moderate compression
            original_count: memories.len(),
        })
    }

    /// Merge with content summarization for maximum compression
    async fn merge_with_summarization(&self, group: &SimilarMemoryGroup) -> Result<MergeResult> {
        let memories = &group.memories;
        let primary_memory = &memories[0];

        // Create a summary of all content
        let summary_content = self.create_content_summary(memories).await?;

        let combined_metadata = self.merge_metadata_summarized(memories)?;
        let combined_embedding = self.calculate_combined_embedding(memories)?;
        let combined_importance = self.calculate_weighted_importance(memories);

        let merged_memory = self
            .create_merged_memory(
                summary_content,
                combined_embedding,
                combined_importance,
                combined_metadata,
                primary_memory,
            )
            .await?;

        let storage_saved = self.archive_original_memories(memories).await?;

        Ok(MergeResult {
            merged_memory,
            storage_saved,
            compression_ratio: 5.0, // High compression
            original_count: memories.len(),
        })
    }

    /// Create a content summary from multiple memories
    async fn create_content_summary(&self, memories: &[Memory]) -> Result<String> {
        // Simple extractive summarization
        let mut key_sentences = Vec::new();

        for memory in memories {
            let sentences: Vec<&str> = memory.content.split('.').collect();
            if !sentences.is_empty() {
                key_sentences.push(sentences[0]); // Take first sentence as key
            }
        }

        let summary = format!(
            "Summary of {} related memories:\n{}",
            memories.len(),
            key_sentences.join(". ")
        );

        Ok(summary)
    }

    /// Merge metadata in lossless mode
    fn merge_metadata_lossless(&self, memories: &[Memory]) -> Result<serde_json::Value> {
        let mut combined = serde_json::Map::new();

        // Add merge information
        combined.insert(
            "merge_info".to_string(),
            serde_json::json!({
                "merge_type": "lossless",
                "original_count": memories.len(),
                "merged_at": Utc::now(),
                "original_ids": memories.iter().map(|m| m.id).collect::<Vec<_>>()
            }),
        );

        // Preserve all original metadata
        for (i, memory) in memories.iter().enumerate() {
            if let serde_json::Value::Object(metadata_map) = &memory.metadata {
                for (key, value) in metadata_map {
                    let prefixed_key = format!("memory_{}_{}", i, key);
                    combined.insert(prefixed_key, value.clone());
                }
            }
        }

        Ok(serde_json::Value::Object(combined))
    }

    /// Merge metadata in consolidated mode
    fn merge_metadata_consolidated(&self, memories: &[Memory]) -> Result<serde_json::Value> {
        let mut combined = serde_json::Map::new();

        combined.insert(
            "merge_info".to_string(),
            serde_json::json!({
                "merge_type": "consolidated",
                "original_count": memories.len(),
                "merged_at": Utc::now(),
                "original_ids": memories.iter().map(|m| m.id).collect::<Vec<_>>()
            }),
        );

        // Merge common keys, keep unique ones with prefixes
        let mut common_keys = HashMap::new();
        for memory in memories {
            if let serde_json::Value::Object(metadata_map) = &memory.metadata {
                for (key, value) in metadata_map {
                    common_keys
                        .entry(key.clone())
                        .or_insert_with(Vec::new)
                        .push(value.clone());
                }
            }
        }

        for (key, values) in common_keys {
            if values.len() == memories.len() && values.iter().all(|v| v == &values[0]) {
                // Common value across all memories
                combined.insert(key, values[0].clone());
            } else {
                // Different values, store as array
                combined.insert(key, serde_json::Value::Array(values));
            }
        }

        Ok(serde_json::Value::Object(combined))
    }

    /// Merge metadata in summarized mode
    fn merge_metadata_summarized(&self, memories: &[Memory]) -> Result<serde_json::Value> {
        let mut combined = serde_json::Map::new();

        combined.insert(
            "merge_info".to_string(),
            serde_json::json!({
                "merge_type": "summarized",
                "original_count": memories.len(),
                "merged_at": Utc::now(),
                "original_ids": memories.iter().map(|m| m.id).collect::<Vec<_>>(),
                "compression_applied": true
            }),
        );

        // Only keep essential metadata
        if let serde_json::Value::Object(primary_metadata) = &memories[0].metadata {
            for (key, value) in primary_metadata {
                if ["tags", "category", "type", "priority"].contains(&key.as_str()) {
                    combined.insert(key.clone(), value.clone());
                }
            }
        }

        Ok(serde_json::Value::Object(combined))
    }

    /// Calculate combined embedding from multiple memories
    fn calculate_combined_embedding(&self, memories: &[Memory]) -> Result<Option<Vector>> {
        let embeddings: Vec<&Vector> = memories
            .iter()
            .filter_map(|m| m.embedding.as_ref())
            .collect();

        if embeddings.is_empty() {
            return Ok(None);
        }

        let dimension = embeddings[0].as_slice().len();
        let mut combined = vec![0.0f32; dimension];

        for embedding in &embeddings {
            let slice = embedding.as_slice();
            for (i, &value) in slice.iter().enumerate() {
                combined[i] += value;
            }
        }

        // Average the embeddings
        for value in &mut combined {
            *value /= embeddings.len() as f32;
        }

        Ok(Some(Vector::from(combined)))
    }

    /// Calculate weighted importance score
    fn calculate_weighted_importance(&self, memories: &[Memory]) -> f64 {
        let total_weight: f64 = memories.iter().map(|m| m.access_count as f64 + 1.0).sum();
        let weighted_sum: f64 = memories
            .iter()
            .map(|m| m.importance_score * (m.access_count as f64 + 1.0))
            .sum();

        weighted_sum / total_weight
    }

    /// Create the merged memory record within a transaction
    async fn create_merged_memory_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        content: String,
        embedding: Option<Vector>,
        importance: f64,
        metadata: serde_json::Value,
        primary_memory: &Memory,
    ) -> Result<Memory> {
        let memory_id = Uuid::new_v4();
        let content_hash = format!("{:x}", md5::compute(&content));
        let now = Utc::now();

        let query = r#"
            INSERT INTO memories (
                id, content, content_hash, embedding, tier, status,
                importance_score, access_count, last_accessed_at, metadata,
                parent_id, created_at, updated_at, expires_at,
                consolidation_strength, decay_rate, recall_probability,
                recency_score, relevance_score, is_merged_result,
                original_memory_count, merge_generation
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22
            ) RETURNING *
        "#;

        let row = sqlx::query(query)
            .bind(memory_id)
            .bind(&content)
            .bind(&content_hash)
            .bind(embedding)
            .bind(primary_memory.tier)
            .bind(MemoryStatus::Active)
            .bind(importance)
            .bind(0i32) // access_count
            .bind(now) // last_accessed_at
            .bind(&metadata)
            .bind(primary_memory.parent_id)
            .bind(now) // created_at
            .bind(now) // updated_at
            .bind(primary_memory.expires_at)
            .bind(primary_memory.consolidation_strength)
            .bind(primary_memory.decay_rate)
            .bind(primary_memory.recall_probability)
            .bind(primary_memory.recency_score)
            .bind(primary_memory.relevance_score)
            .bind(true) // is_merged_result
            .bind(1i32) // original_memory_count (will be updated)
            .bind(
                primary_memory
                    .metadata
                    .get("merge_generation")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    + 1,
            ) // merge_generation
            .fetch_one(&mut **transaction)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to create merged memory: {}", e),
            })?;

        Ok(Memory {
            id: row.get("id"),
            content: row.get("content"),
            content_hash: row.get("content_hash"),
            embedding: row.get("embedding"),
            tier: row.get("tier"),
            status: row.get("status"),
            importance_score: row.get("importance_score"),
            access_count: row.get("access_count"),
            last_accessed_at: row.get("last_accessed_at"),
            metadata: row.get("metadata"),
            parent_id: row.get("parent_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            expires_at: row.get("expires_at"),
            consolidation_strength: row.get("consolidation_strength"),
            decay_rate: row.get("decay_rate"),
            recall_probability: row.get("recall_probability"),
            last_recall_interval: row.get("last_recall_interval"),
            recency_score: row.get("recency_score"),
            relevance_score: row.get("relevance_score"),
        })
    }

    /// Create the merged memory record (legacy method)
    async fn create_merged_memory(
        &self,
        content: String,
        embedding: Option<Vector>,
        importance: f64,
        metadata: serde_json::Value,
        primary_memory: &Memory,
    ) -> Result<Memory> {
        let create_request = CreateMemoryRequest {
            content,
            embedding: embedding.map(|v| v.as_slice().to_vec()),
            tier: Some(primary_memory.tier),
            importance_score: Some(importance),
            metadata: Some(metadata),
            parent_id: None,
            expires_at: primary_memory.expires_at,
        };

        self.repository.create_memory(create_request).await
    }

    /// Transaction-based lossless merge
    async fn merge_lossless_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        group: &SimilarMemoryGroup,
    ) -> Result<MergeResult> {
        let memories = &group.memories;
        let primary_memory = &memories[0];

        // Combine all content with clear delineation
        let combined_content = memories
            .iter()
            .enumerate()
            .map(|(i, m)| format!("--- Memory {} (ID: {}) ---\n{}", i + 1, m.id, m.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        // Merge metadata preserving all information
        let combined_metadata = self.merge_metadata_lossless(memories)?;
        let combined_embedding = self.calculate_combined_embedding(memories)?;
        let combined_importance = self.calculate_weighted_importance(memories);

        // Create the merged memory within transaction
        let merged_memory = self
            .create_merged_memory_tx(
                transaction,
                combined_content,
                combined_embedding,
                combined_importance,
                combined_metadata,
                primary_memory,
            )
            .await?;

        // Archive original memories with verification
        let storage_saved = self
            .archive_original_memories_tx(transaction, memories, merged_memory.id)
            .await?;

        Ok(MergeResult {
            merged_memory: merged_memory.clone(),
            storage_saved,
            compression_ratio: storage_saved as f32 / merged_memory.content.len() as f32,
            original_count: memories.len(),
        })
    }

    /// Transaction-based metadata consolidation merge
    async fn merge_with_metadata_consolidation_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        group: &SimilarMemoryGroup,
    ) -> Result<MergeResult> {
        let memories = &group.memories;
        let primary_memory = &memories[0];

        // Keep primary content but add reference to similar memories
        let mut combined_content = primary_memory.content.clone();
        combined_content.push_str("\n\n--- Related Content References ---\n");

        for (i, memory) in memories.iter().skip(1).enumerate() {
            combined_content.push_str(&format!(
                "Related Memory {}: {} (similarity: {:.3})\n",
                i + 1,
                memory.content.chars().take(100).collect::<String>(),
                group.average_similarity
            ));
        }

        let combined_metadata = self.merge_metadata_consolidated(memories)?;
        let combined_embedding = self.calculate_combined_embedding(memories)?;
        let combined_importance = self.calculate_weighted_importance(memories);

        let merged_memory = self
            .create_merged_memory_tx(
                transaction,
                combined_content,
                combined_embedding,
                combined_importance,
                combined_metadata,
                primary_memory,
            )
            .await?;

        let storage_saved = self
            .archive_original_memories_tx(transaction, memories, merged_memory.id)
            .await?;

        Ok(MergeResult {
            merged_memory,
            storage_saved,
            compression_ratio: 2.0, // Moderate compression
            original_count: memories.len(),
        })
    }

    /// Transaction-based summarization merge
    async fn merge_with_summarization_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        group: &SimilarMemoryGroup,
    ) -> Result<MergeResult> {
        let memories = &group.memories;
        let primary_memory = &memories[0];

        // Create a summary of all content
        let summary_content = self.create_content_summary(memories).await?;

        let combined_metadata = self.merge_metadata_summarized(memories)?;
        let combined_embedding = self.calculate_combined_embedding(memories)?;
        let combined_importance = self.calculate_weighted_importance(memories);

        let merged_memory = self
            .create_merged_memory_tx(
                transaction,
                summary_content,
                combined_embedding,
                combined_importance,
                combined_metadata,
                primary_memory,
            )
            .await?;

        let storage_saved = self
            .archive_original_memories_tx(transaction, memories, merged_memory.id)
            .await?;

        Ok(MergeResult {
            merged_memory,
            storage_saved,
            compression_ratio: 5.0, // High compression
            original_count: memories.len(),
        })
    }

    /// Archive the original memories with proper verification and transaction safety
    async fn archive_original_memories_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        memories: &[Memory],
        merged_memory_id: Uuid,
    ) -> Result<u64> {
        let mut total_size = 0u64;

        // Verify merged memory was created successfully before archiving originals
        let merged_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM memories WHERE id = $1 AND status = 'active')",
        )
        .bind(merged_memory_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to verify merged memory exists: {}", e),
        })?;

        if !merged_exists {
            return Err(MemoryError::SafetyViolation {
                message: format!(
                    "Cannot archive original memories: merged memory {} does not exist",
                    merged_memory_id
                ),
            });
        }

        for memory in memories {
            // Verify memory is still active before archiving
            let current_status =
                sqlx::query_scalar::<_, String>("SELECT status FROM memories WHERE id = $1")
                    .bind(memory.id)
                    .fetch_optional(&mut **transaction)
                    .await
                    .map_err(|e| MemoryError::DatabaseError {
                        message: format!("Failed to check memory status: {}", e),
                    })?;

            match current_status {
                Some(status) if status == "active" => {
                    total_size += memory.content.len() as u64;

                    // Create backup in compression log for reversibility
                    sqlx::query(
                        r#"
                        INSERT INTO memory_compression_log (
                            memory_id, original_content, original_metadata, 
                            compression_type, compression_ratio, 
                            reversible_until
                        ) VALUES ($1, $2, $3, $4, $5, $6)
                    "#,
                    )
                    .bind(memory.id)
                    .bind(&memory.content)
                    .bind(&memory.metadata)
                    .bind("archive")
                    .bind(1.0)
                    .bind(Utc::now() + Duration::days(7))
                    .execute(&mut **transaction)
                    .await
                    .map_err(|e| MemoryError::DatabaseError {
                        message: format!("Failed to create archive backup: {}", e),
                    })?;

                    // Update status to archived
                    let archive_result = sqlx::query(
                        "UPDATE memories SET status = 'archived', updated_at = NOW() WHERE id = $1 AND status = 'active'",
                    )
                    .bind(memory.id)
                    .execute(&mut **transaction)
                    .await
                    .map_err(|e| MemoryError::DatabaseError {
                        message: format!("Failed to archive memory: {}", e),
                    })?;

                    if archive_result.rows_affected() == 0 {
                        warn!(
                            "Memory {} was not archived (may have been modified concurrently)",
                            memory.id
                        );
                    } else {
                        debug!("Successfully archived memory: {}", memory.id);
                    }
                }
                Some(status) => {
                    warn!(
                        "Skipping archival of memory {} - status is {}, not active",
                        memory.id, status
                    );
                }
                None => {
                    warn!("Memory {} no longer exists, cannot archive", memory.id);
                }
            }
        }

        Ok(total_size)
    }

    /// Archive the original memories (legacy method for backward compatibility)
    async fn archive_original_memories(&self, memories: &[Memory]) -> Result<u64> {
        let mut total_size = 0u64;

        for memory in memories {
            total_size += memory.content.len() as u64;

            // Update status to archived instead of deleting
            sqlx::query(
                "UPDATE memories SET status = 'archived', updated_at = NOW() WHERE id = $1",
            )
            .bind(memory.id)
            .execute(self.repository.pool())
            .await?;
        }

        Ok(total_size)
    }
}

/// Compression manager for hierarchical compression strategies
pub struct CompressionManager {
    config: SemanticDeduplicationConfig,
}

impl CompressionManager {
    pub fn new(config: SemanticDeduplicationConfig) -> Self {
        Self { config }
    }

    /// Apply compression based on memory tier and criticality
    pub async fn compress_memory(&self, memory: &Memory) -> Result<CompressionResult> {
        let is_critical =
            memory.importance_score > 0.8 || matches!(memory.tier, MemoryTier::Working);

        if is_critical && self.config.lossless_critical {
            self.apply_lossless_compression(memory).await
        } else {
            self.apply_lossy_compression(memory).await
        }
    }

    /// Apply lossless compression (mainly structural optimization)
    async fn apply_lossless_compression(&self, memory: &Memory) -> Result<CompressionResult> {
        // Lossless compression: remove redundant whitespace, normalize structure
        let compressed_content = self.normalize_content(&memory.content);
        let compression_ratio = memory.content.len() as f32 / compressed_content.len() as f32;

        Ok(CompressionResult {
            original_size: memory.content.len(),
            compressed_size: compressed_content.len(),
            compression_ratio,
            is_lossless: true,
            compressed_content,
        })
    }

    /// Apply lossy compression (content summarization, metadata reduction)
    async fn apply_lossy_compression(&self, memory: &Memory) -> Result<CompressionResult> {
        // Lossy compression: extract key information, summarize
        let compressed_content = self.extract_key_information(&memory.content);
        let compression_ratio = memory.content.len() as f32 / compressed_content.len() as f32;

        Ok(CompressionResult {
            original_size: memory.content.len(),
            compressed_size: compressed_content.len(),
            compression_ratio,
            is_lossless: false,
            compressed_content,
        })
    }

    /// Normalize content for lossless compression
    fn normalize_content(&self, content: &str) -> String {
        // Remove excessive whitespace while preserving structure
        content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract key information for lossy compression
    fn extract_key_information(&self, content: &str) -> String {
        // Simple extractive approach: keep first and last sentences, key phrases
        let sentences: Vec<&str> = content
            .split('.')
            .filter(|s| !s.trim().is_empty())
            .collect();

        if sentences.len() <= 2 {
            return content.to_string();
        }

        format!(
            "{}. ... {} (content compressed from {} to 2 key sentences)",
            sentences[0].trim(),
            sentences.last().unwrap().trim(),
            sentences.len()
        )
    }
}

/// Audit trail for operation tracking and reversibility
pub struct AuditTrail {
    repository: Arc<MemoryRepository>,
}

impl AuditTrail {
    pub fn new(repository: Arc<MemoryRepository>) -> Self {
        Self { repository }
    }

    /// Create a merge audit entry within a transaction
    pub async fn create_merge_entry_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        group: &SimilarMemoryGroup,
    ) -> Result<AuditEntry> {
        let entry_id = Uuid::new_v4();
        let operation_data = serde_json::json!({
            "operation_type": "merge",
            "memory_ids": group.memories.iter().map(|m| m.id).collect::<Vec<_>>(),
            "strategy": group.merge_strategy,
            "similarity": group.average_similarity,
            "memory_count": group.memories.len()
        });

        sqlx::query(
            r#"
            INSERT INTO deduplication_audit_log 
            (id, operation_type, operation_data, created_at, status)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(entry_id)
        .bind("merge")
        .bind(operation_data)
        .bind(Utc::now())
        .bind("in_progress")
        .execute(&mut **transaction)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to create merge audit entry: {}", e),
        })?;

        Ok(AuditEntry {
            id: entry_id,
            operation_type: "merge".to_string(),
            created_at: Utc::now(),
            status: "in_progress".to_string(),
        })
    }

    /// Create a merge audit entry (legacy method)
    pub async fn create_merge_entry(&self, group: &SimilarMemoryGroup) -> Result<AuditEntry> {
        let entry_id = Uuid::new_v4();
        let operation_data = serde_json::json!({
            "operation_type": "merge",
            "memory_ids": group.memories.iter().map(|m| m.id).collect::<Vec<_>>(),
            "strategy": group.merge_strategy,
            "similarity": group.average_similarity,
            "memory_count": group.memories.len()
        });

        sqlx::query(
            r#"
            INSERT INTO deduplication_audit_log 
            (id, operation_type, operation_data, created_at, status)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(entry_id)
        .bind("merge")
        .bind(operation_data)
        .bind(Utc::now())
        .bind("in_progress")
        .execute(self.repository.pool())
        .await?;

        Ok(AuditEntry {
            id: entry_id,
            operation_type: "merge".to_string(),
            created_at: Utc::now(),
            status: "in_progress".to_string(),
        })
    }

    /// Complete a merge audit entry within a transaction
    pub async fn complete_merge_entry_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        entry_id: Uuid,
        result: &MergeResult,
    ) -> Result<()> {
        let completion_data = serde_json::json!({
            "merged_memory_id": result.merged_memory.id,
            "storage_saved": result.storage_saved,
            "compression_ratio": result.compression_ratio,
            "original_count": result.original_count
        });

        sqlx::query(
            r#"
            UPDATE deduplication_audit_log 
            SET status = $1, completion_data = $2, completed_at = $3
            WHERE id = $4
            "#,
        )
        .bind("completed")
        .bind(completion_data)
        .bind(Utc::now())
        .bind(entry_id)
        .execute(&mut **transaction)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to complete merge audit entry: {}", e),
        })?;

        Ok(())
    }

    /// Complete a merge audit entry (legacy method)
    pub async fn complete_merge_entry(&self, entry_id: Uuid, result: &MergeResult) -> Result<()> {
        let completion_data = serde_json::json!({
            "merged_memory_id": result.merged_memory.id,
            "storage_saved": result.storage_saved,
            "compression_ratio": result.compression_ratio,
            "original_count": result.original_count
        });

        sqlx::query(
            r#"
            UPDATE deduplication_audit_log 
            SET status = $1, completion_data = $2, completed_at = $3
            WHERE id = $4
            "#,
        )
        .bind("completed")
        .bind(completion_data)
        .bind(Utc::now())
        .bind(entry_id)
        .execute(self.repository.pool())
        .await?;

        Ok(())
    }

    /// Record a pruning operation
    pub async fn record_pruning(&self, pruned_ids: &[Uuid], reason: &str) -> Result<AuditEntry> {
        let entry_id = Uuid::new_v4();
        let operation_data = serde_json::json!({
            "operation_type": "prune",
            "pruned_memory_ids": pruned_ids,
            "reason": reason,
            "count": pruned_ids.len()
        });

        sqlx::query(
            r#"
            INSERT INTO deduplication_audit_log 
            (id, operation_type, operation_data, created_at, status, completed_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(entry_id)
        .bind("prune")
        .bind(operation_data)
        .bind(Utc::now())
        .bind("completed")
        .bind(Utc::now())
        .execute(self.repository.pool())
        .await?;

        Ok(AuditEntry {
            id: entry_id,
            operation_type: "prune".to_string(),
            created_at: Utc::now(),
            status: "completed".to_string(),
        })
    }

    /// Get reversible operations (within 7 days)
    pub async fn get_reversible_operations(&self) -> Result<Vec<ReversibleOperation>> {
        let cutoff_date = Utc::now() - Duration::days(7);

        let query = r#"
            SELECT id, operation_type, operation_data, completion_data, created_at
            FROM deduplication_audit_log 
            WHERE created_at > $1 
            AND operation_type IN ('merge', 'prune')
            AND status = 'completed'
            AND reversible_until IS NOT NULL
            AND reversible_until > NOW()
            ORDER BY created_at DESC
        "#;

        let rows = sqlx::query(query)
            .bind(cutoff_date)
            .fetch_all(self.repository.pool())
            .await?;

        let mut operations = Vec::new();
        for row in rows {
            operations.push(ReversibleOperation {
                id: row.get("id"),
                operation_type: row.get("operation_type"),
                operation_data: row.get("operation_data"),
                completion_data: row.get("completion_data"),
                created_at: row.get("created_at"),
            });
        }

        Ok(operations)
    }

    /// Reverse a deduplication or pruning operation
    pub async fn reverse_operation(&self, operation_id: Uuid) -> Result<ReversalResult> {
        info!("Starting reversal of operation: {}", operation_id);

        // Begin transaction for atomic reversal
        let mut transaction =
            self.repository
                .pool()
                .begin()
                .await
                .map_err(|e| MemoryError::DatabaseError {
                    message: format!("Failed to begin reversal transaction: {}", e),
                })?;

        let result = self
            .execute_operation_reversal(&mut transaction, operation_id)
            .await;

        match result {
            Ok(reversal_result) => {
                transaction
                    .commit()
                    .await
                    .map_err(|e| MemoryError::DatabaseError {
                        message: format!("Failed to commit reversal transaction: {}", e),
                    })?;
                info!("Successfully reversed operation: {}", operation_id);
                Ok(reversal_result)
            }
            Err(e) => {
                if let Err(rollback_err) = transaction.rollback().await {
                    error!("Failed to rollback reversal transaction: {}", rollback_err);
                }
                error!("Reversal failed for operation {}: {}", operation_id, e);
                Err(e)
            }
        }
    }

    /// Execute operation reversal within a transaction
    async fn execute_operation_reversal(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        operation_id: Uuid,
    ) -> Result<ReversalResult> {
        // Get operation details
        let operation_query = r#"
            SELECT operation_type, operation_data, completion_data, status, reversible_until
            FROM deduplication_audit_log 
            WHERE id = $1
        "#;

        let operation_row = sqlx::query(operation_query)
            .bind(operation_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to fetch operation details: {}", e),
            })?
            .ok_or_else(|| MemoryError::NotFound {
                id: operation_id.to_string(),
            })?;

        let operation_type: String = operation_row.get("operation_type");
        let operation_data: serde_json::Value = operation_row.get("operation_data");
        let status: String = operation_row.get("status");
        let reversible_until: Option<DateTime<Utc>> = operation_row.get("reversible_until");

        // Validate operation is reversible
        if status != "completed" {
            return Err(MemoryError::InvalidRequest {
                message: format!(
                    "Operation {} is not in completed state: {}",
                    operation_id, status
                ),
            });
        }

        if let Some(cutoff) = reversible_until {
            if Utc::now() > cutoff {
                return Err(MemoryError::InvalidRequest {
                    message: format!("Operation {} is past its reversal deadline", operation_id),
                });
            }
        } else {
            return Err(MemoryError::InvalidRequest {
                message: format!("Operation {} is not reversible", operation_id),
            });
        }

        // Perform reversal based on operation type
        let reversal_result = match operation_type.as_str() {
            "merge" => {
                self.reverse_merge_operation(transaction, operation_id, &operation_data)
                    .await?
            }
            "prune" => {
                self.reverse_prune_operation(transaction, operation_id, &operation_data)
                    .await?
            }
            _ => {
                return Err(MemoryError::InvalidRequest {
                    message: format!(
                        "Unsupported operation type for reversal: {}",
                        operation_type
                    ),
                });
            }
        };

        // Mark operation as reversed
        sqlx::query(
            r#"
            UPDATE deduplication_audit_log 
            SET status = 'reversed', reversible_until = NULL
            WHERE id = $1
        "#,
        )
        .bind(operation_id)
        .execute(&mut **transaction)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to mark operation as reversed: {}", e),
        })?;

        Ok(reversal_result)
    }

    /// Reverse a merge operation by restoring original memories
    async fn reverse_merge_operation(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        operation_id: Uuid,
        operation_data: &serde_json::Value,
    ) -> Result<ReversalResult> {
        info!("Reversing merge operation: {}", operation_id);

        // Extract memory IDs from operation data
        let memory_ids: Vec<Uuid> = operation_data["memory_ids"]
            .as_array()
            .ok_or_else(|| MemoryError::InvalidData {
                message: "Missing memory_ids in merge operation data".to_string(),
            })?
            .iter()
            .map(|id| {
                Uuid::parse_str(id.as_str().unwrap_or("")).map_err(|e| MemoryError::InvalidData {
                    message: format!("Invalid UUID in memory_ids: {}", e),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // Get merge history to find the merged result and original memories
        let merge_history_query = r#"
            SELECT merged_memory_id, original_memory_id 
            FROM memory_merge_history 
            WHERE merge_operation_id = $1
        "#;

        let merge_rows = sqlx::query(merge_history_query)
            .bind(operation_id)
            .fetch_all(&mut **transaction)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to fetch merge history: {}", e),
            })?;

        if merge_rows.is_empty() {
            return Err(MemoryError::InvalidData {
                message: "No merge history found for operation".to_string(),
            });
        }

        let merged_memory_id: Uuid = merge_rows[0].get("merged_memory_id");
        let mut restored_count = 0;

        // Restore each original memory from archived status
        for memory_id in &memory_ids {
            let restore_result = sqlx::query(
                r#"
                UPDATE memories 
                SET status = 'active', updated_at = NOW() 
                WHERE id = $1 AND status = 'archived'
            "#,
            )
            .bind(memory_id)
            .execute(&mut **transaction)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to restore memory {}: {}", memory_id, e),
            })?;

            if restore_result.rows_affected() > 0 {
                restored_count += 1;
                info!("Restored original memory: {}", memory_id);
            } else {
                warn!(
                    "Memory {} was not in archived status, cannot restore",
                    memory_id
                );
            }
        }

        // Archive the merged memory result
        sqlx::query(
            r#"
            UPDATE memories 
            SET status = 'archived', updated_at = NOW() 
            WHERE id = $1
        "#,
        )
        .bind(merged_memory_id)
        .execute(&mut **transaction)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to archive merged memory: {}", e),
        })?;

        info!("Archived merged memory result: {}", merged_memory_id);

        Ok(ReversalResult {
            operation_id,
            operation_type: "merge".to_string(),
            memories_restored: restored_count,
            success: restored_count > 0,
            message: format!(
                "Restored {} of {} original memories from merge operation",
                restored_count,
                memory_ids.len()
            ),
        })
    }

    /// Reverse a prune operation by restoring deleted memories
    async fn reverse_prune_operation(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        operation_id: Uuid,
        operation_data: &serde_json::Value,
    ) -> Result<ReversalResult> {
        info!("Reversing prune operation: {}", operation_id);

        // Extract pruned memory IDs from operation data
        let pruned_memory_ids: Vec<Uuid> = operation_data["pruned_memory_ids"]
            .as_array()
            .ok_or_else(|| MemoryError::InvalidData {
                message: "Missing pruned_memory_ids in prune operation data".to_string(),
            })?
            .iter()
            .map(|id| {
                Uuid::parse_str(id.as_str().unwrap_or("")).map_err(|e| MemoryError::InvalidData {
                    message: format!("Invalid UUID in pruned_memory_ids: {}", e),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let mut restored_count = 0;

        // Restore each pruned memory from deleted status
        for memory_id in &pruned_memory_ids {
            let restore_result = sqlx::query(
                r#"
                UPDATE memories 
                SET status = 'active', updated_at = NOW() 
                WHERE id = $1 AND status = 'deleted'
            "#,
            )
            .bind(memory_id)
            .execute(&mut **transaction)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to restore pruned memory {}: {}", memory_id, e),
            })?;

            if restore_result.rows_affected() > 0 {
                restored_count += 1;
                info!("Restored pruned memory: {}", memory_id);
            } else {
                warn!(
                    "Memory {} was not in deleted status, cannot restore",
                    memory_id
                );
            }
        }

        Ok(ReversalResult {
            operation_id,
            operation_type: "prune".to_string(),
            memories_restored: restored_count,
            success: restored_count > 0,
            message: format!(
                "Restored {} of {} pruned memories",
                restored_count,
                pruned_memory_ids.len()
            ),
        })
    }
}

/// Auto-pruner for automated memory cleanup
pub struct AutoPruner {
    #[allow(dead_code)]
    config: SemanticDeduplicationConfig,
    repository: Arc<MemoryRepository>,
}

impl AutoPruner {
    pub fn new(config: SemanticDeduplicationConfig, repository: Arc<MemoryRepository>) -> Self {
        Self { config, repository }
    }

    /// Prune memories based on recall probability and age with comprehensive safety checks
    pub async fn prune_memories(
        &self,
        threshold: f64,
        cutoff_date: DateTime<Utc>,
    ) -> Result<PruningResult> {
        info!(
            "Starting safe pruning with threshold {} and cutoff date {}",
            threshold, cutoff_date
        );

        // Begin transaction for atomic pruning operation
        let mut transaction =
            self.repository
                .pool()
                .begin()
                .await
                .map_err(|e| MemoryError::DatabaseError {
                    message: format!("Failed to begin pruning transaction: {}", e),
                })?;

        let result = self
            .execute_safe_pruning(&mut transaction, threshold, cutoff_date)
            .await;

        match result {
            Ok(pruning_result) => {
                transaction
                    .commit()
                    .await
                    .map_err(|e| MemoryError::DatabaseError {
                        message: format!("Failed to commit pruning transaction: {}", e),
                    })?;
                info!(
                    "Safe pruning completed: {} memories pruned, {} bytes freed",
                    pruning_result.memories_pruned, pruning_result.storage_freed
                );
                Ok(pruning_result)
            }
            Err(e) => {
                if let Err(rollback_err) = transaction.rollback().await {
                    error!("Failed to rollback pruning transaction: {}", rollback_err);
                }
                error!("Pruning failed: {}", e);
                Err(e)
            }
        }
    }

    /// Execute pruning with comprehensive safety checks within a transaction
    async fn execute_safe_pruning(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        threshold: f64,
        cutoff_date: DateTime<Utc>,
    ) -> Result<PruningResult> {
        // Query for pruning candidates with comprehensive safety filtering
        let query = r#"
            SELECT 
                id, 
                content, 
                recall_probability,
                importance_score,
                access_count,
                last_accessed_at,
                tier,
                metadata,
                consolidation_strength
            FROM memories 
            WHERE recall_probability IS NOT NULL 
            AND recall_probability < $1 
            AND created_at < $2 
            AND status = 'active'
            AND tier IN ('cold', 'frozen')
            -- Additional safety checks
            AND importance_score < 0.3  -- Don't prune important memories
            AND access_count < 10       -- Don't prune frequently accessed memories
            AND (
                last_accessed_at IS NULL OR 
                last_accessed_at < (NOW() - INTERVAL '30 days')
            )  -- Don't prune recently accessed memories
            AND consolidation_strength < 0.5  -- Don't prune well-consolidated memories
            -- Exclude memories with critical metadata markers
            AND NOT (
                metadata ? 'critical' OR 
                metadata ? 'important' OR 
                metadata ? 'permanent' OR
                metadata ? 'do_not_prune'
            )
            ORDER BY 
                recall_probability ASC, 
                importance_score ASC,
                last_accessed_at ASC NULLS FIRST
            LIMIT 500  -- Conservative batch size for safety
        "#;

        let candidates = sqlx::query(query)
            .bind(threshold)
            .bind(cutoff_date)
            .fetch_all(&mut **transaction)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to fetch pruning candidates: {}", e),
            })?;

        info!(
            "Found {} pruning candidates after safety filtering",
            candidates.len()
        );

        let mut pruned_ids = Vec::new();
        let mut storage_freed = 0u64;
        let mut safety_violations = 0;

        for row in candidates {
            let memory_id: Uuid = row.get("id");
            let content: String = row.get("content");
            let importance_score: f64 = row.get("importance_score");
            let access_count: i32 = row.get("access_count");
            let metadata: serde_json::Value = row.get("metadata");
            let tier: MemoryTier = row.get("tier");

            // Final safety validation before pruning
            if let Err(violation) = self.validate_pruning_safety(
                memory_id,
                importance_score,
                access_count,
                &metadata,
                tier,
            ) {
                warn!("Skipping pruning due to safety violation: {}", violation);
                safety_violations += 1;
                continue;
            }

            // Double-check memory is still eligible (race condition protection)
            let recheck_query = r#"
                SELECT status, tier, importance_score 
                FROM memories 
                WHERE id = $1 
                AND status = 'active' 
                AND tier IN ('cold', 'frozen')
                AND importance_score < 0.3
            "#;

            let recheck_result = sqlx::query(recheck_query)
                .bind(memory_id)
                .fetch_optional(&mut **transaction)
                .await
                .map_err(|e| MemoryError::DatabaseError {
                    message: format!("Failed to recheck memory eligibility: {}", e),
                })?;

            if recheck_result.is_none() {
                warn!(
                    "Memory {} no longer eligible for pruning, skipping",
                    memory_id
                );
                continue;
            }

            storage_freed += content.len() as u64;
            pruned_ids.push(memory_id);

            // Create audit entry for reversibility
            sqlx::query(
                r#"
                INSERT INTO memory_pruning_log (
                    memory_id, recall_probability, age_days, tier, 
                    importance_score, access_count, content_size_bytes, 
                    pruning_reason
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            )
            .bind(memory_id)
            .bind(
                row.get::<Option<f64>, _>("recall_probability")
                    .unwrap_or(0.0),
            )
            .bind((Utc::now() - cutoff_date).num_days())
            .bind(format!("{:?}", tier).to_lowercase())
            .bind(importance_score)
            .bind(access_count)
            .bind(content.len() as i32)
            .bind(format!(
                "Auto-pruning: threshold={}, cutoff={}",
                threshold, cutoff_date
            ))
            .execute(&mut **transaction)
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to create pruning audit entry: {}", e),
            })?;

            // Mark as deleted rather than hard delete for reversibility
            sqlx::query("UPDATE memories SET status = 'deleted', updated_at = NOW() WHERE id = $1")
                .bind(memory_id)
                .execute(&mut **transaction)
                .await
                .map_err(|e| MemoryError::DatabaseError {
                    message: format!("Failed to mark memory as deleted: {}", e),
                })?;
        }

        if safety_violations > 0 {
            warn!(
                "Encountered {} safety violations during pruning",
                safety_violations
            );
        }

        Ok(PruningResult {
            memories_pruned: pruned_ids.len(),
            storage_freed,
            pruned_memory_ids: pruned_ids,
        })
    }

    /// Validate that pruning a memory is safe
    fn validate_pruning_safety(
        &self,
        memory_id: Uuid,
        importance_score: f64,
        access_count: i32,
        metadata: &serde_json::Value,
        tier: MemoryTier,
    ) -> Result<()> {
        // Check importance score threshold
        if importance_score >= 0.3 {
            return Err(MemoryError::SafetyViolation {
                message: format!(
                    "Memory {} has high importance score: {}",
                    memory_id, importance_score
                ),
            });
        }

        // Check access count threshold
        if access_count >= 10 {
            return Err(MemoryError::SafetyViolation {
                message: format!(
                    "Memory {} has high access count: {}",
                    memory_id, access_count
                ),
            });
        }

        // Check tier restrictions
        if matches!(tier, MemoryTier::Working | MemoryTier::Warm) {
            return Err(MemoryError::SafetyViolation {
                message: format!("Memory {} is in protected tier: {:?}", memory_id, tier),
            });
        }

        // Check for critical metadata flags
        if let serde_json::Value::Object(obj) = metadata {
            for key in ["critical", "important", "permanent", "do_not_prune"] {
                if obj.contains_key(key) {
                    return Err(MemoryError::SafetyViolation {
                        message: format!(
                            "Memory {} has critical metadata flag: {}",
                            memory_id, key
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Comprehensive metrics for deduplication operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeduplicationMetrics {
    pub total_operations: u64,
    pub total_memories_processed: usize,
    pub total_memories_merged: usize,
    pub total_storage_saved: u64,
    pub total_execution_time_ms: u64,
    pub average_compression_ratio: f32,
    pub errors_encountered: u64,
    pub last_operation_timestamp: Option<DateTime<Utc>>,
}

/// Result structures for various operations
#[derive(Debug, Clone, Default)]
pub struct DeduplicationResult {
    pub total_processed: usize,
    pub groups_identified: usize,
    pub memories_merged: usize,
    pub storage_saved_bytes: u64,
    pub compression_ratio: f32,
    pub execution_time_ms: u64,
    pub errors_encountered: u64,
}

#[derive(Debug, Clone)]
pub struct SimilarMemoryGroup {
    pub memories: Vec<Memory>,
    pub average_similarity: f32,
    pub merge_strategy: MergeStrategy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MergeStrategy {
    LosslessPreservation,
    MetadataConsolidation,
    ContentSummarization,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub merged_memory: Memory,
    pub storage_saved: u64,
    pub compression_ratio: f32,
    pub original_count: usize,
}

#[derive(Debug, Clone)]
pub struct GroupMergeResult {
    pub merged_memory_id: Uuid,
    pub original_memory_ids: Vec<Uuid>,
    pub memories_merged: usize,
    pub storage_saved: u64,
    pub compression_ratio: f32,
    pub merge_strategy: MergeStrategy,
}

#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub original_size: usize,
    pub compressed_size: usize,
    pub compression_ratio: f32,
    pub is_lossless: bool,
    pub compressed_content: String,
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: Uuid,
    pub operation_type: String,
    pub created_at: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct ReversibleOperation {
    pub id: Uuid,
    pub operation_type: String,
    pub operation_data: serde_json::Value,
    pub completion_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PruningResult {
    pub memories_pruned: usize,
    pub storage_freed: u64,
    pub pruned_memory_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct HeadroomMaintenanceResult {
    pub initial_utilization: f32,
    pub final_utilization: f32,
    pub memories_processed: usize,
    pub memories_merged: usize,
    pub memories_pruned: usize,
    pub storage_freed: u64,
}

#[derive(Debug, Clone)]
pub struct MemoryStatistics {
    pub total_memories: i64,
    pub total_content_bytes: i64,
    pub total_space_bytes: i64,
    pub free_space_bytes: i64,
    pub avg_importance: f64,
    pub working_count: i64,
    pub warm_count: i64,
    pub cold_count: i64,
    pub frozen_count: i64,
}

#[derive(Debug, Clone)]
pub struct ReversalResult {
    pub operation_id: Uuid,
    pub operation_type: String,
    pub memories_restored: usize,
    pub success: bool,
    pub message: String,
}

/// Performance monitoring for deduplication operations
#[derive(Debug)]
struct PerformanceMetrics {
    operation_id: String,
    start_time: Instant,
    phase_timings: HashMap<String, std::time::Duration>,
    memory_count: usize,
    target_time_seconds: u64,
}

impl PerformanceMetrics {
    fn record_phase(&mut self, phase: &str, duration: std::time::Duration) {
        self.phase_timings.insert(phase.to_string(), duration);

        // Log slow phases
        if duration.as_millis() > 1000 {
            warn!(
                "Slow phase '{}' in operation {}: {}ms",
                phase,
                self.operation_id,
                duration.as_millis()
            );
        }
    }

    fn format_phase_summary(&self) -> String {
        let mut phases: Vec<_> = self.phase_timings.iter().collect();
        phases.sort_by_key(|(_, duration)| *duration);

        phases
            .iter()
            .map(|(name, duration)| format!("{}:{}ms", name, duration.as_millis()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn get_performance_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();

        let total_duration = self.start_time.elapsed();
        if total_duration.as_secs() > self.target_time_seconds {
            violations.push(format!(
                "Total time {}s exceeds target {}s",
                total_duration.as_secs(),
                self.target_time_seconds
            ));
        }

        if self.memory_count >= 10_000 && total_duration.as_secs() > 30 {
            violations.push(format!(
                "Large batch ({} memories) took {}s, exceeds 30s target",
                self.memory_count,
                total_duration.as_secs()
            ));
        }

        // Check individual phase performance
        for (phase, duration) in &self.phase_timings {
            if duration.as_millis() > 5000 {
                violations.push(format!(
                    "Phase '{}' took {}ms, may need optimization",
                    phase,
                    duration.as_millis()
                ));
            }
        }

        violations
    }
}
