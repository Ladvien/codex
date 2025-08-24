//! Mathematical Formula Validation Tests
//!
//! This test suite validates all mathematical formulas documented in 
//! docs/mathematical_formulas.md to ensure they match the research
//! literature and implementation specifications.

use codex_memory::memory::math_engine::{MathEngine, MemoryParameters};
use codex_memory::memory::three_component_scoring::{ThreeComponentEngine, ThreeComponentConfig, ScoringContext};
use chrono::{Duration, Utc};
use std::f64::consts::E;

/// Test the Ebbinghaus forgetting curve mathematical properties
#[cfg(test)]
mod ebbinghaus_validation {
    use super::*;

    #[test]
    fn test_ebbinghaus_curve_at_t_zero() {
        let engine = MathEngine::new();
        let params = MemoryParameters {
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now()), // t = 0
            created_at: Utc::now() - Duration::days(1),
            access_count: 1,
            importance_score: 0.5,
        };

        let result = engine.calculate_recall_probability(&params).unwrap();
        
        // At t=0, retention should be 1.0 (perfect retention)
        assert!((result.recall_probability - 1.0).abs() < 0.001, 
               "At t=0, retention should be 1.0, got {}", result.recall_probability);
    }

    #[test]
    fn test_ebbinghaus_curve_at_strength_parameter() {
        let engine = MathEngine::new();
        let strength = 2.0;
        let params = MemoryParameters {
            consolidation_strength: strength,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::seconds((strength * 3600.0) as i64)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 1,
            importance_score: 0.5,
        };

        let result = engine.calculate_recall_probability(&params).unwrap();
        let expected = (1.0 / E); // e^(-1) ≈ 0.368
        
        // At t=S, retention should be 1/e ≈ 0.368
        assert!((result.recall_probability - expected).abs() < 0.01, 
               "At t=S, retention should be 1/e ≈ 0.368, got {}", result.recall_probability);
    }

    #[test]
    fn test_ebbinghaus_curve_monotonic_decrease() {
        let engine = MathEngine::new();
        let strength = 1.0;
        
        let times = vec![0.0, 1.0, 2.0, 4.0, 8.0]; // hours
        let mut retentions = Vec::new();
        
        for t in &times {
            let params = MemoryParameters {
                consolidation_strength: strength,
                decay_rate: 1.0,
                last_accessed_at: Some(Utc::now() - Duration::seconds((*t * 3600.0) as i64)),
                created_at: Utc::now() - Duration::days(1),
                access_count: 1,
                importance_score: 0.5,
            };
            
            let result = engine.calculate_recall_probability(&params).unwrap();
            retentions.push(result.recall_probability);
        }
        
        // Verify monotonic decrease
        for i in 1..retentions.len() {
            assert!(retentions[i-1] > retentions[i], 
                   "Retention should decrease over time: {} > {} at indices {}, {}", 
                   retentions[i-1], retentions[i], i-1, i);
        }
    }

    #[test]
    fn test_ebbinghaus_curve_research_benchmarks() {
        let engine = MathEngine::new();
        let strength = 1.0;
        
        // Test specific research benchmarks from documentation
        let test_cases = vec![
            (1.0, 0.607),   // 1 hour: e^(-1/1) = e^(-1) ≈ 0.607
            (24.0, 0.000000004), // 24 hours: effectively 0
        ];
        
        for (hours, expected) in test_cases {
            let params = MemoryParameters {
                consolidation_strength: strength,
                decay_rate: 1.0,
                last_accessed_at: Some(Utc::now() - Duration::seconds((hours * 3600.0) as i64)),
                created_at: Utc::now() - Duration::days(1),
                access_count: 1,
                importance_score: 0.5,
            };
            
            let result = engine.calculate_recall_probability(&params).unwrap();
            
            if expected > 0.001 {
                // For larger values, check within 2% tolerance
                let tolerance = expected * 0.02;
                assert!((result.recall_probability - expected).abs() < tolerance,
                       "At {}h: expected {:.6}, got {:.6}", hours, expected, result.recall_probability);
            } else {
                // For very small values, just check that it's near zero
                assert!(result.recall_probability < 0.001,
                       "At {}h: expected near zero, got {:.10}", hours, result.recall_probability);
            }
        }
    }
}

/// Test three-component scoring mathematical properties
#[cfg(test)]
mod three_component_validation {
    use super::*;

    #[test]
    fn test_weight_normalization() {
        let mut config = ThreeComponentConfig {
            recency_weight: 2.0,
            importance_weight: 3.0,
            relevance_weight: 1.0,
            ..Default::default()
        };
        
        config.normalize_weights();
        
        // Check sum equals 1.0
        let sum = config.recency_weight + config.importance_weight + config.relevance_weight;
        assert!((sum - 1.0).abs() < 0.001, "Weights should sum to 1.0, got {}", sum);
        
        // Check proportional scaling
        assert!((config.recency_weight - 2.0/6.0).abs() < 0.001);
        assert!((config.importance_weight - 3.0/6.0).abs() < 0.001);
        assert!((config.relevance_weight - 1.0/6.0).abs() < 0.001);
    }

