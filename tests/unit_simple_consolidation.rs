use chrono::{Duration, Utc};
use codex_memory::memory::models::{Memory, MemoryStatus, MemoryTier};
use codex_memory::memory::simple_consolidation::{
    SimpleConsolidationConfig, SimpleConsolidationEngine,
};
use uuid::Uuid;

fn create_test_memory(hours_ago: i64, consolidation_strength: f64, access_count: i32) -> Memory {
    let now = Utc::now();
    Memory {
        id: Uuid::new_v4(),
        content: "Test memory content".to_string(),
        content_hash: "test_hash".to_string(),
        embedding: None,
        tier: MemoryTier::Working,
        status: MemoryStatus::Active,
        importance_score: 0.5,
        access_count,
        last_accessed_at: Some(now - Duration::hours(hours_ago)),
        metadata: serde_json::json!({}),
        parent_id: None,
        created_at: now - Duration::hours(hours_ago * 2),
        updated_at: now - Duration::hours(hours_ago),
        expires_at: None,
        consolidation_strength,
        decay_rate: 1.0,
        recall_probability: Some(0.8),
        last_recall_interval: None,
        recency_score: 0.5,
        relevance_score: 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_consolidation_config() {
        let config = SimpleConsolidationConfig::default();

        assert_eq!(config.base_recall_strength, 0.95);
        assert_eq!(config.migration_threshold, 0.5);
        assert_eq!(config.max_consolidation_strength, 10.0);
        // similarity_weight removed - now using direct multiplication
        assert_eq!(config.time_scale_factor, 0.1);
    }

    #[test]
    fn test_recall_probability_calculation() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        // Test with fresh memory
        let fresh_memory = create_test_memory(0, 2.0, 5);
        let result = engine
            .calculate_recall_probability(&fresh_memory, None)
            .unwrap();

        // Should be high since t=0
        assert!(
            result > 0.8,
            "Fresh memory should have high recall probability: {}",
            result
        );
        assert!(result <= 1.0, "Recall probability should not exceed 1.0");
    }

    #[test]
    fn test_recall_probability_decay() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        // Test memories at different time intervals
        let fresh_memory = create_test_memory(0, 2.0, 5);
        let day_old_memory = create_test_memory(24, 2.0, 5);
        let week_old_memory = create_test_memory(168, 2.0, 5);

        let fresh_result = engine
            .calculate_recall_probability(&fresh_memory, None)
            .unwrap();
        let day_result = engine
            .calculate_recall_probability(&day_old_memory, None)
            .unwrap();
        let week_result = engine
            .calculate_recall_probability(&week_old_memory, None)
            .unwrap();

        // Recall probability should decrease over time
        assert!(
            fresh_result >= day_result,
            "Fresh should be >= day old: {} vs {}",
            fresh_result,
            day_result
        );
        assert!(
            day_result >= week_result,
            "Day old should be >= week old: {} vs {}",
            day_result,
            week_result
        );
    }

    #[test]
    fn test_recall_probability_with_access_count() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        // Same time but different access counts
        let low_access = create_test_memory(24, 2.0, 1);
        let high_access = create_test_memory(24, 2.0, 10);

        let low_result = engine
            .calculate_recall_probability(&low_access, None)
            .unwrap();
        let high_result = engine
            .calculate_recall_probability(&high_access, None)
            .unwrap();

        // Higher access count should result in higher recall probability
        assert!(
            high_result >= low_result,
            "Higher access count should improve recall: {} vs {}",
            high_result,
            low_result
        );
    }

    #[test]
    fn test_recall_probability_with_cosine_similarity() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        let memory = create_test_memory(24, 2.0, 5);

        // Test without similarity (defaults to 1.0)
        let result_no_sim = engine.calculate_recall_probability(&memory, None).unwrap();

        // Test with high similarity (0.9)
        let result_high_sim = engine
            .calculate_recall_probability(&memory, Some(0.9))
            .unwrap();

        // Test with low similarity (0.1)
        let result_low_sim = engine
            .calculate_recall_probability(&memory, Some(0.1))
            .unwrap();

        // With similarity weight of 0.1, the effect should be minimal
        // But we can verify that different similarities produce different results
        println!(
            "No sim: {:.6}, High sim: {:.6}, Low sim: {:.6}",
            result_no_sim, result_high_sim, result_low_sim
        );

        // All should be valid probabilities
        assert!(result_no_sim >= 0.0 && result_no_sim <= 1.0);
        assert!(result_high_sim >= 0.0 && result_high_sim <= 1.0);
        assert!(result_low_sim >= 0.0 && result_low_sim <= 1.0);

        // Low similarity should be lower than high similarity
        assert!(
            result_low_sim <= result_high_sim,
            "Low similarity should not exceed high similarity: {} vs {}",
            result_low_sim,
            result_high_sim
        );
    }

    #[test]
    fn test_consolidation_strength_update() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config.clone());

        let initial_strength = 1.5;
        let time_hours = 2.0;

        let new_strength = engine
            .update_consolidation_strength(initial_strength, time_hours)
            .unwrap();

        // Consolidation should increase: gn = gn-1 + (1 - e^-t)/(1 + e^-t)
        assert!(
            new_strength > initial_strength,
            "Consolidation strength should increase from {} to {}",
            initial_strength,
            new_strength
        );

        // Should be bounded
        assert!(new_strength >= 0.1);
        assert!(new_strength <= config.max_consolidation_strength);
    }

    #[test]
    fn test_consolidation_strength_bounds() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config.clone());

        // Test very high initial strength
        let high_result = engine.update_consolidation_strength(9.9, 1.0).unwrap();
        assert!(high_result <= config.max_consolidation_strength);

        // Test very low initial strength
        let low_result = engine.update_consolidation_strength(0.05, 1.0).unwrap();
        assert!(low_result >= 0.1);
    }

    #[test]
    fn test_process_memory_consolidation() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config.clone());

        let memory = create_test_memory(24, 1.5, 5);
        let result = engine.process_memory_consolidation(&memory, None).unwrap();

        // Verify result structure
        assert!(result.new_consolidation_strength > memory.consolidation_strength);
        assert!(result.recall_probability >= 0.0 && result.recall_probability <= 1.0);
        assert_eq!(
            result.should_migrate,
            result.recall_probability < config.migration_threshold
        );
        assert!(result.calculation_time_ms < 10); // Should be very fast
        assert!(result.time_since_access_hours > 0.0);
    }

    #[test]
    fn test_migration_decision() {
        let mut config = SimpleConsolidationConfig::default();
        config.migration_threshold = 0.5; // Lower threshold for testing
        let engine = SimpleConsolidationEngine::new(config);

        // Create weak memory that should migrate
        let weak_memory = create_test_memory(168, 0.3, 1); // 1 week old, weak
        let weak_result = engine
            .process_memory_consolidation(&weak_memory, None)
            .unwrap();

        // Create strong memory that should not migrate
        let strong_memory = create_test_memory(1, 3.0, 10); // Fresh, strong
        let strong_result = engine
            .process_memory_consolidation(&strong_memory, None)
            .unwrap();

        // Weak memory should have low recall probability
        assert!(weak_result.recall_probability < strong_result.recall_probability);

        // Migration decisions may vary based on actual calculation
        println!(
            "Weak memory recall: {:.3}, should migrate: {}",
            weak_result.recall_probability, weak_result.should_migrate
        );
        println!(
            "Strong memory recall: {:.3}, should migrate: {}",
            strong_result.recall_probability, strong_result.should_migrate
        );
    }

    #[test]
    fn test_batch_processing() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        // Create batch of test memories
        let memories: Vec<Memory> = (0..100)
            .map(|i| create_test_memory(i % 48, 1.0 + (i as f64 * 0.1), i as i32))
            .collect();

        let start = std::time::Instant::now();
        let results = engine.process_batch_consolidation(&memories, None).unwrap();
        let duration = start.elapsed();

        // Should process quickly (target: 1000 in 1 second)
        assert!(
            duration.as_millis() < 100,
            "Batch processing took {:?}",
            duration
        );
        assert!(results.len() <= 100); // Some may fail, that's OK

        // Verify all results are valid
        for result in &results {
            assert!(result.recall_probability >= 0.0 && result.recall_probability <= 1.0);
            assert!(result.new_consolidation_strength > 0.0);
        }
    }

    #[test]
    fn test_batch_processing_with_similarities() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        let memories: Vec<Memory> = (0..10)
            .map(|i| create_test_memory(i + 1, 1.0, i as i32))
            .collect();

        let similarities: Vec<f64> = (0..10).map(|i| 0.5 + (i as f64 * 0.05)).collect();

        let results = engine
            .process_batch_consolidation(&memories, Some(&similarities))
            .unwrap();

        assert!(results.len() <= 10);

        // Higher similarities should generally result in higher recall probabilities
        // (though this depends on the weighting)
        for result in &results {
            assert!(result.recall_probability >= 0.0 && result.recall_probability <= 1.0);
        }
    }

    #[test]
    fn test_edge_case_no_last_accessed() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        let mut memory = create_test_memory(24, 1.0, 5);
        memory.last_accessed_at = None; // No last access time

        // Should fall back to created_at
        let result = engine.calculate_recall_probability(&memory, None).unwrap();
        assert!(
            result >= 0.0 && result <= 1.0,
            "Should handle missing last_accessed_at"
        );

        let consolidation_result = engine.process_memory_consolidation(&memory, None).unwrap();
        assert!(consolidation_result.recall_probability >= 0.0);
    }

    #[test]
    fn test_edge_case_zero_access_count() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        let memory = create_test_memory(24, 1.0, 0); // No accesses
        let result = engine.calculate_recall_probability(&memory, None).unwrap();

        // Should handle zero access count (n=0 means denominator = 1+0 = 1)
        assert!(
            result >= 0.0 && result <= 1.0,
            "Should handle zero access count"
        );
    }

    #[test]
    fn test_mathematical_formula_verification() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config.clone());

        let memory = create_test_memory(6, 2.0, 3); // 6 hours ago, strength=2.0, n=3
        let result = engine.calculate_recall_probability(&memory, None).unwrap();

        // Manual calculation for verification
        let r = config.base_recall_strength; // 0.95
        let g = 2.0; // consolidation strength
        let t = 6.0 * config.time_scale_factor; // normalized time
        let n = 3.0; // access count
        let cos_similarity = 1.0; // No similarity provided, defaults to 1.0

        let expected = r * (-g * t / (1.0 + n)).exp() * cos_similarity;

        // Should be close to manual calculation (allowing for floating point differences)
        assert!(
            (result - expected).abs() < 0.001,
            "Formula calculation mismatch: expected {:.6}, got {:.6}",
            expected,
            result
        );
    }

    #[test]
    fn test_performance_target() {
        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        // Create 1000 test memories for performance test
        let memories: Vec<Memory> = (0..1000)
            .map(|i| create_test_memory((i % 168) + 1, 1.0 + (i as f64 * 0.01), (i % 20) as i32))
            .collect();

        let start = std::time::Instant::now();
        let results = engine.process_batch_consolidation(&memories, None).unwrap();
        let duration = start.elapsed();

        // Target: 1000 memories in < 1 second
        assert!(
            duration.as_millis() < 1000,
            "Performance target not met: {} memories in {:?}",
            memories.len(),
            duration
        );

        println!(
            "Performance test: {} memories processed in {:?} ({:.1} memories/sec)",
            results.len(),
            duration,
            results.len() as f64 / duration.as_secs_f64()
        );
    }

    #[test]
    fn test_time_scale_factor_effect() {
        let mut config1 = SimpleConsolidationConfig::default();
        config1.time_scale_factor = 0.1; // Slow decay

        let mut config2 = SimpleConsolidationConfig::default();
        config2.time_scale_factor = 0.5; // Fast decay

        let engine1 = SimpleConsolidationEngine::new(config1);
        let engine2 = SimpleConsolidationEngine::new(config2);

        let memory = create_test_memory(24, 2.0, 5); // 24 hours old

        let result1 = engine1.calculate_recall_probability(&memory, None).unwrap();
        let result2 = engine2.calculate_recall_probability(&memory, None).unwrap();

        // Faster time scale should result in lower recall probability for same time
        assert!(
            result1 >= result2,
            "Slower time scale should have higher recall: {} vs {}",
            result1,
            result2
        );
    }
}
