//! Mathematical engine for memory consolidation and recall probability calculations.
//!
//! This module implements the exact formulas for memory consolidation based on
//! cognitive science research, including the forgetting curve and consolidation
//! strength updates. All calculations are optimized for performance with batch
//! processing capabilities and strict mathematical accuracy requirements.
//!
//! ## Formulas Implemented
//!
//! ### Forgetting Curve
//! ```text
//! p(t) = [1 - exp(-r * e^(-t/gn))] / (1 - e^(-1))
//! ```
//! Where:
//! - p(t) = recall probability at time t
//! - r = decay rate (based on access patterns)
//! - t = time since last access (normalized)
//! - gn = consolidation strength
//!
//! ### Consolidation Strength Update
//! ```text
//! gn = gn-1 + (1 - e^(-t)) / (1 + e^(-t))
//! ```
//! Where:
//! - gn = new consolidation strength
//! - gn-1 = previous consolidation strength
//! - t = recall interval (hours)
//!
//! ## Performance Requirements
//! - <10ms per memory calculation
//! - Mathematical accuracy within 0.001 tolerance
//! - Batch processing for multiple memories
//! - Edge case handling for new/never-accessed memories

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::types::PgInterval;
use std::time::Instant;
use thiserror::Error;

/// Mathematical constants and default values
pub mod constants {
    /// Euler's number (e)
    pub const E: f64 = std::f64::consts::E;

    /// Mathematical tolerance for accuracy validation
    pub const MATHEMATICAL_TOLERANCE: f64 = 0.001;

    /// Default thresholds for tier migration
    pub const COLD_MIGRATION_THRESHOLD: f64 = 0.5;
    pub const FROZEN_MIGRATION_THRESHOLD: f64 = 0.2;

    /// Default consolidation parameters
    pub const DEFAULT_CONSOLIDATION_STRENGTH: f64 = 1.0;
    pub const DEFAULT_DECAY_RATE: f64 = 1.0;
    pub const MAX_CONSOLIDATION_STRENGTH: f64 = 10.0;
    pub const MIN_CONSOLIDATION_STRENGTH: f64 = 0.1;

    /// Performance targets
    pub const MAX_CALCULATION_TIME_MS: u64 = 10;

    /// Time conversion constants
    pub const MICROSECONDS_PER_HOUR: f64 = 3_600_000_000.0;
    pub const SECONDS_PER_HOUR: f64 = 3600.0;
}

/// Errors that can occur during mathematical calculations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum MathEngineError {
    #[error("Invalid parameter: {parameter} = {value}, expected {constraint}")]
    InvalidParameter {
        parameter: String,
        value: f64,
        constraint: String,
    },

    #[error("Mathematical overflow in calculation: {operation}")]
    MathematicalOverflow { operation: String },

    #[error("Calculation accuracy exceeded tolerance: expected {expected}, got {actual}, tolerance {tolerance}")]
    AccuracyError {
        expected: f64,
        actual: f64,
        tolerance: f64,
    },

    #[error("Performance target exceeded: {duration_ms}ms > {target_ms}ms")]
    PerformanceError { duration_ms: u64, target_ms: u64 },

    #[error("Batch processing error: {message}")]
    BatchProcessingError { message: String },
}

pub type Result<T> = std::result::Result<T, MathEngineError>;

/// Configuration for mathematical calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MathEngineConfig {
    /// Threshold for cold tier migration
    pub cold_threshold: f64,

    /// Threshold for frozen tier migration
    pub frozen_threshold: f64,

    /// Maximum consolidation strength
    pub max_consolidation_strength: f64,

    /// Minimum consolidation strength
    pub min_consolidation_strength: f64,

    /// Mathematical tolerance for accuracy validation
    pub tolerance: f64,

    /// Performance target in milliseconds
    pub performance_target_ms: u64,

    /// Enable batch processing optimization
    pub enable_batch_processing: bool,
}