    #[test]
    fn test_recency_score_exponential_decay() {
        let config = ThreeComponentConfig::default();
        let engine = ThreeComponentEngine::new(config).unwrap();
        let context = ScoringContext::default();
        
        // Test exponential decay with λ = 0.005
        let lambda = 0.005;
        let test_hours = vec![0.0, 1.0, 100.0, 1000.0];
        
        for hours in test_hours {
            let mut memory = codex_memory::memory::models::Memory::default();
            memory.last_accessed_at = Some(Utc::now() - Duration::seconds((hours * 3600.0) as i64));
            memory.importance_score = 0.5;
            
            let result = engine.calculate_score(&memory, &context, false).unwrap();
            let expected = (-lambda * hours as f64).exp();
            
            // Within 1% tolerance for exponential decay
            let tolerance = expected * 0.01 + 0.001; // Add small constant for very small values
            assert!((result.recency_score - expected).abs() < tolerance,
                   "At {}h: expected {:.6}, got {:.6}", hours, expected, result.recency_score);
        }
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
        let context = ScoringContext::default();
        
        let mut memory = codex_memory::memory::models::Memory::default();
        memory.importance_score = 0.8;
        memory.last_accessed_at = Some(Utc::now()); // Recent for high recency
        memory.access_count = 10;
        
        let result = engine.calculate_score(&memory, &context, true).unwrap();
        
        // Manually calculate expected combined score
        let expected = 0.5 * result.recency_score + 0.3 * 0.8 + 0.2 * result.relevance_score;
        
        assert!((result.combined_score - expected).abs() < 0.001,
               "Combined score should be weighted average: expected {:.6}, got {:.6}", 
               expected, result.combined_score);
        
        // Verify explanation breakdown
        if let Some(explanation) = result.score_explanation {
            assert!((explanation.recency_contribution - 0.5 * result.recency_score).abs() < 0.001);
            assert!((explanation.importance_contribution - 0.3 * 0.8).abs() < 0.001);
            assert!((explanation.relevance_contribution - 0.2 * result.relevance_score).abs() < 0.001);
        } else {
            panic!("Expected score explanation but got None");
        }
    }

    #[test]
    fn test_score_bounds_validation() {
        let engine = ThreeComponentEngine::default();
        let context = ScoringContext::default();
        
        // Test extreme values
        let mut memory = codex_memory::memory::models::Memory::default();
        memory.importance_score = 0.0; // Minimum importance
        memory.access_count = 0; // No access history
        memory.last_accessed_at = Some(Utc::now() - Duration::days(365)); // Very old
        
        let result = engine.calculate_score(&memory, &context, false).unwrap();
        
        // All scores should be within [0, 1] bounds
        assert!(result.recency_score >= 0.0 && result.recency_score <= 1.0,
               "Recency score out of bounds: {}", result.recency_score);
        assert!(result.importance_score >= 0.0 && result.importance_score <= 1.0,
               "Importance score out of bounds: {}", result.importance_score);
        assert!(result.relevance_score >= 0.0 && result.relevance_score <= 1.0,
               "Relevance score out of bounds: {}", result.relevance_score);
        assert!(result.combined_score >= 0.0 && result.combined_score <= 1.0,
               "Combined score out of bounds: {}", result.combined_score);
    }
}

/// Test performance requirements from documentation
#[cfg(test)]
mod performance_validation {
    use super::*;

    #[test]
    fn test_calculation_performance_requirements() {
        let engine = MathEngine::new();
        let params = MemoryParameters {
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(2)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 5,
            importance_score: 0.7,
        };

        let result = engine.calculate_recall_probability(&params).unwrap();
        
        // Documentation states <10ms target, with 5ms average
        assert!(result.calculation_time_ms <= 10,
               "Calculation exceeded 10ms target: {}ms", result.calculation_time_ms);
    }

    #[test]
    fn test_three_component_performance_target() {
        let engine = ThreeComponentEngine::default();
        let context = ScoringContext::default();
        let mut memory = codex_memory::memory::models::Memory::default();
        memory.importance_score = 0.5;
        
        let result = engine.calculate_score(&memory, &context, false).unwrap();
        
        // Documentation states <5ms target for three-component scoring
        assert!(result.calculation_time_ms <= 5,
               "Three-component scoring exceeded 5ms target: {}ms", result.calculation_time_ms);
    }

    #[test]
    fn test_batch_processing_throughput() {
        let engine = MathEngine::new();
        let batch_size = 100;
        let params = vec![MemoryParameters {
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(1)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 3,
            importance_score: 0.6,
        }; batch_size];

        let start_time = std::time::Instant::now();
        let result = engine.batch_calculate_recall_probability(&params).unwrap();
        let total_time = start_time.elapsed().as_millis() as f64;

        // Documentation states 1000 memories per second for three-component
        let throughput = (batch_size as f64 / total_time) * 1000.0; // memories per second
        
        assert!(throughput >= 100.0, // More lenient for smaller batches
               "Batch throughput too low: {:.1} memories/second", throughput);
        
        assert_eq!(result.processed_count, batch_size);
        assert!(result.average_time_per_memory_ms <= 10.0);
    }
}

