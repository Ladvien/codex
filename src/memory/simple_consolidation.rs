//! Simple Memory Consolidation Engine
//!
//! This module implements the simplified consolidation formulas from Story 2:
//! - P(recall) = r × exp(-g × t / (1 + n)) × cos_similarity
//! - Consolidation update: gn = gn-1 + (1 - e^-t)/(1 + e^-t)
//!
//! This is intentionally simpler than the complex cognitive consolidation
//! implementation and focuses on fast, efficient batch processing.

use super::error::{MemoryError, Result};
use super::models::*;
use chrono::Utc;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Configuration for simple consolidation engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleConsolidationConfig {
    /// Base recall strength (r parameter)
    pub base_recall_strength: f64,

    /// Migration threshold for recall probability
    pub migration_threshold: f64,

    /// Maximum consolidation strength
    pub max_consolidation_strength: f64,

    /// Time scaling factor (hours to normalized units)
    pub time_scale_factor: f64,
}

impl Default for SimpleConsolidationConfig {
    fn default() -> Self {
        use super::math_engine::constants;
        Self {
            base_recall_strength: 0.95,
            migration_threshold: constants::COLD_MIGRATION_THRESHOLD,
            max_consolidation_strength: 10.0,
            time_scale_factor: 0.1, // Much slower time decay
        }
    }
}

/// Result of simple consolidation calculation
#[derive(Debug, Clone)]
pub struct SimpleConsolidationResult {
    pub new_consolidation_strength: f64,
    pub recall_probability: f64,
    pub should_migrate: bool,
    pub calculation_time_ms: u64,
    pub time_since_access_hours: f64,
}

/// Simple consolidation engine for fast batch processing
pub struct SimpleConsolidationEngine {
    config: SimpleConsolidationConfig,
}

impl SimpleConsolidationEngine {
    pub fn new(config: SimpleConsolidationConfig) -> Self {
        Self { config }
    }

    /// Calculate recall probability using Story 2 formula:
    /// P(recall) = r × exp(-g × t / (1 + n)) × cos_similarity
    pub fn calculate_recall_probability(
        &self,
        memory: &Memory,
        cos_similarity: Option<f64>,
    ) -> Result<f64> {
        let r = self.config.base_recall_strength;
        let g = memory.consolidation_strength;
        let n = memory.recall_count() as f64;

        // Calculate time since last access in hours
        let current_time = Utc::now();
        let last_access = memory.last_accessed_at.unwrap_or(memory.created_at);
        let t = current_time
            .signed_duration_since(last_access)
            .num_seconds() as f64
            / 3600.0;

        // Normalize time with scaling factor
        let t_normalized = t * self.config.time_scale_factor;

        // Base recall calculation: r × exp(-g × t / (1 + n))
        let base_recall = r * (-g * t_normalized / (1.0 + n)).exp();

        // Apply cosine similarity if available (Story 2 formula: direct multiplication)
        let similarity_factor = cos_similarity.unwrap_or(1.0);

        // Story 2 formula: P(recall) = r × exp(-g × t / (1 + n)) × cos_similarity
        let recall_probability = base_recall * similarity_factor;

        // Ensure bounds [0, 1]
        Ok(recall_probability.max(0.0).min(1.0))
    }

    /// Update consolidation strength using Story 2 formula:
    /// gn = gn-1 + (1 - e^-t)/(1 + e^-t)
    pub fn update_consolidation_strength(
        &self,
        current_strength: f64,
        time_since_access_hours: f64,
    ) -> Result<f64> {
        let t = time_since_access_hours * self.config.time_scale_factor;

        // Calculate strength increment: (1 - e^-t)/(1 + e^-t)
        let exp_neg_t = (-t).exp();
        let increment = (1.0 - exp_neg_t) / (1.0 + exp_neg_t);

        let new_strength = current_strength + increment;

        // Apply bounds
        Ok(new_strength
            .max(0.1)
            .min(self.config.max_consolidation_strength))
    }