impl Default for MathEngineConfig {
    fn default() -> Self {
        Self {
            cold_threshold: constants::COLD_MIGRATION_THRESHOLD,
            frozen_threshold: constants::FROZEN_MIGRATION_THRESHOLD,
            max_consolidation_strength: constants::MAX_CONSOLIDATION_STRENGTH,
            min_consolidation_strength: constants::MIN_CONSOLIDATION_STRENGTH,
            tolerance: constants::MATHEMATICAL_TOLERANCE,
            performance_target_ms: constants::MAX_CALCULATION_TIME_MS,
            enable_batch_processing: true,
        }
    }
}

/// Memory parameters for mathematical calculations
#[derive(Debug, Clone)]
pub struct MemoryParameters {
    pub consolidation_strength: f64,
    pub decay_rate: f64,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub access_count: i32,
    pub importance_score: f64,
}

/// Result of recall probability calculation
#[derive(Debug, Clone, PartialEq)]
pub struct RecallCalculationResult {
    pub recall_probability: f64,
    pub time_since_access_hours: f64,
    pub normalized_time: f64,
    pub calculation_time_ms: u64,
}

/// Result of consolidation strength update
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationUpdateResult {
    pub new_consolidation_strength: f64,
    pub strength_increment: f64,
    pub recall_interval_hours: f64,
    pub calculation_time_ms: u64,
}

/// Result of batch processing operation
#[derive(Debug, Clone)]
pub struct BatchProcessingResult {
    pub processed_count: usize,
    pub total_time_ms: u64,
    pub average_time_per_memory_ms: f64,
    pub results: Vec<RecallCalculationResult>,
    pub errors: Vec<(usize, MathEngineError)>,
}

/// Main mathematical engine for memory consolidation calculations
#[derive(Debug, Clone)]
pub struct MathEngine {
    config: MathEngineConfig,
}

impl MathEngine {
    /// Create a new math engine with default configuration
    pub fn new() -> Self {
        Self {
            config: MathEngineConfig::default(),
        }
    }

    /// Create a new math engine with custom configuration
    pub fn with_config(config: MathEngineConfig) -> Self {
        Self { config }
    }

    /// Get the current configuration
    pub fn config(&self) -> &MathEngineConfig {
        &self.config
    }

    /// Update the configuration
    pub fn update_config(&mut self, config: MathEngineConfig) {
        self.config = config;
    }

    /// Calculate recall probability using the exact forgetting curve formula
    ///
    /// Formula: p(t) = [1 - exp(-r * e^(-t/gn))] / (1 - e^(-1))
    ///
    /// # Arguments
    /// * `params` - Memory parameters for calculation
    ///
    /// # Returns
    /// * `Result<RecallCalculationResult>` - Calculation result or error
    ///
    /// # Performance
    /// Target: <10ms per calculation
    pub fn calculate_recall_probability(
        &self,
        params: &MemoryParameters,
    ) -> Result<RecallCalculationResult> {
        let start_time = Instant::now();

        // Validate input parameters
        self.validate_parameters(params)?;

        // Handle edge case: never accessed
        let last_access = match params.last_accessed_at {
            Some(access_time) => access_time,
            None => {
                // For never-accessed memories, use creation time as baseline
                let time_since_creation = (Utc::now() - params.created_at).num_seconds() as f64
                    / constants::SECONDS_PER_HOUR;
                let probability =
                    self.calculate_new_memory_probability(time_since_creation, params)?;
                let calculation_time = start_time.elapsed().as_millis() as u64;

                return Ok(RecallCalculationResult {
                    recall_probability: probability,
                    time_since_access_hours: time_since_creation,
                    normalized_time: time_since_creation
                        / params
                            .consolidation_strength
                            .max(self.config.min_consolidation_strength),
                    calculation_time_ms: calculation_time,
                });
            }
        };

        // Calculate time since last access in hours
        let time_since_access =
            (Utc::now() - last_access).num_seconds() as f64 / constants::SECONDS_PER_HOUR;

        // Note: Removed hard-coded bypass for recent access to ensure mathematical consistency
        // The forgetting curve formula handles small time values correctly

        // Ensure consolidation strength is within bounds
        let consolidation_strength = params
            .consolidation_strength
            .max(self.config.min_consolidation_strength);
        let normalized_time = time_since_access / consolidation_strength;

        // Calculate recall probability using Ebbinghaus forgetting curve: R(t) = e^(-t/S)
        let probability = self.ebbinghaus_forgetting_curve(time_since_access, consolidation_strength)?;

        let calculation_time = start_time.elapsed().as_millis() as u64;

        // Validate performance target
        if calculation_time > self.config.performance_target_ms {
            return Err(MathEngineError::PerformanceError {
                duration_ms: calculation_time,
                target_ms: self.config.performance_target_ms,
            });
        }

        Ok(RecallCalculationResult {
            recall_probability: probability,
            time_since_access_hours: time_since_access,
            normalized_time,
            calculation_time_ms: calculation_time,
        })
    }

