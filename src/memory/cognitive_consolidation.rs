//! Cognitive Science-Based Memory Consolidation Implementation
//!
//! This module implements memory consolidation mechanics based on established
//! cognitive science research, particularly focusing on the spacing effect,
//! strength-dependent forgetting, and semantic clustering principles.
//!
//! ## Research Foundation
//!
//! ### Core Principles
//! 1. **Ebbinghaus Forgetting Curve (1885)**: Exponential decay of memory strength
//! 2. **Spacing Effect (Cepeda et al., 2006)**: Distributed practice enhances retention
//! 3. **Testing Effect (Roediger & Karpicke, 2006)**: Retrieval practice strengthens memories
//! 4. **Strength-Dependent Forgetting (Wickelgren, 1974)**: Strong memories decay more slowly
//! 5. **Semantic Network Theory (Collins & Loftus, 1975)**: Memories form interconnected networks
//!
//! ## Mathematical Models
//!
//! ### Enhanced Recall Probability (Cognitive Factors Applied to Ebbinghaus Base)
//! ```text
//! P(recall) = R_base(t) × cognitive_factors
//! where R_base(t) = e^(-t/S) (Ebbinghaus forgetting curve)
//! and cognitive_factors = cos_similarity × context_boost × spacing_effect × testing_effect
//! ```
//! Where:
//! - R_base(t) = standard Ebbinghaus retention curve e^(-t/S)
//! - S = consolidation strength (enhanced by cognitive factors)
//! - t = time since last access (hours)
//! - cos_similarity = semantic relatedness to current context
//! - context_boost = environmental/emotional context matching
//! - spacing_effect = benefit from optimal recall intervals
//! - testing_effect = benefit from retrieval difficulty
//!
//! ### Consolidation Strength Update
//! ```text
//! gn = gn-1 + α × (1 - e^(-βt)) / (1 + e^(-βt)) × difficulty_factor
//! ```
//! Where:
//! - α = learning rate (based on individual differences)
//! - β = spacing sensitivity parameter
//! - difficulty_factor = retrieval effort (desirable difficulty principle)

use super::error::{MemoryError, Result};
use super::math_engine::{MathEngine, MemoryParameters};
use super::models::*;
use chrono::Utc;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::postgres::types::PgInterval;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Enhanced consolidation parameters incorporating cognitive research
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveConsolidationConfig {
    /// Learning rate factor (individual differences)
    pub alpha: f64,

    /// Spacing sensitivity parameter
    pub beta: f64,

    /// Context similarity weight
    pub context_weight: f64,

    /// Semantic clustering threshold
    pub clustering_threshold: f64,

    /// Minimum consolidation interval (spacing effect)
    pub min_spacing_hours: f64,

    /// Maximum consolidation strength
    pub max_strength: f64,

    /// Difficulty scaling factor
    pub difficulty_scaling: f64,
}

impl Default for CognitiveConsolidationConfig {
    fn default() -> Self {
        Self {
            alpha: 0.3,                 // Conservative learning rate
            beta: 1.5,                  // Moderate spacing sensitivity
            context_weight: 0.2,        // Moderate context influence
            clustering_threshold: 0.75, // High similarity for clustering
            min_spacing_hours: 0.5,     // 30 minutes minimum spacing
            max_strength: 15.0,         // Higher ceiling for expertise
            difficulty_scaling: 1.2,    // Slight boost for difficult retrievals
        }
    }
}

/// Retrieval context for consolidation calculations
#[derive(Debug, Clone)]
pub struct RetrievalContext {
    pub query_embedding: Option<Vector>,
    pub environmental_factors: HashMap<String, f64>,
    pub retrieval_latency_ms: u64,
    pub confidence_score: f64,
    pub related_memories: Vec<Uuid>,
}

/// Enhanced consolidation result with cognitive metrics
#[derive(Debug, Clone)]
pub struct CognitiveConsolidationResult {
    pub new_consolidation_strength: f64,
    pub strength_increment: f64,
    pub recall_probability: f64,
    pub spacing_bonus: f64,
    pub difficulty_bonus: f64,
    pub context_similarity: f64,
    pub calculation_time_ms: u64,
    pub cognitive_factors: CognitiveFactors,
}

