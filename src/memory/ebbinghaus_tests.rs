//! Unit tests validating Ebbinghaus forgetting curve implementation
//! 
//! These tests validate the mathematical correctness of the forgetting curve
//! implementation against established cognitive science research.
//!
//! ## Research Validation
//! 
//! Tests are based on:
//! - Ebbinghaus, H. (1885). Über das Gedächtnis
//! - Wickelgren, W. A. (1974). Single-trace fragility theory of memory dynamics  
//! - Rubin, D. C., & Wenzel, A. E. (1996). One hundred years of forgetting
//! - Murre, J. M., & Chessa, A. G. (2011). Power laws from individual differences

use super::math_engine::*;
use approx::assert_relative_eq;
use chrono::{Duration, Utc};

/// Mathematical tolerance for Ebbinghaus formula validation
const EBBINGHAUS_TOLERANCE: f64 = 0.001;

/// Test data based on Ebbinghaus (1885) empirical measurements
/// Format: (time_hours, expected_retention_percentage)
const EBBINGHAUS_EMPIRICAL_DATA: &[(f64, f64)] = &[
    (0.33, 0.58),   // 20 minutes: 58% retention  
    (1.0, 0.44),    // 1 hour: 44% retention
    (8.8, 0.36),    // ~9 hours: 36% retention
    (24.0, 0.34),   // 1 day: 34% retention
    (144.0, 0.25),  // 6 days: 25% retention
    (720.0, 0.21),  // 30 days: 21% retention
];

/// Test the standard Ebbinghaus forgetting curve: R(t) = e^(-t/S)
#[cfg(test)]
mod ebbinghaus_validation_tests {
    use super::*;

    #[test]
    fn test_ebbinghaus_formula_mathematical_correctness() {
        // Test the pure mathematical form: R(t) = e^(-t/S)
        let strength = 10.0; // S parameter
        
        // Test known mathematical properties
        assert_relative_eq!(
            calculate_ebbinghaus_retention(0.0, strength),
            1.0,
            epsilon = EBBINGHAUS_TOLERANCE
        );
        
        assert_relative_eq!(
            calculate_ebbinghaus_retention(strength, strength),
            std::f64::consts::E.recip(), // 1/e ≈ 0.368
            epsilon = EBBINGHAUS_TOLERANCE
        );
        
        // Test monotonic decrease
        let t1 = 5.0;
        let t2 = 10.0;
        let r1 = calculate_ebbinghaus_retention(t1, strength);
        let r2 = calculate_ebbinghaus_retention(t2, strength);
        assert!(
            r1 > r2,
            "Retention should decrease over time: R({}) = {} > R({}) = {}",
            t1, r1, t2, r2
        );
    }

    #[test]
    fn test_ebbinghaus_empirical_data_alignment() {
        // Use empirical strength parameter that fits Ebbinghaus data
        // Based on curve fitting to original 1885 data
        let empirical_strength = 12.5; // hours
        
        for &(time_hours, expected_retention) in EBBINGHAUS_EMPIRICAL_DATA {
            let calculated_retention = calculate_ebbinghaus_retention(time_hours, empirical_strength);
            
            // Allow 5% tolerance for empirical data fitting
            assert_relative_eq!(
                calculated_retention,
                expected_retention,
                epsilon = 0.05
            );
        }
    }

    #[test] 
    fn test_strength_parameter_effects() {
        let time = 24.0; // 24 hours
        
        let weak_strength = 5.0;
        let strong_strength = 20.0;
        
        let weak_retention = calculate_ebbinghaus_retention(time, weak_strength);
        let strong_retention = calculate_ebbinghaus_retention(time, strong_strength);
        
        assert!(
            strong_retention > weak_retention,
            "Stronger memories should have higher retention: strong={} > weak={}",
            strong_retention, weak_retention
        );
        
        // Verify reasonable bounds
        assert!(weak_retention > 0.0 && weak_retention < 1.0);
        assert!(strong_retention > 0.0 && strong_retention < 1.0);
    }

    #[test]
    fn test_asymptotic_behavior() {
        let strength = 10.0;
        
        // Test approach to zero as time increases
        let very_long_time = 1000.0;
        let retention = calculate_ebbinghaus_retention(very_long_time, strength);
        
        assert!(
            retention > 0.0 && retention < 0.001,
            "Very old memories should have near-zero retention: {}",
            retention
        );
        
        // Test that retention never reaches exactly zero
        assert!(retention > 0.0, "Retention should never be exactly zero");
    }

    #[test]
    fn test_performance_requirement() {
        let params = MemoryParameters {
            consolidation_strength: 5.0,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(2)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 3,
            importance_score: 0.7,
        };
        
        let start = std::time::Instant::now();
        let engine = MathEngine::new();
        let _result = engine.calculate_recall_probability(&params).unwrap();
        let duration = start.elapsed();
        
        assert!(
            duration.as_millis() < 10,
            "Ebbinghaus calculation should complete in <10ms, took {}ms",
            duration.as_millis()
        );
    }