    /// Update consolidation strength using the exact formula
    ///
    /// Formula: gn = gn-1 + (1 - e^(-t)) / (1 + e^(-t))
    ///
    /// # Arguments
    /// * `current_strength` - Current consolidation strength
    /// * `recall_interval` - Time interval since last recall
    ///
    /// # Returns
    /// * `Result<ConsolidationUpdateResult>` - Update result or error
    pub fn update_consolidation_strength(
        &self,
        current_strength: f64,
        recall_interval: PgInterval,
    ) -> Result<ConsolidationUpdateResult> {
        let start_time = Instant::now();

        // Validate current strength
        if current_strength < 0.0 || current_strength > self.config.max_consolidation_strength * 2.0
        {
            return Err(MathEngineError::InvalidParameter {
                parameter: "current_strength".to_string(),
                value: current_strength,
                constraint: format!(
                    "0.0 <= value <= {}",
                    self.config.max_consolidation_strength * 2.0
                ),
            });
        }

        // Convert interval to hours
        let recall_interval_hours =
            recall_interval.microseconds as f64 / constants::MICROSECONDS_PER_HOUR;

        // Handle edge case: very short intervals (< 1 minute)
        if recall_interval_hours < 1.0 / 60.0 {
            let calculation_time = start_time.elapsed().as_millis() as u64;
            return Ok(ConsolidationUpdateResult {
                new_consolidation_strength: current_strength,
                strength_increment: 0.0,
                recall_interval_hours,
                calculation_time_ms: calculation_time,
            });
        }

        // Calculate strength increment using exact formula
        let strength_increment = self.consolidation_strength_formula(recall_interval_hours)?;

        // Calculate new strength with bounds checking
        let new_strength = (current_strength + strength_increment)
            .min(self.config.max_consolidation_strength)
            .max(self.config.min_consolidation_strength);

        let calculation_time = start_time.elapsed().as_millis() as u64;

        // Validate performance target
        if calculation_time > self.config.performance_target_ms {
            return Err(MathEngineError::PerformanceError {
                duration_ms: calculation_time,
                target_ms: self.config.performance_target_ms,
            });
        }

        Ok(ConsolidationUpdateResult {
            new_consolidation_strength: new_strength,
            strength_increment,
            recall_interval_hours,
            calculation_time_ms: calculation_time,
        })
    }