/// Cognitive factors influencing consolidation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveFactors {
    pub spacing_effect_strength: f64,
    pub testing_effect_strength: f64,
    pub semantic_clustering_bonus: f64,
    pub context_dependent_boost: f64,
    pub interference_penalty: f64,
}

/// Cognitive consolidation engine implementing research-backed algorithms
pub struct CognitiveConsolidationEngine {
    config: CognitiveConsolidationConfig,
    math_engine: MathEngine,
}

impl CognitiveConsolidationEngine {
    pub fn new(config: CognitiveConsolidationConfig) -> Self {
        Self {
            config,
            math_engine: MathEngine::new(),
        }
    }

    /// Calculate enhanced consolidation with cognitive factors
    ///
    /// This method implements the complete cognitive model including:
    /// - Spacing effect calculations
    /// - Testing effect strength
    /// - Semantic similarity bonuses
    /// - Context-dependent memory effects
    /// - Interference calculations
    pub async fn calculate_cognitive_consolidation(
        &self,
        memory: &Memory,
        context: &RetrievalContext,
        similar_memories: &[Memory],
    ) -> Result<CognitiveConsolidationResult> {
        let start_time = std::time::Instant::now();

        // Calculate base Ebbinghaus retention using math engine
        let params = MemoryParameters {
            consolidation_strength: memory.consolidation_strength,
            decay_rate: memory.decay_rate,
            last_accessed_at: memory.last_accessed_at,
            created_at: memory.created_at,
            access_count: memory.access_count,
            importance_score: memory.importance_score,
        };

        let base_recall = self.math_engine.calculate_recall_probability(&params)?;

        // Calculate spacing effect strength
        let spacing_effect = self.calculate_spacing_effect(memory)?;

        // Calculate testing effect based on retrieval difficulty
        let testing_effect = self.calculate_testing_effect(context)?;

        // Calculate semantic clustering bonus
        let clustering_bonus =
            self.calculate_semantic_clustering_bonus(memory, similar_memories)?;

        // Calculate context-dependent memory boost
        let context_boost = self.calculate_context_boost(memory, context)?;

        // Calculate interference from similar memories
        let interference_penalty = self.calculate_interference_penalty(memory, similar_memories)?;

        // Combine all factors for enhanced consolidation strength
        let strength_increment = self.calculate_enhanced_strength_increment(
            memory,
            spacing_effect,
            testing_effect,
            clustering_bonus,
            context_boost,
            interference_penalty,
        )?;

        let new_strength = (memory.consolidation_strength + strength_increment)
            .min(self.config.max_strength)
            .max(0.1);

        // Apply cognitive factors to base Ebbinghaus retention
        let cognitive_multiplier = (1.0 + clustering_bonus + context_boost - interference_penalty)
            .max(0.1) // Prevent negative multipliers
            .min(2.0); // Cap enhancement
            
        let enhanced_retention = (base_recall.recall_probability * cognitive_multiplier)
            .max(0.0)
            .min(1.0);

        // Also recalculate with new consolidated strength for comparison
        let enhanced_params = MemoryParameters {
            consolidation_strength: new_strength,
            ..params
        };

        let _strength_based_recall = self
            .math_engine
            .calculate_recall_probability(&enhanced_params)?;

        let calculation_time = start_time.elapsed().as_millis() as u64;

        Ok(CognitiveConsolidationResult {
            new_consolidation_strength: new_strength,
            strength_increment,
            recall_probability: enhanced_retention, // Use cognitive-enhanced retention
            spacing_bonus: spacing_effect,
            difficulty_bonus: testing_effect,
            context_similarity: context_boost,
            calculation_time_ms: calculation_time,
            cognitive_factors: CognitiveFactors {
                spacing_effect_strength: spacing_effect,
                testing_effect_strength: testing_effect,
                semantic_clustering_bonus: clustering_bonus,
                context_dependent_boost: context_boost,
                interference_penalty,
            },
        })
    }

