//! Three-Component Memory Scoring System
//!
//! This module implements the proven three-component formula from Generative Agents research:
//! Combined Score = α × recency + β × importance + γ × relevance
//!
//! ## Cognitive Science Foundation
//!
//! ### Research Basis
//! 1. **Park et al. (2023) - Generative Agents**: Empirically validated three-component formula
//! 2. **Anderson & Schooler (1991) - Rational Analysis**: Memory importance reflects environmental need
//! 3. **Brown et al. (2007) - Spacing Effect**: Recency with exponential decay mirrors forgetting curves
//! 4. **Tulving (1983) - Episodic Memory**: Context-dependent retrieval enhanced by relevance
//!
//! ### Mathematical Model
//!
//! #### Recency Score
//! ```text
//! R(t) = e^(-λt)
//! ```
//! Where:
//! - λ = decay constant (default: 0.005 per hour)
//! - t = hours since last access (or creation)
//!
//! #### Relevance Score  
//! ```text
//! V(context) = 0.6 × cos_sim(embedding, context) + 0.25 × importance + 0.15 × access_pattern
//! ```
//! Where:
//! - cos_sim = cosine similarity to current context
//! - access_pattern = normalized access frequency
//!
//! #### Combined Score
//! ```text
//! S = α × R(t) + β × I + γ × V(context)
//! ```
//! Default weights: α = β = γ = 0.333 (equal weighting)
//!
//! ## Performance Requirements
//! - Real-time scoring: <5ms per memory
//! - Batch updates: 1000 memories per second
//! - Configurable weights via environment variables
//! - Automatic score updates on memory access

use super::error::{MemoryError, Result};
use super::models::*;
use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Configuration for three-component scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeComponentConfig {
    /// Weight for recency component (α)
    pub recency_weight: f64,

    /// Weight for importance component (β)  
    pub importance_weight: f64,

    /// Weight for relevance component (γ)
    pub relevance_weight: f64,

    /// Decay constant for recency calculation (λ per hour)
    pub decay_lambda: f64,

    /// Context similarity weight in relevance calculation
    pub context_similarity_weight: f64,

    /// Access pattern weight in relevance calculation
    pub access_pattern_weight: f64,

    /// Importance factor weight in relevance calculation
    pub importance_factor_weight: f64,

    /// Maximum access count for normalization
    pub max_access_count_for_norm: i32,

    /// Enable automatic score updates on access
    pub auto_update_on_access: bool,

    /// Performance target for single score calculation (ms)
    pub performance_target_ms: u64,
}

impl Default for ThreeComponentConfig {
    fn default() -> Self {
        Self {
            recency_weight: 0.333,
            importance_weight: 0.333,
            relevance_weight: 0.334,
            decay_lambda: 0.005,
            context_similarity_weight: 0.6,
            access_pattern_weight: 0.15,
            importance_factor_weight: 0.25,
            max_access_count_for_norm: 100,
            auto_update_on_access: true,
            performance_target_ms: 5,
        }
    }
}