    /// Process consolidation for a single memory
    pub fn process_memory_consolidation(
        &self,
        memory: &Memory,
        cos_similarity: Option<f64>,
    ) -> Result<SimpleConsolidationResult> {
        let start_time = Instant::now();

        // Calculate time since last access
        let current_time = Utc::now();
        let last_access = memory.last_accessed_at.unwrap_or(memory.created_at);
        let time_since_access_hours = current_time
            .signed_duration_since(last_access)
            .num_seconds() as f64
            / 3600.0;

        // Update consolidation strength
        let new_consolidation_strength = self.update_consolidation_strength(
            memory.consolidation_strength,
            time_since_access_hours,
        )?;

        // Create temporary memory with updated strength for recall calculation
        let mut updated_memory = memory.clone();
        updated_memory.consolidation_strength = new_consolidation_strength;

        // Calculate recall probability
        let recall_probability =
            self.calculate_recall_probability(&updated_memory, cos_similarity)?;

        // Check if migration is needed
        let should_migrate = recall_probability < self.config.migration_threshold;

        let calculation_time = start_time.elapsed().as_millis() as u64;

        Ok(SimpleConsolidationResult {
            new_consolidation_strength,
            recall_probability,
            should_migrate,
            calculation_time_ms: calculation_time,
            time_since_access_hours,
        })
    }

    /// Process consolidation for a batch of memories
    /// Target: Process 1000 memories in < 1 second
    pub fn process_batch_consolidation(
        &self,
        memories: &[Memory],
        similarities: Option<&[f64]>,
    ) -> Result<Vec<SimpleConsolidationResult>> {
        let start_time = Instant::now();
        let mut results = Vec::with_capacity(memories.len());

        for (i, memory) in memories.iter().enumerate() {
            let cos_similarity = similarities.and_then(|sims| sims.get(i)).copied();

            match self.process_memory_consolidation(memory, cos_similarity) {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!(
                        "Failed to process consolidation for memory {}: {}",
                        memory.id, e
                    );
                    // Continue processing other memories
                }
            }
        }

        let total_time = start_time.elapsed();
        info!(
            "Processed {} memories in {:.3}s ({:.1} memories/sec)",
            results.len(),
            total_time.as_secs_f64(),
            results.len() as f64 / total_time.as_secs_f64()
        );