    /// Calculate spacing effect strength based on retrieval intervals
    ///
    /// Implements findings from Cepeda et al. (2006) that optimal spacing
    /// intervals depend on retention interval and create non-linear benefits.
    fn calculate_spacing_effect(&self, memory: &Memory) -> Result<f64> {
        let current_time = Utc::now();

        // Calculate time since last access
        let last_access = memory.last_accessed_at.unwrap_or(memory.created_at);
        let interval_hours = current_time
            .signed_duration_since(last_access)
            .num_seconds() as f64
            / 3600.0;

        // Spacing effect follows inverted-U curve: too short = poor, too long = forgotten
        if interval_hours < self.config.min_spacing_hours {
            // Too recent - minimal spacing benefit
            return Ok(0.1);
        }

        // Optimal spacing based on current consolidation strength
        // Stronger memories benefit from longer intervals
        let optimal_interval = memory.consolidation_strength * 24.0; // hours

        // Calculate spacing effect using research-based curve
        let spacing_ratio = interval_hours / optimal_interval;
        let spacing_effect = if spacing_ratio < 0.5 {
            // Sub-optimal: too short
            spacing_ratio * 2.0
        } else if spacing_ratio <= 2.0 {
            // Optimal range: strong spacing effect
            1.0 + (spacing_ratio - 1.0) * 0.5
        } else {
            // Too long: diminishing returns
            1.5 * (2.0 / spacing_ratio).min(1.0)
        };

        Ok(spacing_effect.max(0.1).min(2.0))
    }

    /// Calculate testing effect based on retrieval difficulty
    ///
    /// Implements findings from Roediger & Karpicke (2008) and Bjork (1994) that desirable 
    /// difficulties during retrieval enhance long-term retention. Now integrated with
    /// dedicated testing effect implementation for research compliance.
    fn calculate_testing_effect(&self, context: &RetrievalContext) -> Result<f64> {
        // Enhanced testing effect calculation based on research
        let difficulty = match context.retrieval_latency_ms {
            0..=500 => 0.2,     // Too easy - minimal benefit (automatic recall)
            501..=1500 => 0.8,  // Easy - some benefit
            1501..=3000 => 1.5, // Optimal difficulty - maximum testing effect (Roediger & Karpicke)
            3001..=6000 => 1.2, // Hard - good benefit but effortful
            6001..=10000 => 0.9, // Very hard - some benefit but approaching failure
            _ => 0.6,           // Too difficult - minimal benefit (near failure)
        };

        // Adjust by confidence - lower confidence indicates more effort and greater benefit
        let confidence_factor = 1.0 + (1.0 - context.confidence_score) * 0.4;

        // Apply research-backed multipliers (1.5x for successful retrieval as per Karpicke & Roediger 2008)
        let testing_effect_multiplier = if context.confidence_score > 0.7 { 1.5 } else { 1.2 };

        let testing_effect = difficulty * confidence_factor * testing_effect_multiplier * self.config.difficulty_scaling;

        // Ensure testing effect stays within research-validated bounds
        Ok(testing_effect.max(0.2).min(2.5))
    }

    /// Calculate semantic clustering bonus
    ///
    /// Implements semantic network theory (Collins & Loftus, 1975) where
    /// memories in dense semantic clusters are more accessible.
    fn calculate_semantic_clustering_bonus(
        &self,
        memory: &Memory,
        similar_memories: &[Memory],
    ) -> Result<f64> {
        if similar_memories.is_empty() || memory.embedding.is_none() {
            return Ok(0.0);
        }

        let memory_embedding = memory.embedding.as_ref().unwrap();
        let mut similarity_sum = 0.0;
        let mut high_similarity_count = 0;

        for similar_memory in similar_memories {
            if let Some(similar_embedding) = &similar_memory.embedding {
                // Calculate cosine similarity
                let similarity =
                    self.calculate_cosine_similarity(memory_embedding, similar_embedding)?;

                if similarity > self.config.clustering_threshold {
                    high_similarity_count += 1;
                    similarity_sum += similarity;
                }
            }
        }

        if high_similarity_count == 0 {
            return Ok(0.0);
        }

        // Clustering bonus based on density of highly similar memories
        let avg_similarity = similarity_sum / high_similarity_count as f64;
        let density_bonus = (high_similarity_count as f64).ln() / 10.0; // Logarithmic scaling

        let clustering_bonus = avg_similarity * density_bonus;

        Ok(clustering_bonus.max(0.0).min(1.0))
    }