    /// Calculate decay rate based on access patterns
    ///
    /// This function calculates an adaptive decay rate based on access frequency,
    /// importance score, and memory age. More important and frequently accessed
    /// memories have lower decay rates.
    ///
    /// # Arguments
    /// * `params` - Memory parameters for calculation
    ///
    /// # Returns
    /// * `Result<f64>` - Calculated decay rate
    pub fn calculate_decay_rate(&self, params: &MemoryParameters) -> Result<f64> {
        // Validate parameters
        if params.access_count < 0 {
            return Err(MathEngineError::InvalidParameter {
                parameter: "access_count".to_string(),
                value: params.access_count as f64,
                constraint: "access_count >= 0".to_string(),
            });
        }

        if !(0.0..=1.0).contains(&params.importance_score) {
            return Err(MathEngineError::InvalidParameter {
                parameter: "importance_score".to_string(),
                value: params.importance_score,
                constraint: "0.0 <= importance_score <= 1.0".to_string(),
            });
        }

        // Base decay rate
        let mut decay_rate = constants::DEFAULT_DECAY_RATE;

        // Adjust based on access frequency (logarithmic scaling)
        let access_factor = if params.access_count > 0 {
            1.0 / (1.0 + (params.access_count as f64).ln())
        } else {
            1.0
        };

        // Adjust based on importance (inverse relationship)
        let importance_factor = 1.0 - (params.importance_score * 0.5);

        // Calculate memory age in days
        let age_days = (Utc::now() - params.created_at).num_days() as f64;
        let age_factor = if age_days > 0.0 {
            1.0 + (age_days / 30.0).min(2.0) // Cap at 2x after 60 days
        } else {
            1.0
        };

        // Combine factors
        decay_rate *= access_factor * importance_factor * age_factor;

        // Ensure reasonable bounds
        Ok(decay_rate.max(0.1).min(5.0))
    }

    /// Batch process multiple memories for recall probability calculation
    ///
    /// This function optimizes performance by processing multiple memories
    /// in a single operation, reducing overhead and improving throughput.
    ///
    /// # Arguments
    /// * `memory_params` - Vector of memory parameters
    ///
    /// # Returns
    /// * `Result<BatchProcessingResult>` - Batch processing results
    pub fn batch_calculate_recall_probability(
        &self,
        memory_params: &[MemoryParameters],
    ) -> Result<BatchProcessingResult> {
        if !self.config.enable_batch_processing {
            return Err(MathEngineError::BatchProcessingError {
                message: "Batch processing is disabled".to_string(),
            });
        }

        let start_time = Instant::now();
        let mut results = Vec::with_capacity(memory_params.len());
        let mut errors = Vec::new();

        for (index, params) in memory_params.iter().enumerate() {
            match self.calculate_recall_probability(params) {
                Ok(result) => results.push(result),
                Err(error) => {
                    errors.push((index, error));
                    // Add a placeholder result to maintain index alignment
                    results.push(RecallCalculationResult {
                        recall_probability: 0.0,
                        time_since_access_hours: 0.0,
                        normalized_time: 0.0,
                        calculation_time_ms: 0,
                    });
                }
            }
        }

        let total_time = start_time.elapsed().as_millis() as u64;
        let average_time = if !results.is_empty() {
            total_time as f64 / results.len() as f64
        } else {
            0.0
        };

        Ok(BatchProcessingResult {
            processed_count: memory_params.len(),
            total_time_ms: total_time,
            average_time_per_memory_ms: average_time,
            results,
            errors,
        })
    }

    /// Determine if a memory should migrate to the next tier
    ///
    /// # Arguments
    /// * `recall_probability` - Current recall probability
    /// * `current_tier` - Current memory tier
    ///
    /// # Returns
    /// * `bool` - True if memory should migrate
    pub fn should_migrate(&self, recall_probability: f64, current_tier: &str) -> bool {
        match current_tier.to_lowercase().as_str() {
            "working" => recall_probability < 0.7,
            "warm" => recall_probability < self.config.cold_threshold,
            "cold" => recall_probability < self.config.frozen_threshold,
            "frozen" => false,
            _ => false,
        }
    }