impl ThreeComponentConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(val) = env::var("MEMORY_RECENCY_WEIGHT") {
            if let Ok(weight) = val.parse::<f64>() {
                config.recency_weight = weight;
            }
        }

        if let Ok(val) = env::var("MEMORY_IMPORTANCE_WEIGHT") {
            if let Ok(weight) = val.parse::<f64>() {
                config.importance_weight = weight;
            }
        }

        if let Ok(val) = env::var("MEMORY_RELEVANCE_WEIGHT") {
            if let Ok(weight) = val.parse::<f64>() {
                config.relevance_weight = weight;
            }
        }

        if let Ok(val) = env::var("MEMORY_DECAY_LAMBDA") {
            if let Ok(lambda) = val.parse::<f64>() {
                config.decay_lambda = lambda;
            }
        }

        // Normalize weights to sum to 1.0
        config.normalize_weights();

        config
    }

    /// Normalize weights to sum to 1.0
    pub fn normalize_weights(&mut self) {
        let total = self.recency_weight + self.importance_weight + self.relevance_weight;
        if total > 0.0 {
            self.recency_weight /= total;
            self.importance_weight /= total;
            self.relevance_weight /= total;
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<()> {
        if self.recency_weight < 0.0 || self.importance_weight < 0.0 || self.relevance_weight < 0.0
        {
            return Err(MemoryError::InvalidRequest {
                message: "All weights must be non-negative".to_string(),
            });
        }

        let total_weight = self.recency_weight + self.importance_weight + self.relevance_weight;
        if (total_weight - 1.0).abs() > 0.001 {
            return Err(MemoryError::InvalidRequest {
                message: format!("Weights must sum to 1.0, got {total_weight:.3}"),
            });
        }

        if self.decay_lambda <= 0.0 {
            return Err(MemoryError::InvalidRequest {
                message: "Decay lambda must be positive".to_string(),
            });
        }

        Ok(())
    }
}

/// Context for relevance scoring
#[derive(Debug, Clone)]
pub struct ScoringContext {
    /// Query embedding for semantic similarity
    pub query_embedding: Option<Vector>,

    /// Environmental context factors
    pub context_factors: HashMap<String, f64>,

    /// Temporal context (time of query)
    pub query_time: DateTime<Utc>,

    /// User preferences or biases
    pub user_preferences: HashMap<String, f64>,
}

impl Default for ScoringContext {
    fn default() -> Self {
        Self {
            query_embedding: None,
            context_factors: HashMap::new(),
            query_time: Utc::now(),
            user_preferences: HashMap::new(),
        }
    }
}

/// Result of three-component scoring
#[derive(Debug, Clone, PartialEq)]
pub struct ScoringResult {
    pub recency_score: f64,
    pub importance_score: f64,
    pub relevance_score: f64,
    pub combined_score: f64,
    pub calculation_time_ms: u64,
    pub score_explanation: Option<ScoreBreakdown>,
}

/// Detailed breakdown of score calculation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub recency_contribution: f64,
    pub importance_contribution: f64,
    pub relevance_contribution: f64,
    pub recency_details: RecencyDetails,
    pub relevance_details: RelevanceDetails,
    pub weights_used: WeightsUsed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecencyDetails {
    pub hours_since_last_access: f64,
    pub decay_applied: f64,
    pub reference_time: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelevanceDetails {
    pub semantic_similarity: Option<f64>,
    pub access_pattern_score: f64,
    pub importance_factor: f64,
    pub context_factors_applied: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeightsUsed {
    pub recency_weight: f64,
    pub importance_weight: f64,
    pub relevance_weight: f64,
}

/// Three-component scoring engine
pub struct ThreeComponentEngine {
    config: ThreeComponentConfig,
}

impl ThreeComponentEngine {
    /// Create new scoring engine with configuration
    pub fn new(config: ThreeComponentConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Create engine with default configuration
    pub fn default() -> Self {
        Self {
            config: ThreeComponentConfig::default(),
        }
    }

    /// Create engine with environment-based configuration
    pub fn from_env() -> Result<Self> {
        let config = ThreeComponentConfig::from_env();
        config.validate()?;
        Ok(Self { config })
    }

    /// Calculate three-component score for a memory
    pub fn calculate_score(
        &self,
        memory: &Memory,
        context: &ScoringContext,
        explain: bool,
    ) -> Result<ScoringResult> {
        let start_time = Instant::now();

        // Calculate recency score
        let (recency_score, recency_details) = self.calculate_recency_score(memory)?;

        // Calculate relevance score
        let (relevance_score, relevance_details) =
            self.calculate_relevance_score(memory, context)?;

        // Importance score is already stored in memory
        let importance_score = memory.importance_score;

        // Calculate combined score
        let combined_score = self.config.recency_weight * recency_score
            + self.config.importance_weight * importance_score
            + self.config.relevance_weight * relevance_score;

        let calculation_time = start_time.elapsed().as_millis() as u64;

        // Check performance target
        if calculation_time > self.config.performance_target_ms {
            warn!(
                "Score calculation exceeded target: {}ms > {}ms for memory {}",
                calculation_time, self.config.performance_target_ms, memory.id
            );
        }

        let score_explanation = if explain {
            Some(ScoreBreakdown {
                recency_contribution: self.config.recency_weight * recency_score,
                importance_contribution: self.config.importance_weight * importance_score,
                relevance_contribution: self.config.relevance_weight * relevance_score,
                recency_details,
                relevance_details,
                weights_used: WeightsUsed {
                    recency_weight: self.config.recency_weight,
                    importance_weight: self.config.importance_weight,
                    relevance_weight: self.config.relevance_weight,
                },
            })
        } else {
            None
        };

        Ok(ScoringResult {
            recency_score,
            importance_score,
            relevance_score,
            combined_score: combined_score.max(0.0).min(1.0), // Ensure bounds
            calculation_time_ms: calculation_time,
            score_explanation,
        })
    }

    /// Calculate recency score using exponential decay
    fn calculate_recency_score(&self, memory: &Memory) -> Result<(f64, RecencyDetails)> {
        // Use last accessed time if available, otherwise creation time
        let reference_time = memory.last_accessed_at.unwrap_or(memory.created_at);

        // Calculate hours elapsed
        let hours_elapsed = Utc::now()
            .signed_duration_since(reference_time)
            .num_seconds() as f64
            / 3600.0;

        // Apply exponential decay: e^(-λt)
        let decay_applied = (-self.config.decay_lambda * hours_elapsed).exp();
        let recency_score = decay_applied.max(0.0).min(1.0);

        let details = RecencyDetails {
            hours_since_last_access: hours_elapsed,
            decay_applied,
            reference_time,
        };

        Ok((recency_score, details))
    }

    /// Calculate relevance score based on context
    fn calculate_relevance_score(
        &self,
        memory: &Memory,
        context: &ScoringContext,
    ) -> Result<(f64, RelevanceDetails)> {
        // Calculate semantic similarity if embeddings are available
        let semantic_similarity = if let (Some(memory_embedding), Some(query_embedding)) =
            (&memory.embedding, &context.query_embedding)
        {
            Some(self.calculate_cosine_similarity(memory_embedding, query_embedding)?)
        } else {
            None
        };

        // Calculate access pattern score (normalized frequency)
        let access_pattern_score = (memory.access_count as f64
            / self.config.max_access_count_for_norm as f64)
            .min(1.0)
            .max(0.0);

        // Use importance as a factor in relevance
        let importance_factor = memory.importance_score;

        // Combine relevance factors
        let relevance_score = if let Some(similarity) = semantic_similarity {
            // Full relevance calculation with semantic similarity
            self.config.context_similarity_weight * similarity
                + self.config.importance_factor_weight * importance_factor
                + self.config.access_pattern_weight * access_pattern_score
        } else {
            // Fallback relevance without semantic similarity
            0.5 * importance_factor + 0.3 * access_pattern_score + 0.2 // Base relevance
        };

        let details = RelevanceDetails {
            semantic_similarity,
            access_pattern_score,
            importance_factor,
            context_factors_applied: context.context_factors.keys().cloned().collect(),
        };

        Ok((relevance_score.max(0.0).min(1.0), details))
    }

    /// Calculate cosine similarity between two vectors
    fn calculate_cosine_similarity(&self, vec1: &Vector, vec2: &Vector) -> Result<f64> {
        let slice1 = vec1.as_slice();
        let slice2 = vec2.as_slice();

        if slice1.len() != slice2.len() {
            return Ok(0.0); // Return neutral similarity for mismatched dimensions
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

        let similarity = dot_product / (norm1 * norm2);
        // Normalize from [-1, 1] to [0, 1] for scoring
        let normalized_similarity = (similarity.max(-1.0).min(1.0) + 1.0) / 2.0;
        Ok(normalized_similarity)
    }

    /// Batch calculate scores for multiple memories
    pub fn batch_calculate_scores(
        &self,
        memories: &[Memory],
        context: &ScoringContext,
        explain: bool,
    ) -> Result<Vec<ScoringResult>> {
        let start_time = Instant::now();
        let mut results = Vec::with_capacity(memories.len());

        for memory in memories {
            match self.calculate_score(memory, context, explain) {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("Failed to calculate score for memory {}: {}", memory.id, e);
                    // Add default result to maintain index alignment
                    results.push(ScoringResult {
                        recency_score: 0.0,
                        importance_score: memory.importance_score,
                        relevance_score: 0.5,
                        combined_score: memory.importance_score * 0.333 + 0.167, // Fallback
                        calculation_time_ms: 0,
                        score_explanation: None,
                    });
                }
            }
        }

        let total_time = start_time.elapsed().as_millis() as u64;
        let avg_time = if !results.is_empty() {
            total_time / results.len() as u64
        } else {
            0
        };

        debug!(
            "Batch scored {} memories in {}ms (avg {}ms per memory)",
            memories.len(),
            total_time,
            avg_time
        );

        Ok(results)
    }

    /// Update memory with calculated three-component scores
    pub fn update_memory_scores(
        &self,
        memory: &mut Memory,
        context: &ScoringContext,
    ) -> Result<ScoringResult> {
        let result = self.calculate_score(memory, context, false)?;

        // Update memory with calculated scores
        memory.recency_score = result.recency_score;
        memory.relevance_score = result.relevance_score;
        memory.updated_at = Utc::now();

        // Add scoring metadata for audit trail
        let scoring_metadata = serde_json::json!({
            "last_score_update": Utc::now(),
            "combined_score": result.combined_score,
            "scoring_config": {
                "recency_weight": self.config.recency_weight,
                "importance_weight": self.config.importance_weight,
                "relevance_weight": self.config.relevance_weight,
                "decay_lambda": self.config.decay_lambda
            }
        });

        // Merge with existing metadata
        if let serde_json::Value::Object(ref mut map) = &mut memory.metadata {
            if let serde_json::Value::Object(scoring_map) = scoring_metadata {
                for (key, value) in scoring_map {
                    map.insert(key, value);
                }
            }
        }

        Ok(result)
    }

    /// Get current configuration
    pub fn config(&self) -> &ThreeComponentConfig {
        &self.config
    }

    /// Update configuration (useful for A/B testing)
    pub fn update_config(&mut self, config: ThreeComponentConfig) -> Result<()> {
        config.validate()?;
        self.config = config;
        info!("Updated three-component scoring configuration");
        Ok(())
    }
}

/// Enhanced search request with three-component scoring
#[derive(Debug, Clone)]
pub struct EnhancedSearchRequest {
    pub base_request: SearchRequest,
    pub scoring_context: ScoringContext,
    pub use_combined_score: bool,
    pub explain_scores: bool,
    pub score_threshold: Option<f64>,
}

/// Enhanced search result with three-component scores
#[derive(Debug, Clone)]
pub struct EnhancedSearchResult {
    pub memory: Memory,
    pub scoring_result: ScoringResult,
    pub rank_position: usize,
    pub original_similarity: f32,
}

/// Search service with three-component scoring integration
pub struct EnhancedSearchService {
    scoring_engine: ThreeComponentEngine,
}

impl EnhancedSearchService {
    pub fn new(config: ThreeComponentConfig) -> Result<Self> {
        Ok(Self {
            scoring_engine: ThreeComponentEngine::new(config)?,
        })
    }

    /// Rank search results using three-component scoring
    pub fn rank_search_results(
        &self,
        search_results: Vec<SearchResult>,
        context: &ScoringContext,
        explain_scores: bool,
    ) -> Result<Vec<EnhancedSearchResult>> {
        let mut enhanced_results = Vec::with_capacity(search_results.len());

        // Calculate three-component scores for all results
        for result in search_results {
            let scoring_result =
                self.scoring_engine
                    .calculate_score(&result.memory, context, explain_scores)?;

            enhanced_results.push(EnhancedSearchResult {
                memory: result.memory,
                scoring_result,
                rank_position: 0, // Will be set after sorting
                original_similarity: result.similarity_score,
            });
        }

        // Sort by combined score (descending)
        enhanced_results.sort_by(|a, b| {
            b.scoring_result
                .combined_score
                .partial_cmp(&a.scoring_result.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Update rank positions
        for (i, result) in enhanced_results.iter_mut().enumerate() {
            result.rank_position = i + 1;
        }

        Ok(enhanced_results)
    }

    /// Filter results by score threshold
    pub fn filter_by_score_threshold(
        &self,
        results: Vec<EnhancedSearchResult>,
        threshold: f64,
    ) -> Vec<EnhancedSearchResult> {
        results
            .into_iter()
            .filter(|result| result.scoring_result.combined_score >= threshold)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_memory() -> Memory {
        let mut memory = Memory::default();
        memory.importance_score = 0.7;
        memory.access_count = 5;
        memory.last_accessed_at = Some(Utc::now() - Duration::hours(2));
        memory
    }

    fn create_test_context() -> ScoringContext {
        ScoringContext {
            query_embedding: None,
            context_factors: HashMap::new(),
            query_time: Utc::now(),
            user_preferences: HashMap::new(),
        }
    }

    #[test]
    fn test_three_component_config_validation() {
        let mut config = ThreeComponentConfig::default();
        assert!(config.validate().is_ok());

        // Test invalid weights
        config.recency_weight = -0.1;
        assert!(config.validate().is_err());

        // Test weights not summing to 1
        config.recency_weight = 0.5;
        config.importance_weight = 0.5;
        config.relevance_weight = 0.5; // Sum = 1.5
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_weight_normalization_basic() {
        let mut config = ThreeComponentConfig {
            recency_weight: 2.0,
            importance_weight: 3.0,
            relevance_weight: 1.0,
            ..Default::default()
        };

        config.normalize_weights();

        assert!((config.recency_weight - 0.333).abs() < 0.01);
        assert!((config.importance_weight - 0.5).abs() < 0.01);
        assert!((config.relevance_weight - 0.167).abs() < 0.01);

        let sum = config.recency_weight + config.importance_weight + config.relevance_weight;
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_recency_score_calculation() {
        let engine = ThreeComponentEngine::default();
        let memory = create_test_memory();

        let (recency_score, details) = engine.calculate_recency_score(&memory).unwrap();

        // Recent access (2 hours ago) should have high recency
        assert!(recency_score > 0.99);
        assert!(recency_score <= 1.0);
        assert_eq!(details.hours_since_last_access, 2.0);
    }

    #[test]
    fn test_relevance_score_calculation() {
        let engine = ThreeComponentEngine::default();
        let memory = create_test_memory();
        let context = create_test_context();

        let (relevance_score, details) =
            engine.calculate_relevance_score(&memory, &context).unwrap();

        // Should be between 0 and 1
        assert!(relevance_score >= 0.0);
        assert!(relevance_score <= 1.0);

        // Should include access pattern and importance factors
        assert!(details.access_pattern_score > 0.0);
        assert_eq!(details.importance_factor, 0.7);
    }

    #[test]
    fn test_combined_score_calculation() {
        let engine = ThreeComponentEngine::default();
        let memory = create_test_memory();
        let context = create_test_context();

        let result = engine.calculate_score(&memory, &context, true).unwrap();

        // Combined score should be weighted average
        assert!(result.combined_score >= 0.0);
        assert!(result.combined_score <= 1.0);

        // Should have explanation when requested
        assert!(result.score_explanation.is_some());

        // Performance check
        assert!(result.calculation_time_ms <= 10); // Should be very fast
    }

    #[test]
    fn test_batch_scoring() {
        let engine = ThreeComponentEngine::default();
        let memories = vec![create_test_memory(); 10];
        let context = create_test_context();

        let results = engine
            .batch_calculate_scores(&memories, &context, false)
            .unwrap();

        assert_eq!(results.len(), 10);

        // All results should be valid
        for result in &results {
            assert!(result.combined_score >= 0.0);
            assert!(result.combined_score <= 1.0);
        }
    }

    #[test]
    fn test_cosine_similarity() {
        let engine = ThreeComponentEngine::default();

        let vec1 = Vector::from(vec![1.0, 0.0, 0.0]);
        let vec2 = Vector::from(vec![1.0, 0.0, 0.0]);
        let similarity = engine.calculate_cosine_similarity(&vec1, &vec2).unwrap();
        assert!((similarity - 1.0).abs() < 0.001);

        let vec3 = Vector::from(vec![0.0, 1.0, 0.0]);
        let similarity = engine.calculate_cosine_similarity(&vec1, &vec3).unwrap();
        assert!((similarity - 0.5).abs() < 0.001); // Orthogonal vectors normalize to 0.5
    }

    #[test]
    fn test_score_bounds() {
        let engine = ThreeComponentEngine::default();
        let mut memory = create_test_memory();
        let context = create_test_context();

        // Test extreme values
        memory.importance_score = 0.0;
        memory.access_count = 0;
        memory.last_accessed_at = Some(Utc::now() - Duration::days(365)); // Very old

        let result = engine.calculate_score(&memory, &context, false).unwrap();

        // All scores should still be within bounds
        assert!(result.recency_score >= 0.0 && result.recency_score <= 1.0);
        assert!(result.importance_score >= 0.0 && result.importance_score <= 1.0);
        assert!(result.relevance_score >= 0.0 && result.relevance_score <= 1.0);
        assert!(result.combined_score >= 0.0 && result.combined_score <= 1.0);
    }

    #[test]
    fn test_recency_score_exponential_decay() {
        let engine = ThreeComponentEngine::default();
        let mut memory = create_test_memory();

        // Test immediate access (t=0)
        memory.last_accessed_at = Some(Utc::now());
        let (recency_now, _) = engine.calculate_recency_score(&memory).unwrap();
        assert!((recency_now - 1.0).abs() < 0.001); // Should be very close to 1.0

        // Test 1 hour ago (λ=0.005, so e^(-0.005*1) ≈ 0.995)
        memory.last_accessed_at = Some(Utc::now() - Duration::hours(1));
        let (recency_1h, _) = engine.calculate_recency_score(&memory).unwrap();
        assert!((recency_1h - 0.995).abs() < 0.01);

        // Test 100 hours ago (λ=0.005, so e^(-0.005*100) ≈ 0.606)
        memory.last_accessed_at = Some(Utc::now() - Duration::hours(100));
        let (recency_100h, _) = engine.calculate_recency_score(&memory).unwrap();
        assert!((recency_100h - 0.606).abs() < 0.01);

        // Test 1000 hours ago (λ=0.005, so e^(-0.005*1000) ≈ 0.007)
        memory.last_accessed_at = Some(Utc::now() - Duration::hours(1000));
        let (recency_1000h, _) = engine.calculate_recency_score(&memory).unwrap();
        assert!(recency_1000h < 0.01); // Should be very small

        // Verify exponential decay property: newer > older
        assert!(recency_now > recency_1h);
        assert!(recency_1h > recency_100h);
        assert!(recency_100h > recency_1000h);
    }

    #[test]
    fn test_relevance_score_components() {
        let engine = ThreeComponentEngine::default();
        let mut memory = create_test_memory();
        let context = create_test_context();

        // Test with different access counts
        memory.access_count = 0;
        let (relevance_0, details_0) = engine.calculate_relevance_score(&memory, &context).unwrap();

        memory.access_count = 50;
        let (relevance_50, details_50) =
            engine.calculate_relevance_score(&memory, &context).unwrap();

        memory.access_count = 100;
        let (relevance_100, details_100) =
            engine.calculate_relevance_score(&memory, &context).unwrap();

        // Higher access count should increase relevance (due to access pattern component)
        assert!(relevance_50 > relevance_0);
        assert!(relevance_100 > relevance_50);

        // Verify access pattern calculation
        assert_eq!(details_0.access_pattern_score, 0.0);
        assert_eq!(details_50.access_pattern_score, 0.5);
        assert_eq!(details_100.access_pattern_score, 1.0);
    }

    #[test]
    fn test_weight_normalization() {
        let mut config = ThreeComponentConfig {
            recency_weight: 2.0,
            importance_weight: 4.0,
            relevance_weight: 6.0,
            ..Default::default()
        };

        config.normalize_weights();

        // Weights should sum to 1.0
        let sum = config.recency_weight + config.importance_weight + config.relevance_weight;
        assert!((sum - 1.0).abs() < 0.001);

        // Verify proportional scaling
        assert!((config.recency_weight - 1.0 / 6.0).abs() < 0.001); // 2/12
        assert!((config.importance_weight - 1.0 / 3.0).abs() < 0.001); // 4/12
        assert!((config.relevance_weight - 0.5).abs() < 0.001); // 6/12
    }

    #[test]
    fn test_combined_score_weighted_average() {
        let config = ThreeComponentConfig {
            recency_weight: 0.5,
            importance_weight: 0.3,
            relevance_weight: 0.2,
            ..Default::default()
        };
        let engine = ThreeComponentEngine::new(config).unwrap();

        let mut memory = create_test_memory();
        memory.importance_score = 0.8;
        memory.last_accessed_at = Some(Utc::now()); // Recent access for high recency
        memory.access_count = 10; // Moderate access

        let context = create_test_context();
        let result = engine.calculate_score(&memory, &context, true).unwrap();

        // Manually calculate expected score
        let expected = 0.5 * result.recency_score + 0.3 * 0.8 + 0.2 * result.relevance_score;

        assert!((result.combined_score - expected).abs() < 0.001);

        // Verify explanation
        let explanation = result.score_explanation.unwrap();
        assert!((explanation.recency_contribution - 0.5 * result.recency_score).abs() < 0.001);
        assert!((explanation.importance_contribution - 0.3 * 0.8).abs() < 0.001);
        assert!((explanation.relevance_contribution - 0.2 * result.relevance_score).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_calculation() {
        let engine = ThreeComponentEngine::default();

        // Test identical vectors
        let vec1 = Vector::from(vec![1.0, 0.0, 0.0]);
        let vec2 = Vector::from(vec![1.0, 0.0, 0.0]);
        let similarity = engine.calculate_cosine_similarity(&vec1, &vec2).unwrap();
        assert!((similarity - 1.0).abs() < 0.001);

        // Test orthogonal vectors (normalize to 0.5 for [0, 1] range)
        let vec3 = Vector::from(vec![0.0, 1.0, 0.0]);
        let similarity = engine.calculate_cosine_similarity(&vec1, &vec3).unwrap();
        assert!((similarity - 0.5).abs() < 0.001);

        // Test opposite vectors (normalize to 0.0 for [0, 1] range)
        let vec4 = Vector::from(vec![-1.0, 0.0, 0.0]);
        let similarity = engine.calculate_cosine_similarity(&vec1, &vec4).unwrap();
        assert!((similarity - 0.0).abs() < 0.001);

        // Test mismatched dimensions (should return 0.0)
        let vec5 = Vector::from(vec![1.0, 0.0]);
        let similarity = engine.calculate_cosine_similarity(&vec1, &vec5).unwrap();
        assert_eq!(similarity, 0.0);

        // Test zero vectors (should return 0.0)
        let vec6 = Vector::from(vec![0.0, 0.0, 0.0]);
        let similarity = engine.calculate_cosine_similarity(&vec1, &vec6).unwrap();
        assert_eq!(similarity, 0.0);
    }

    #[test]
    fn test_memory_score_updates() {
        let engine = ThreeComponentEngine::default();
        let mut memory = create_test_memory();
        let context = create_test_context();

        // Store original scores
        let original_recency = memory.recency_score;
        let original_relevance = memory.relevance_score;

        let result = engine.update_memory_scores(&mut memory, &context).unwrap();

        // Scores should be updated
        assert_ne!(memory.recency_score, original_recency);
        assert_ne!(memory.relevance_score, original_relevance);

        // Updated scores should match calculation result
        assert_eq!(memory.recency_score, result.recency_score);
        assert_eq!(memory.relevance_score, result.relevance_score);

        // Metadata should contain scoring information
        let metadata = &memory.metadata;
        assert!(metadata.get("last_score_update").is_some());
        assert!(metadata.get("combined_score").is_some());
        assert!(metadata.get("scoring_config").is_some());
    }

    #[test]
    fn test_performance_requirement() {
        let engine = ThreeComponentEngine::default();
        let memory = create_test_memory();
        let context = create_test_context();

        // Calculate score and check timing
        let result = engine.calculate_score(&memory, &context, false).unwrap();

        // Should meet 5ms performance target
        assert!(result.calculation_time_ms <= 5);
    }

    #[test]
    fn test_batch_scoring_consistency() {
        let engine = ThreeComponentEngine::default();
        let memories = vec![create_test_memory(); 5];
        let context = create_test_context();

        // Calculate individual scores
        let mut individual_results = Vec::new();
        for memory in &memories {
            let result = engine.calculate_score(memory, &context, false).unwrap();
            individual_results.push(result);
        }

        // Calculate batch scores
        let batch_results = engine
            .batch_calculate_scores(&memories, &context, false)
            .unwrap();

        // Results should be consistent
        assert_eq!(individual_results.len(), batch_results.len());

        for (individual, batch) in individual_results.iter().zip(batch_results.iter()) {
            assert!((individual.recency_score - batch.recency_score).abs() < 0.001);
            assert!((individual.importance_score - batch.importance_score).abs() < 0.001);
            assert!((individual.relevance_score - batch.relevance_score).abs() < 0.001);
            assert!((individual.combined_score - batch.combined_score).abs() < 0.001);
        }
    }

    #[test]
    fn test_config_validation() {
        // Valid config should pass
        let valid_config = ThreeComponentConfig::default();
        assert!(valid_config.validate().is_ok());

        // Negative weights should fail
        let mut invalid_config = ThreeComponentConfig::default();
        invalid_config.recency_weight = -0.1;
        assert!(invalid_config.validate().is_err());

        // Weights not summing to 1.0 should fail
        invalid_config = ThreeComponentConfig::default();
        invalid_config.recency_weight = 0.5;
        invalid_config.importance_weight = 0.5;
        invalid_config.relevance_weight = 0.5; // Sum = 1.5
        assert!(invalid_config.validate().is_err());

        // Zero decay lambda should fail
        invalid_config = ThreeComponentConfig::default();
        invalid_config.decay_lambda = 0.0;
        assert!(invalid_config.validate().is_err());

        // Negative decay lambda should fail
        invalid_config = ThreeComponentConfig::default();
        invalid_config.decay_lambda = -0.001;
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_environment_config_loading() {
        // This test would normally set environment variables
        // For now, just test that from_env doesn't panic and produces valid config
        let config = ThreeComponentConfig::from_env();
        assert!(config.validate().is_ok());

        // Verify default values when no env vars are set
        let default_config = ThreeComponentConfig::default();
        assert_eq!(config.decay_lambda, default_config.decay_lambda);

        // Weights should be normalized
        let sum = config.recency_weight + config.importance_weight + config.relevance_weight;
        assert!((sum - 1.0).abs() < 0.001);
    }
}