    /// Calculate context-dependent memory boost
    ///
    /// Implements context-dependent memory effects (Godden & Baddeley, 1975)
    /// where environmental context at encoding and retrieval affects recall.
    fn calculate_context_boost(&self, memory: &Memory, context: &RetrievalContext) -> Result<f64> {
        // Extract environmental context from memory metadata
        let memory_context = memory
            .metadata
            .get("environmental_context")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_f64().map(|val| (k.clone(), val)))
                    .collect::<HashMap<String, f64>>()
            })
            .unwrap_or_default();

        if memory_context.is_empty() || context.environmental_factors.is_empty() {
            return Ok(0.0);
        }

        // Calculate context similarity using overlapping factors
        let mut context_similarity = 0.0;
        let mut matching_factors = 0;

        for (factor, current_value) in &context.environmental_factors {
            if let Some(memory_value) = memory_context.get(factor) {
                let factor_similarity = 1.0 - (current_value - memory_value).abs().min(1.0);
                context_similarity += factor_similarity;
                matching_factors += 1;
            }
        }

        if matching_factors == 0 {
            return Ok(0.0);
        }

        let avg_context_similarity = context_similarity / matching_factors as f64;
        let context_boost = avg_context_similarity * self.config.context_weight;

        Ok(context_boost.max(0.0).min(0.5))
    }

    /// Calculate interference penalty from similar memories
    ///
    /// Implements interference theory where similar memories can compete
    /// and reduce recall probability.
    fn calculate_interference_penalty(
        &self,
        memory: &Memory,
        similar_memories: &[Memory],
    ) -> Result<f64> {
        if similar_memories.is_empty() || memory.embedding.is_none() {
            return Ok(0.0);
        }

        let memory_embedding = memory.embedding.as_ref().unwrap();
        let mut interference_total = 0.0;

        for similar_memory in similar_memories {
            if similar_memory.id == memory.id {
                continue; // Skip self
            }

            if let Some(similar_embedding) = &similar_memory.embedding {
                let similarity =
                    self.calculate_cosine_similarity(memory_embedding, similar_embedding)?;

                // Higher similarity with stronger memories creates more interference
                let strength_ratio =
                    similar_memory.consolidation_strength / memory.consolidation_strength;
                let interference_strength = similarity * strength_ratio.min(2.0);

                interference_total += interference_strength;
            }
        }

        // Interference is logarithmically scaled to prevent excessive penalties
        let interference_penalty = (1.0 + interference_total).ln() / 10.0;

        Ok(interference_penalty.max(0.0).min(0.3))
    }

    /// Combine all cognitive factors into enhanced strength increment
    fn calculate_enhanced_strength_increment(
        &self,
        memory: &Memory,
        spacing_effect: f64,
        testing_effect: f64,
        clustering_bonus: f64,
        context_boost: f64,
        interference_penalty: f64,
    ) -> Result<f64> {
        // Base increment using hyperbolic tangent growth
        let base_increment = if let Some(last_access) = memory.last_accessed_at {
            let hours_since_access =
                Utc::now().signed_duration_since(last_access).num_seconds() as f64 / 3600.0;
            let base = (1.0 - (-self.config.beta * hours_since_access).exp())
                / (1.0 + (-self.config.beta * hours_since_access).exp());
            self.config.alpha * base
        } else {
            self.config.alpha * 0.5 // Default for never-accessed memories
        };

        // Apply cognitive factors multiplicatively
        let cognitive_multiplier = spacing_effect
            * testing_effect
            * (1.0 + clustering_bonus + context_boost - interference_penalty);

        let enhanced_increment = base_increment * cognitive_multiplier;

        // Ensure reasonable bounds
        Ok(enhanced_increment.max(0.01).min(2.0))
    }

    /// Calculate cosine similarity between two vectors
    fn calculate_cosine_similarity(&self, vec1: &Vector, vec2: &Vector) -> Result<f64> {
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

    /// Update memory with cognitive consolidation results
    pub async fn apply_consolidation_results(
        &self,
        memory: &mut Memory,
        result: &CognitiveConsolidationResult,
        repository: &crate::memory::repository::MemoryRepository,
    ) -> Result<()> {
        let previous_strength = memory.consolidation_strength;
        let previous_probability = memory.recall_probability;

        // Update memory fields
        memory.consolidation_strength = result.new_consolidation_strength;
        memory.recall_probability = Some(result.recall_probability);
        memory.access_count += 1;
        memory.last_accessed_at = Some(Utc::now());

        // Create recall interval
        let recall_interval = if let Some(last_access) = memory.last_accessed_at {
            let duration = Utc::now().signed_duration_since(last_access);
            PgInterval {
                months: 0,
                days: duration.num_days() as i32,
                microseconds: (duration.num_microseconds().unwrap_or(0) % (24 * 60 * 60 * 1000000)),
            }
        } else {
            PgInterval {
                months: 0,
                days: 0,
                microseconds: 0,
            }
        };

        memory.last_recall_interval = Some(recall_interval);

        // Log cognitive consolidation event
        let context = serde_json::json!({
            "cognitive_factors": result.cognitive_factors,
            "spacing_bonus": result.spacing_bonus,
            "difficulty_bonus": result.difficulty_bonus,
            "context_similarity": result.context_similarity,
            "calculation_time_ms": result.calculation_time_ms
        });

        repository
            .log_consolidation_event(
                memory.id,
                "cognitive_consolidation",
                previous_strength,
                result.new_consolidation_strength,
                previous_probability,
                Some(result.recall_probability),
                Some(recall_interval),
                context,
            )
            .await?;

        info!(
            "Applied cognitive consolidation to memory {}: strength {:.3} -> {:.3}, recall {:.3}",
            memory.id,
            previous_strength,
            result.new_consolidation_strength,
            result.recall_probability
        );

        Ok(())
    }

    /// Apply testing effect to memory using dedicated testing effect implementation
    /// This method integrates the research-backed testing effect from testing_effect.rs
    /// with the cognitive consolidation system for comprehensive memory enhancement.
    pub async fn apply_testing_effect(
        &self,
        memory_id: uuid::Uuid,
        retrieval_success: bool,
        retrieval_latency_ms: u64,
        confidence_score: f64,
        query_type: crate::memory::testing_effect::RetrievalType,
        repository: Arc<crate::memory::repository::MemoryRepository>,
    ) -> Result<()> {
        use crate::memory::testing_effect::{
            RetrievalAttempt, TestingEffectConfig, TestingEffectEngine,
        };

        // Create testing effect engine with research-backed configuration
        let testing_config = TestingEffectConfig::default();
        let testing_engine = TestingEffectEngine::new(testing_config, repository);

        // Create retrieval attempt context
        let retrieval_attempt = RetrievalAttempt {
            memory_id,
            success: retrieval_success,
            retrieval_latency_ms,
            confidence_score,
            context_similarity: None,
            query_type,
            additional_context: Some(serde_json::json!({
                "integration": "cognitive_consolidation",
                "research_basis": "Roediger_Karpicke_2008",
                "consolidation_method": "enhanced_cognitive"
            })),
        };

        // Process the testing effect
        let testing_result = testing_engine
            .process_retrieval_attempt(retrieval_attempt)
            .await?;

        info!(
            "Testing effect applied via cognitive consolidation: memory={}, boost={:.2}x, next_review={:.1}d",
            memory_id,
            testing_result.consolidation_boost,
            testing_result.next_interval_days
        );

        Ok(())
    }
}