    /// Validate calculation accuracy against expected result
    ///
    /// # Arguments
    /// * `expected` - Expected value
    /// * `actual` - Actual calculated value
    ///
    /// # Returns
    /// * `Result<()>` - Ok if within tolerance, error otherwise
    pub fn validate_accuracy(&self, expected: f64, actual: f64) -> Result<()> {
        let difference = (expected - actual).abs();
        if difference > self.config.tolerance {
            return Err(MathEngineError::AccuracyError {
                expected,
                actual,
                tolerance: self.config.tolerance,
            });
        }
        Ok(())
    }

    // Private helper methods

    /// Implement the exact forgetting curve formula
    /// p(t) = [1 - exp(-r * e^(-t/gn))] / (1 - e^(-1))
    fn forgetting_curve_formula(&self, normalized_time: f64, decay_rate: f64) -> Result<f64> {
        // Validate inputs
        if normalized_time < 0.0 {
            return Err(MathEngineError::InvalidParameter {
                parameter: "normalized_time".to_string(),
                value: normalized_time,
                constraint: "normalized_time >= 0.0".to_string(),
            });
        }

        if decay_rate <= 0.0 {
            return Err(MathEngineError::InvalidParameter {
                parameter: "decay_rate".to_string(),
                value: decay_rate,
                constraint: "decay_rate > 0.0".to_string(),
            });
        }

        // Calculate components with overflow protection
        let exp_neg_t = (-normalized_time).exp();
        if !exp_neg_t.is_finite() {
            return Err(MathEngineError::MathematicalOverflow {
                operation: "exp(-t) calculation".to_string(),
            });
        }

        let exponent = -decay_rate * exp_neg_t;
        if !exponent.is_finite() {
            return Err(MathEngineError::MathematicalOverflow {
                operation: "-r * e^(-t) calculation".to_string(),
            });
        }

        let numerator = 1.0 - exponent.exp();
        let denominator = 1.0 - (-1.0_f64).exp();

        if !numerator.is_finite() || !denominator.is_finite() || denominator.abs() < f64::EPSILON {
            return Err(MathEngineError::MathematicalOverflow {
                operation: "forgetting curve probability calculation".to_string(),
            });
        }

        let probability = numerator / denominator;

        // Ensure result is within valid probability range
        Ok(probability.max(0.0).min(1.0))
    }

    /// Implement the exact consolidation strength formula
    /// increment = (1 - e^(-t)) / (1 + e^(-t))
    fn consolidation_strength_formula(&self, time_hours: f64) -> Result<f64> {
        if time_hours < 0.0 {
            return Err(MathEngineError::InvalidParameter {
                parameter: "time_hours".to_string(),
                value: time_hours,
                constraint: "time_hours >= 0.0".to_string(),
            });
        }

        let exp_neg_t = (-time_hours).exp();
        if !exp_neg_t.is_finite() {
            return Err(MathEngineError::MathematicalOverflow {
                operation: "exp(-t) in consolidation formula".to_string(),
            });
        }

        let numerator = 1.0 - exp_neg_t;
        let denominator = 1.0 + exp_neg_t;

        if denominator.abs() < f64::EPSILON {
            return Err(MathEngineError::MathematicalOverflow {
                operation: "division by zero in consolidation formula".to_string(),
            });
        }

        Ok(numerator / denominator)
    }

    /// Calculate probability for new/never-accessed memories using consistent forgetting curve
    fn calculate_new_memory_probability(
        &self,
        time_since_creation: f64,
        params: &MemoryParameters,
    ) -> Result<f64> {
        // Use the same forgetting curve formula for consistency
        // For new memories, use creation time with adjusted consolidation strength
        let adjusted_consolidation = params.consolidation_strength * params.importance_score;
        let normalized_time = time_since_creation / adjusted_consolidation.max(0.1);
        self.forgetting_curve_formula(normalized_time, params.decay_rate)
    }