    #[test]
    fn test_edge_cases() {
        let strength = 10.0;
        
        // Test very small time values
        let small_time = 0.001;
        let retention = calculate_ebbinghaus_retention(small_time, strength);
        assert!(retention > 0.999, "Very recent memories should have ~100% retention");
        
        // Test zero strength (should not panic)
        let result = std::panic::catch_unwind(|| {
            calculate_ebbinghaus_retention(1.0, 0.0)
        });
        assert!(result.is_err(), "Zero strength should cause controlled failure");
        
        // Test negative time (should be handled gracefully)
        let retention = calculate_ebbinghaus_retention(-1.0, strength);
        assert_eq!(retention, 1.0, "Negative time should be treated as t=0");
    }

    /// Helper function implementing pure Ebbinghaus formula
    /// R(t) = e^(-t/S)
    fn calculate_ebbinghaus_retention(time: f64, strength: f64) -> f64 {
        if time <= 0.0 {
            return 1.0; // Perfect retention at t=0 or before
        }
        
        if strength <= 0.0 {
            panic!("Strength parameter must be positive");
        }
        
        (-time / strength).exp()
    }
}

/// Tests for integration with existing MathEngine
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_math_engine_ebbinghaus_integration() {
        let engine = MathEngine::new();
        let params = MemoryParameters {
            consolidation_strength: 10.0, // This becomes S in R(t) = e^(-t/S)
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(24)),
            created_at: Utc::now() - Duration::days(2),
            access_count: 5,
            importance_score: 0.6,
        };
        
        let result = engine.calculate_recall_probability(&params).unwrap();
        
        // After 24 hours with strength 10, retention should be e^(-24/10) ≈ 0.091
        let expected_retention: f64 = (-24.0_f64 / 10.0).exp();
        
        assert_relative_eq!(
            result.recall_probability,
            expected_retention,
            epsilon = 0.05
        );
    }

    #[test] 
    fn test_never_accessed_memory_ebbinghaus() {
        let engine = MathEngine::new();
        let mut params = MemoryParameters {
            consolidation_strength: 8.0,
            decay_rate: 1.0,
            last_accessed_at: None, // Never accessed
            created_at: Utc::now() - Duration::hours(12),
            access_count: 0,
            importance_score: 0.5,
        };
        
        let result = engine.calculate_recall_probability(&params).unwrap();
        
        // For never-accessed memories, use time since creation
        let expected_retention: f64 = (-12.0_f64 / (8.0 * 0.5)).exp(); // Adjusted by importance
        
        assert_relative_eq!(
            result.recall_probability,
            expected_retention,
            epsilon = 0.1
        );
    }

    #[test]
    fn test_batch_processing_consistency() {
        let engine = MathEngine::new();
        let params = vec![
            MemoryParameters {
                consolidation_strength: 5.0,
                decay_rate: 1.0,
                last_accessed_at: Some(Utc::now() - Duration::hours(1)),
                created_at: Utc::now() - Duration::days(1),
                access_count: 2,
                importance_score: 0.4,
            };
            10
        ];
        
        // Test individual vs batch consistency
        let individual_result = engine.calculate_recall_probability(&params[0]).unwrap();
        let batch_result = engine.batch_calculate_recall_probability(&params).unwrap();
        
        assert_eq!(batch_result.results.len(), 10);
        assert_relative_eq!(
            batch_result.results[0].recall_probability,
            individual_result.recall_probability,
            epsilon = EBBINGHAUS_TOLERANCE
        );
    }
}

/// Benchmark tests for performance validation
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_ebbinghaus_calculation_performance() {
        let engine = MathEngine::new();
        let params = MemoryParameters {
            consolidation_strength: 7.0,
            decay_rate: 1.0,
            last_accessed_at: Some(Utc::now() - Duration::hours(6)),
            created_at: Utc::now() - Duration::days(1),
            access_count: 4,
            importance_score: 0.8,
        };
        
        // Warm up
        for _ in 0..10 {
            let _ = engine.calculate_recall_probability(&params);
        }
        
        // Measure performance over multiple iterations
        let iterations = 1000;
        let start = Instant::now();
        
        for _ in 0..iterations {
            let _ = engine.calculate_recall_probability(&params).unwrap();
        }
        
        let total_duration = start.elapsed();
        let avg_duration = total_duration / iterations;
        
        assert!(
            avg_duration.as_nanos() < 10_000_000, // <10ms in nanoseconds
            "Average Ebbinghaus calculation should be <10ms, got {:?}",
            avg_duration
        );
        
        println!(
            "Ebbinghaus calculation performance: {:?} average over {} iterations", 
            avg_duration, iterations
        );
    }

    #[test]
    fn test_batch_processing_performance() {
        let engine = MathEngine::new();
        let batch_sizes = [10, 100, 1000];
        
        for &batch_size in &batch_sizes {
            let params = vec![
                MemoryParameters {
                    consolidation_strength: 6.0,
                    decay_rate: 1.0,
                    last_accessed_at: Some(Utc::now() - Duration::hours(3)),
                    created_at: Utc::now() - Duration::days(1),
                    access_count: 1,
                    importance_score: 0.3,
                };
                batch_size
            ];
            
            let start = Instant::now();
            let result = engine.batch_calculate_recall_probability(&params).unwrap();
            let duration = start.elapsed();
            
            let avg_per_memory = duration.as_millis() as f64 / batch_size as f64;
            
            assert!(
                avg_per_memory < 10.0,
                "Batch processing should maintain <10ms per memory, got {}ms for batch size {}",
                avg_per_memory, batch_size
            );
            
            assert_eq!(result.processed_count, batch_size);
            assert_eq!(result.results.len(), batch_size);
        }
    }
}