/// Cognitive consolidation service for batch processing
pub struct CognitiveConsolidationService {
    engine: CognitiveConsolidationEngine,
    repository: std::sync::Arc<crate::memory::repository::MemoryRepository>,
}

impl CognitiveConsolidationService {
    pub fn new(
        config: CognitiveConsolidationConfig,
        repository: std::sync::Arc<crate::memory::repository::MemoryRepository>,
    ) -> Self {
        Self {
            engine: CognitiveConsolidationEngine::new(config),
            repository,
        }
    }

    /// Process consolidation for a batch of memories with cognitive enhancements
    pub async fn process_batch_consolidation(
        &self,
        memory_ids: &[Uuid],
        context: &RetrievalContext,
    ) -> Result<Vec<CognitiveConsolidationResult>> {
        let mut results = Vec::with_capacity(memory_ids.len());

        for &memory_id in memory_ids {
            match self
                .process_single_memory_consolidation(memory_id, context)
                .await
            {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!(
                        "Failed to process consolidation for memory {}: {}",
                        memory_id, e
                    );
                    // Continue processing other memories
                }
            }
        }

        Ok(results)
    }

    async fn process_single_memory_consolidation(
        &self,
        memory_id: Uuid,
        context: &RetrievalContext,
    ) -> Result<CognitiveConsolidationResult> {
        // Get the target memory
        let mut memory = self.repository.get_memory(memory_id).await?;

        // Find similar memories for clustering analysis
        let similar_memories = self.find_similar_memories(&memory).await?;

        // Calculate cognitive consolidation
        let result = self
            .engine
            .calculate_cognitive_consolidation(&memory, context, &similar_memories)
            .await?;

        // Apply results to memory
        self.engine
            .apply_consolidation_results(&mut memory, &result, &self.repository)
            .await?;

        Ok(result)
    }

    async fn find_similar_memories(&self, memory: &Memory) -> Result<Vec<Memory>> {
        if memory.embedding.is_none() {
            return Ok(Vec::new());
        }

        // Find memories with similar embeddings in the same tier or adjacent tiers
        let search_request = SearchRequest {
            query_embedding: Some(memory.embedding.as_ref().unwrap().as_slice().to_vec()),
            search_type: Some(SearchType::Semantic),
            similarity_threshold: Some(0.7),
            limit: Some(20),
            tier: None, // Search across tiers
            ..Default::default()
        };

        let search_response = self.repository.search_memories(search_request).await?;

        Ok(search_response
            .results
            .into_iter()
            .filter(|result| result.memory.id != memory.id) // Exclude self
            .map(|result| result.memory)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_memory() -> Memory {
        let mut memory = Memory::default();
        memory.consolidation_strength = 2.0;
        memory.access_count = 3;
        memory.last_accessed_at = Some(Utc::now() - Duration::hours(2));
        memory.importance_score = 0.7;
        memory
    }

    fn create_test_context() -> RetrievalContext {
        RetrievalContext {
            query_embedding: None,
            environmental_factors: HashMap::new(),
            retrieval_latency_ms: 1500, // Optimal difficulty
            confidence_score: 0.8,
            related_memories: Vec::new(),
        }
    }

    #[test]
    fn test_spacing_effect_calculation() {
        let engine = CognitiveConsolidationEngine::new(CognitiveConsolidationConfig::default());
        let memory = create_test_memory();

        let spacing_effect = engine.calculate_spacing_effect(&memory).unwrap();

        // Should be positive for memory accessed 2 hours ago
        assert!(spacing_effect > 0.0);
        assert!(spacing_effect <= 2.0);
    }

    #[test]
    fn test_testing_effect_calculation() {
        let engine = CognitiveConsolidationEngine::new(CognitiveConsolidationConfig::default());
        let context = create_test_context();

        let testing_effect = engine.calculate_testing_effect(&context).unwrap();

        // Should be positive for optimal difficulty retrieval
        assert!(testing_effect > 0.0);
        assert!(testing_effect <= 2.0);
    }

    #[test]
    fn test_cognitive_factors_bounds() {
        let config = CognitiveConsolidationConfig::default();

        // Verify all parameters are within reasonable cognitive ranges
        assert!(config.alpha > 0.0 && config.alpha < 1.0);
        assert!(config.beta > 0.0 && config.beta < 5.0);
        assert!(config.context_weight >= 0.0 && config.context_weight <= 1.0);
        assert!(config.clustering_threshold > 0.5 && config.clustering_threshold <= 1.0);
    }
}