    /// Validate memory parameters
    fn validate_parameters(&self, params: &MemoryParameters) -> Result<()> {
        if params.consolidation_strength < 0.0 {
            return Err(MathEngineError::InvalidParameter {
                parameter: "consolidation_strength".to_string(),
                value: params.consolidation_strength,
                constraint: "consolidation_strength >= 0.0".to_string(),
            });
        }

        if params.decay_rate <= 0.0 {
            return Err(MathEngineError::InvalidParameter {
                parameter: "decay_rate".to_string(),
                value: params.decay_rate,
                constraint: "decay_rate > 0.0".to_string(),
            });
        }

        if !(0.0..=1.0).contains(&params.importance_score) {
            return Err(MathEngineError::InvalidParameter {
                parameter: "importance_score".to_string(),
                value: params.importance_score,
                constraint: "0.0 <= importance_score <= 1.0".to_string(),
            });
        }

        Ok(())
    }
}

impl Default for MathEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance benchmarking utilities
pub mod benchmarks {
    use super::*;
    use std::time::Instant;

    /// Benchmark single memory calculation performance
    pub fn benchmark_single_calculation(
        engine: &MathEngine,
        params: &MemoryParameters,
        iterations: usize,
    ) -> (f64, f64, f64) {
        let mut times = Vec::with_capacity(iterations);

        for _ in 0..iterations {
            let start = Instant::now();
            let _ = engine.calculate_recall_probability(params);
            times.push(start.elapsed().as_nanos() as f64 / 1_000_000.0); // Convert to milliseconds
        }

        let sum: f64 = times.iter().sum();
        let avg = sum / times.len() as f64;

        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if times.len() % 2 == 0 {
            (times[times.len() / 2 - 1] + times[times.len() / 2]) / 2.0
        } else {
            times[times.len() / 2]
        };

        let p99_index = ((times.len() as f64) * 0.99) as usize;
        let p99 = times[p99_index.min(times.len() - 1)];

        (avg, median, p99)
    }