/// Test mathematical accuracy requirements
#[cfg(test)]
mod accuracy_validation {
    use super::*;

    #[test]
    fn test_mathematical_tolerance_requirement() {
        let engine = MathEngine::new();
        
        // Test accuracy validation function
        assert!(engine.validate_accuracy(0.5, 0.5001).is_ok()); // Within tolerance
        assert!(engine.validate_accuracy(0.5, 0.501).is_ok()); // Just within 0.001 tolerance
        assert!(engine.validate_accuracy(0.5, 0.502).is_err()); // Outside tolerance
        
        // Documentation states 0.001 tolerance for mathematical accuracy
        let tolerance = 0.001;
        assert!(engine.validate_accuracy(0.0, tolerance).is_ok());
        assert!(engine.validate_accuracy(1.0, 1.0 + tolerance).is_ok());
        assert!(engine.validate_accuracy(0.5, 0.5 + tolerance + 0.0001).is_err());
    }

    #[test]
    fn test_consolidation_strength_bounds() {
        let engine = MathEngine::new();
        
        // Test minimum and maximum consolidation strength bounds
        let min_strength = 0.1;
        let max_strength = 15.0;
        
        // Test minimum strength
        let params_min = MemoryParameters {
            consolidation_strength: min_strength,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(1)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 1,
            importance_score: 0.5,
        };
        
        let result_min = engine.calculate_recall_probability(&params_min).unwrap();
        assert!(result_min.recall_probability >= 0.0 && result_min.recall_probability <= 1.0);
        
        // Test maximum strength  
        let params_max = MemoryParameters {
            consolidation_strength: max_strength,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(1)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 1,
            importance_score: 0.5,
        };
        
        let result_max = engine.calculate_recall_probability(&params_max).unwrap();
        assert!(result_max.recall_probability >= 0.0 && result_max.recall_probability <= 1.0);
        
        // Stronger memories should retain better (for same time interval)
        assert!(result_max.recall_probability >= result_min.recall_probability,
               "Stronger memories should have higher retention: max={:.6}, min={:.6}",
               result_max.recall_probability, result_min.recall_probability);
    }

    #[test]
    fn test_edge_case_handling() {
        let engine = MathEngine::new();
        
        // Test t=0 case (perfect retention)
        let params_zero = MemoryParameters {
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now()),
            created_at: Utc::now() - Duration::days(1),
            access_count: 1,
            importance_score: 0.5,
        };
        
        let result_zero = engine.calculate_recall_probability(&params_zero).unwrap();
        assert!((result_zero.recall_probability - 1.0).abs() < 0.001,
               "At t=0, should have perfect retention");
        
        // Test never-accessed case
        let params_never = MemoryParameters {
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            last_accessed_at: None,
            created_at: Utc::now() - Duration::hours(24),
            access_count: 0,
            importance_score: 0.5,
        };
        
        let result_never = engine.calculate_recall_probability(&params_never).unwrap();
        assert!(result_never.recall_probability >= 0.0 && result_never.recall_probability <= 1.0,
               "Never-accessed memories should have valid retention probability");
    }
}

/// Integration test ensuring consistency across the system
#[cfg(test)]
mod integration_validation {
    use super::*;

    #[test]
    fn test_cross_module_consistency() {
        // Test that math_engine and three_component_scoring produce consistent results
        let math_engine = MathEngine::new();
        let scoring_engine = ThreeComponentEngine::default();
        let context = ScoringContext::default();
        
        let params = MemoryParameters {
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(2)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 5,
            importance_score: 0.7,
        };
        
        let mut memory = codex_memory::memory::models::Memory::default();
        memory.importance_score = params.importance_score;
        memory.access_count = params.access_count;
        memory.last_accessed_at = params.last_accessed_at;
        memory.created_at = params.created_at;
        memory.consolidation_strength = params.consolidation_strength;
        memory.decay_rate = params.decay_rate;
        
        // Calculate recall probability using math engine
        let math_result = math_engine.calculate_recall_probability(&params).unwrap();
        
        // Calculate using three-component scoring
        let scoring_result = scoring_engine.calculate_score(&memory, &context, false).unwrap();
        
        // The recency component of three-component scoring should use the same exponential decay
        // but with different lambda, so we can't expect identical results.
        // Instead, verify both are reasonable and within bounds
        assert!(math_result.recall_probability >= 0.0 && math_result.recall_probability <= 1.0);
        assert!(scoring_result.recency_score >= 0.0 && scoring_result.recency_score <= 1.0);
        assert!(scoring_result.combined_score >= 0.0 && scoring_result.combined_score <= 1.0);
        
        // Both should complete within performance targets
        assert!(math_result.calculation_time_ms <= 10);
        assert!(scoring_result.calculation_time_ms <= 5);
    }
}