        Ok(results)
    }

    /// Calculate cosine similarity between two vectors (helper function)
    pub fn calculate_cosine_similarity(&self, vec1: &Vector, vec2: &Vector) -> Result<f64> {
        let slice1 = vec1.as_slice();
        let slice2 = vec2.as_slice();

        if slice1.len() != slice2.len() {
            return Err(MemoryError::InvalidRequest {
                message: "Vector dimensions must match for similarity calculation".to_string(),
            });
        }

        let dot_product: f64 = slice1
            .iter()
            .zip(slice2.iter())
            .map(|(a, b)| (*a as f64) * (*b as f64))
            .sum();

        let norm1: f64 = slice1
            .iter()
            .map(|x| (*x as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        let norm2: f64 = slice2
            .iter()
            .map(|x| (*x as f64).powi(2))
            .sum::<f64>()
            .sqrt();

        if norm1 == 0.0 || norm2 == 0.0 {
            return Ok(0.0);
        }

        Ok(dot_product / (norm1 * norm2))
    }

    /// Get migration candidates based on recall probability threshold
    pub fn get_migration_candidates(
        &self,
        memories: &[Memory],
    ) -> Result<Vec<(usize, MemoryTier)>> {
        let mut candidates = Vec::new();

        for (i, memory) in memories.iter().enumerate() {
            let result = self.process_memory_consolidation(memory, None)?;

            if result.should_migrate {
                if let Some(next_tier) = memory.next_tier() {
                    candidates.push((i, next_tier));
                }
            }
        }

        Ok(candidates)
    }
}

/// Background consolidation processor for efficient batch processing
pub struct ConsolidationProcessor {
    engine: SimpleConsolidationEngine,
    batch_size: usize,
}

impl ConsolidationProcessor {
    pub fn new(config: SimpleConsolidationConfig, batch_size: usize) -> Self {
        Self {
            engine: SimpleConsolidationEngine::new(config),
            batch_size,
        }
    }

    /// Process memories in batches to maintain performance targets
    pub async fn process_consolidation_batch(
        &self,
        repository: &crate::memory::repository::MemoryRepository,
        tier: Option<MemoryTier>,
    ) -> Result<ConsolidationBatchResult> {
        let start_time = Instant::now();

        // Get memories to process
        let memories = self.get_memories_for_processing(repository, tier).await?;

        if memories.is_empty() {
            debug!("No memories found for consolidation processing");
            return Ok(ConsolidationBatchResult::default());
        }

        debug!("Processing consolidation for {} memories", memories.len());

        // Process in batches
        let mut processed_count = 0;
        let mut migration_candidates = Vec::new();
        let mut consolidation_updates = Vec::new();

        for chunk in memories.chunks(self.batch_size) {
            let results = self.engine.process_batch_consolidation(chunk, None)?;

            for (memory, result) in chunk.iter().zip(results.iter()) {
                processed_count += 1;

                // Collect migration candidates
                if result.should_migrate {
                    if let Some(next_tier) = memory.next_tier() {
                        migration_candidates.push((memory.id, next_tier));
                    }
                }

                // Collect consolidation updates
                consolidation_updates.push((
                    memory.id,
                    result.new_consolidation_strength,
                    result.recall_probability,
                ));
            }
        }

        let total_time = start_time.elapsed();

        Ok(ConsolidationBatchResult {
            processed_count,
            migration_candidates,
            consolidation_updates,
            processing_time_ms: total_time.as_millis() as u64,
        })
    }

    async fn get_memories_for_processing(
        &self,
        repository: &crate::memory::repository::MemoryRepository,
        tier: Option<MemoryTier>,
    ) -> Result<Vec<Memory>> {
        let tier_filter = if let Some(tier) = tier {
            format!("AND tier = '{:?}'", tier).to_lowercase()
        } else {
            String::new()
        };

        let query = format!(
            r#"
            SELECT * FROM memories 
            WHERE status = 'active' 
            AND (last_accessed_at IS NULL OR last_accessed_at < NOW() - INTERVAL '1 hour')
            {}
            ORDER BY last_accessed_at ASC NULLS FIRST
            LIMIT $1
            "#,
            tier_filter
        );

        let memories = sqlx::query_as::<_, Memory>(&query)
            .bind(self.batch_size as i64)
            .fetch_all(repository.pool())
            .await?;

        Ok(memories)
    }
}

/// Result of batch consolidation processing
#[derive(Debug, Clone, Default)]
pub struct ConsolidationBatchResult {
    pub processed_count: usize,
    pub migration_candidates: Vec<(Uuid, MemoryTier)>,
    pub consolidation_updates: Vec<(Uuid, f64, f64)>, // (id, new_strength, recall_prob)
    pub processing_time_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_memory() -> Memory {
        let mut memory = Memory::default();
        memory.consolidation_strength = 2.0;
        memory.access_count = 5;
        memory.last_accessed_at = Some(Utc::now() - Duration::hours(2));
        memory.importance_score = 0.7;
        memory
    }

    #[test]
    fn test_recall_probability_calculation() {
        let engine = SimpleConsolidationEngine::new(SimpleConsolidationConfig::default());
        let memory = create_test_memory();

        let recall_prob = engine
            .calculate_recall_probability(&memory, Some(0.8))
            .unwrap();

        assert!(recall_prob >= 0.0 && recall_prob <= 1.0);
        assert!(recall_prob > 0.0); // Should have some recall probability
    }

    #[test]
    fn test_consolidation_strength_update() {
        let engine = SimpleConsolidationEngine::new(SimpleConsolidationConfig::default());

        let new_strength = engine.update_consolidation_strength(2.0, 1.0).unwrap();

        assert!(new_strength > 2.0); // Should increase
        assert!(new_strength <= 10.0); // Should respect max bound
    }

    #[test]
    fn test_migration_threshold() {
        let mut config = SimpleConsolidationConfig::default();
        config.migration_threshold = 0.5;

        let engine = SimpleConsolidationEngine::new(config);
        let memory = create_test_memory();

        let result = engine.process_memory_consolidation(&memory, None).unwrap();

        // Migration decision should be based on recall probability vs threshold
        assert_eq!(result.should_migrate, result.recall_probability < 0.5);
    }

    #[test]
    fn test_batch_processing_performance() {
        let engine = SimpleConsolidationEngine::new(SimpleConsolidationConfig::default());

        // Create test batch
        let memories: Vec<Memory> = (0..100).map(|_| create_test_memory()).collect();

        let start = Instant::now();
        let results = engine.process_batch_consolidation(&memories, None).unwrap();
        let duration = start.elapsed();

        assert_eq!(results.len(), 100);
        assert!(duration.as_millis() < 100); // Should be fast for 100 memories
    }
}
