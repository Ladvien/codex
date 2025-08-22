use super::error::{MemoryError, Result};
use super::models::*;
use super::repository::MemoryRepository;
use crate::embedding::EmbeddingService;
use chrono::{DateTime, Duration, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
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

/// Main semantic deduplication engine
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
        }
    }

    /// Perform semantic deduplication on a batch of memories
    pub async fn deduplicate_batch(&self, memory_ids: &[Uuid]) -> Result<DeduplicationResult> {
        let start_time = std::time::Instant::now();
        let mut result = DeduplicationResult::default();

        info!(
            "Starting deduplication for {} memories with threshold {}",
            memory_ids.len(),
            self.config.similarity_threshold
        );

        // Load memories with embeddings
        let memories = self.load_memories_with_embeddings(memory_ids).await?;
        if memories.is_empty() {
            return Ok(result);
        }

        result.total_processed = memories.len();

        // Find similar memory groups
        let similar_groups = self.find_similar_groups(&memories).await?;
        result.groups_identified = similar_groups.len();

        // Process each group for merging
        for group in similar_groups {
            match self.process_similar_group(group).await {
                Ok(merge_result) => {
                    result.memories_merged += merge_result.memories_merged;
                    result.storage_saved_bytes += merge_result.storage_saved;
                    result.compression_ratio =
                        (result.compression_ratio + merge_result.compression_ratio) / 2.0;
                }
                Err(e) => {
                    warn!("Failed to process similar group: {}", e);
                    result.errors_encountered += 1;
                }
            }
        }

        result.execution_time_ms = start_time.elapsed().as_millis() as u64;

        // Update metrics
        self.update_metrics(&result).await;

        info!(
            "Deduplication completed: {} memories processed, {} merged, {:.2}% storage saved",
            result.total_processed,
            result.memories_merged,
            (result.storage_saved_bytes as f64 / (result.total_processed as f64 * 1024.0)) * 100.0
        );

        Ok(result)
    }

    /// Find all memories that exceed the similarity threshold
    async fn find_similar_groups(&self, memories: &[Memory]) -> Result<Vec<SimilarMemoryGroup>> {
        let mut groups = Vec::new();
        let mut processed_ids = HashSet::new();

        for (i, memory) in memories.iter().enumerate() {
            if processed_ids.contains(&memory.id) {
                continue;
            }

            let embedding = match &memory.embedding {
                Some(emb) => emb,
                None => continue,
            };

            let mut similar_memories = vec![memory.clone()];
            processed_ids.insert(memory.id);

            // Find all similar memories to this one
            for (j, other_memory) in memories.iter().enumerate() {
                if i == j || processed_ids.contains(&other_memory.id) {
                    continue;
                }

                if let Some(other_embedding) = &other_memory.embedding {
                    let similarity =
                        self.calculate_cosine_similarity(embedding, other_embedding)?;

                    if similarity >= self.config.similarity_threshold {
                        similar_memories.push(other_memory.clone());
                        processed_ids.insert(other_memory.id);
                    }
                }
            }

            // Only create groups with more than one memory
            if similar_memories.len() > 1 {
                let average_similarity = self
                    .calculate_average_similarity(&similar_memories)
                    .await?;
                let merge_strategy = self.determine_merge_strategy(&similar_memories);
                
                groups.push(SimilarMemoryGroup {
                    memories: similar_memories,
                    average_similarity,
                    merge_strategy,
                });
            }
        }

        Ok(groups)
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

    /// Process a group of similar memories for merging
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

    /// Update internal metrics
    async fn update_metrics(&self, result: &DeduplicationResult) {
        let mut metrics = self.metrics.write().await;
        metrics.total_operations += 1;
        metrics.total_memories_processed += result.total_processed;
        metrics.total_memories_merged += result.memories_merged;
        metrics.total_storage_saved += result.storage_saved_bytes;
        metrics.total_execution_time_ms += result.execution_time_ms;
        metrics.errors_encountered += result.errors_encountered;

        if result.compression_ratio > 0.0 {
            metrics.average_compression_ratio =
                (metrics.average_compression_ratio + result.compression_ratio) / 2.0;
        }
    }

    /// Get current deduplication metrics
    pub async fn get_metrics(&self) -> DeduplicationMetrics {
        self.metrics.read().await.clone()
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

    /// Create the merged memory record
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

    /// Archive the original memories
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

    /// Create a merge audit entry
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

    /// Complete a merge audit entry
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
            AND operation_type = 'merge'
            AND status = 'completed'
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

    /// Prune memories based on recall probability and age
    pub async fn prune_memories(
        &self,
        threshold: f64,
        cutoff_date: DateTime<Utc>,
    ) -> Result<PruningResult> {
        let query = r#"
            SELECT id, content, recall_probability 
            FROM memories 
            WHERE recall_probability IS NOT NULL 
            AND recall_probability < $1 
            AND created_at < $2 
            AND status = 'active'
            AND tier IN ('cold', 'frozen')
            ORDER BY recall_probability ASC
            LIMIT 1000
        "#;

        let _rows = sqlx::query(query)
            .bind(threshold)
            .bind(cutoff_date)
            .execute(self.repository.pool())
            .await?;

        let candidates = sqlx::query(query)
            .bind(threshold)
            .bind(cutoff_date)
            .fetch_all(self.repository.pool())
            .await?;

        let mut pruned_ids = Vec::new();
        let mut storage_freed = 0u64;

        for row in candidates {
            let memory_id: Uuid = row.get("id");
            let content: String = row.get("content");

            storage_freed += content.len() as u64;
            pruned_ids.push(memory_id);

            // Mark as deleted rather than hard delete
            sqlx::query("UPDATE memories SET status = 'deleted', updated_at = NOW() WHERE id = $1")
                .bind(memory_id)
                .execute(self.repository.pool())
                .await?;
        }

        info!(
            "Pruned {} memories, freed {} bytes",
            pruned_ids.len(),
            storage_freed
        );

        Ok(PruningResult {
            memories_pruned: pruned_ids.len(),
            storage_freed,
            pruned_memory_ids: pruned_ids,
        })
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