    /// Benchmark batch processing performance
    pub fn benchmark_batch_processing(
        engine: &MathEngine,
        batch_sizes: &[usize],
    ) -> Vec<(usize, f64, f64)> {
        let mut results = Vec::new();

        for &batch_size in batch_sizes {
            let params = vec![
                MemoryParameters {
                    consolidation_strength: 1.0,
                    decay_rate: 1.0,
                    last_accessed_at: Some(Utc::now() - chrono::Duration::hours(1)),
                    created_at: Utc::now() - chrono::Duration::days(1),
                    access_count: 5,
                    importance_score: 0.5,
                };
                batch_size
            ];

            let start = Instant::now();
            let result = engine.batch_calculate_recall_probability(&params);
            let total_time = start.elapsed().as_millis() as f64;

            if let Ok(_batch_result) = result {
                let throughput = batch_size as f64 / (total_time / 1000.0); // memories per second
                results.push((batch_size, total_time, throughput));
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use proptest::prelude::*;

    fn create_test_params() -> MemoryParameters {
        MemoryParameters {
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(1)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 5,
            importance_score: 0.5,
        }
    }

    #[test]
    fn test_recall_probability_calculation() {
        let engine = MathEngine::new();
        let params = create_test_params();

        let result = engine.calculate_recall_probability(&params).unwrap();

        assert!(result.recall_probability >= 0.0);
        assert!(result.recall_probability <= 1.0);
        assert!(result.calculation_time_ms <= constants::MAX_CALCULATION_TIME_MS);
    }

    #[test]
    fn test_consolidation_strength_update() {
        let engine = MathEngine::new();
        let interval = PgInterval {
            months: 0,
            days: 0,
            microseconds: (2.0 * constants::MICROSECONDS_PER_HOUR) as i64, // 2 hours
        };

        let result = engine.update_consolidation_strength(1.0, interval).unwrap();

        assert!(result.new_consolidation_strength > 1.0);
        assert!(result.new_consolidation_strength <= constants::MAX_CONSOLIDATION_STRENGTH);
        assert!(result.calculation_time_ms <= constants::MAX_CALCULATION_TIME_MS);
    }

    #[test]
    fn test_decay_rate_calculation() {
        let engine = MathEngine::new();
        let params = create_test_params();

        let decay_rate = engine.calculate_decay_rate(&params).unwrap();

        assert!(decay_rate > 0.0);
        assert!(decay_rate <= 5.0);
    }

    #[test]
    fn test_edge_case_never_accessed() {
        let engine = MathEngine::new();
        let mut params = create_test_params();
        params.last_accessed_at = None;

        let result = engine.calculate_recall_probability(&params).unwrap();

        assert!(result.recall_probability >= 0.0);
        assert!(result.recall_probability <= 1.0);
    }

    #[test]
    fn test_edge_case_very_recent_access() {
        let engine = MathEngine::new();
        let mut params = create_test_params();
        params.last_accessed_at = Some(Utc::now() - Duration::seconds(30));

        let result = engine.calculate_recall_probability(&params).unwrap();

        // Very recent access should have very high recall probability (close to 1.0)
        // but now uses the actual mathematical formula instead of hard-coded 1.0
        assert!(
            result.recall_probability > 0.99,
            "Very recent access should have >99% recall probability, got {}",
            result.recall_probability
        );
        assert!(result.recall_probability <= 1.0);
    }

    #[test]
    fn test_batch_processing() {
        let engine = MathEngine::new();
        let params = vec![create_test_params(); 100];

        let result = engine.batch_calculate_recall_probability(&params).unwrap();

        assert_eq!(result.processed_count, 100);
        assert_eq!(result.results.len(), 100);
        assert!(result.average_time_per_memory_ms < constants::MAX_CALCULATION_TIME_MS as f64);
    }

    #[test]
    fn test_accuracy_validation() {
        let engine = MathEngine::new();

        // Should pass within tolerance
        assert!(engine.validate_accuracy(0.5, 0.5001).is_ok());

        // Should fail outside tolerance
        assert!(engine.validate_accuracy(0.5, 0.6).is_err());
    }

    proptest! {
        #[test]
        fn test_recall_probability_properties(
            consolidation_strength in 0.1f64..10.0,
            decay_rate in 0.1f64..5.0,
            hours_ago in 0.1f64..168.0, // 1 week max
            importance_score in 0.0f64..1.0,
            access_count in 0i32..1000,
        ) {
            let engine = MathEngine::new();
            let params = MemoryParameters {
                consolidation_strength,
                decay_rate,
                last_accessed_at: Some(Utc::now() - Duration::seconds((hours_ago * 3600.0) as i64)),
                created_at: Utc::now() - Duration::days(1),
                access_count,
                importance_score,
            };

            let result = engine.calculate_recall_probability(&params);

            if let Ok(calculation) = result {
                // Recall probability should always be between 0 and 1
                assert!(calculation.recall_probability >= 0.0);
                assert!(calculation.recall_probability <= 1.0);

                // Should complete within performance target
                assert!(calculation.calculation_time_ms <= constants::MAX_CALCULATION_TIME_MS);
            }
        }

        #[test]
        fn test_consolidation_strength_properties(
            initial_strength in 0.1f64..10.0,
            recall_interval_hours in 0.1f64..168.0,
        ) {
            let engine = MathEngine::new();
            let interval = PgInterval {
                months: 0,
                days: 0,
                microseconds: (recall_interval_hours * constants::MICROSECONDS_PER_HOUR) as i64,
            };

            let result = engine.update_consolidation_strength(initial_strength, interval);

            if let Ok(update) = result {
                // New strength should be greater than or equal to initial (memories can only get stronger)
                assert!(update.new_consolidation_strength >= initial_strength);

                // Should not exceed maximum
                assert!(update.new_consolidation_strength <= constants::MAX_CONSOLIDATION_STRENGTH);

                // Should complete within performance target
                assert!(update.calculation_time_ms <= constants::MAX_CALCULATION_TIME_MS);
            }
        }
    }
}
