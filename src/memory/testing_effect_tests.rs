//! Comprehensive Testing Effect Validation Tests
//!
//! These tests validate the testing effect implementation against established
//! cognitive science research, particularly the seminal work of Roediger & Karpicke (2008).

#[cfg(test)]
mod tests {
    use super::super::models::Memory;
    use super::super::repository::MemoryRepository;
    use super::super::testing_effect::{
        DifficultyThresholds, RetrievalAttempt, RetrievalType, TestingEffectConfig,
        TestingEffectEngine,
    };
    use chrono::{Duration, Utc};
    use std::sync::Arc;
    use uuid::Uuid;

    /// Create a mock repository for testing (in production this would use a test database)
    fn create_mock_repository() -> Arc<MemoryRepository> {
        // This is a placeholder - in actual tests we'd use a test database connection
        // For now, we'll create a basic structure to test algorithms
        unimplemented!("Mock repository creation for isolated algorithm testing")
    }

    fn create_test_config() -> TestingEffectConfig {
        TestingEffectConfig::default()
    }

    fn create_test_memory_with_history() -> Memory {
        let mut memory = Memory::default();
        memory.id = Uuid::new_v4();
        memory.consolidation_strength = 3.0;
        memory.successful_retrievals = 5;
        memory.failed_retrievals = 2;
        memory.total_retrieval_attempts = 7;
        memory.ease_factor = 2.3;
        memory.current_interval_days = Some(14.0);
        memory.next_review_at = Some(Utc::now() - Duration::days(1)); // Due for review
        memory.last_accessed_at = Some(Utc::now() - Duration::days(14));
        memory
    }

    /// Test consolidation boost calculation based on Roediger & Karpicke (2008) research
    #[test]
    fn test_roediger_karpicke_consolidation_boost() {
        let config = create_test_config();
        // Note: This test validates the algorithm without requiring database connection

        // Verify research-backed multipliers
        assert_eq!(config.successful_retrieval_multiplier, 1.5); // Roediger & Karpicke finding
        assert_eq!(config.failed_retrieval_multiplier, 0.8); // Partial benefit

        // Test difficulty thresholds match cognitive research
        assert_eq!(config.difficulty_thresholds.moderate_ms, 3000); // 3-second optimal difficulty
        assert_eq!(config.difficulty_thresholds.very_easy_ms, 500); // Immediate recognition
    }

    /// Test Pimsleur spaced intervals are correctly implemented
    #[test]
    fn test_pimsleur_spaced_intervals() {
        let config = create_test_config();

        // Validate research-backed Pimsleur intervals
        let expected_intervals = vec![1.0, 7.0, 16.0, 35.0];
        assert_eq!(config.pimsleur_intervals, expected_intervals);

        // Test interval bounds are reasonable
        assert_eq!(config.min_interval_days, 1.0);
        assert_eq!(config.max_interval_days, 365.0);
    }

    /// Test SuperMemo2 ease factor implementation
    #[test]
    fn test_supermemo2_ease_factors() {
        let config = create_test_config();

        // Validate SuperMemo2 default parameters
        assert_eq!(config.default_ease_factor, 2.5);
        assert_eq!(config.min_ease_factor, 1.3);
        assert_eq!(config.max_ease_factor, 3.0);
    }

    /// Test difficulty scoring algorithm matches cognitive research
    #[test]
    fn test_difficulty_scoring_research_compliance() {
        let config = create_test_config();
        let repository = create_mock_repository();
        let engine = TestingEffectEngine::new(config.clone(), repository);

        // Test immediate recognition (very easy)
        let difficulty_very_easy = engine.calculate_difficulty_score(300, 0.95);
        assert!(
            difficulty_very_easy < 0.1,
            "Immediate recognition should be very easy"
        );

        // Test optimal difficulty range (1.5-3 seconds)
        let difficulty_optimal = engine.calculate_difficulty_score(2500, 0.8);
        assert!(
            difficulty_optimal >= 0.4 && difficulty_optimal <= 0.6,
            "2.5 second retrieval should be optimal difficulty"
        );

        // Test very hard retrieval (approaching failure)
        let difficulty_very_hard = engine.calculate_difficulty_score(9000, 0.3);
        assert!(
            difficulty_very_hard > 0.8,
            "9 second retrieval should be very hard"
        );

        // Test confidence adjustment (lower confidence = higher difficulty)
        let difficulty_low_confidence = engine.calculate_difficulty_score(2000, 0.2);
        let difficulty_high_confidence = engine.calculate_difficulty_score(2000, 0.9);
        assert!(
            difficulty_low_confidence > difficulty_high_confidence,
            "Lower confidence should increase difficulty score"
        );
    }

    /// Test consolidation boost calculation follows research bounds
    #[test]
    fn test_consolidation_boost_research_bounds() {
        let config = create_test_config();
        let repository = create_mock_repository();
        let engine = TestingEffectEngine::new(config.clone(), repository);

        // Test successful retrieval with optimal difficulty
        let boost_success_optimal =
            engine.calculate_consolidation_boost(true, 0.5, &RetrievalType::CuedRecall);
        assert!(
            boost_success_optimal >= 1.5 && boost_success_optimal <= 2.0,
            "Successful optimal retrieval should give 1.5-2.0x boost (research range)"
        );

        // Test failed retrieval gives partial benefit
        let boost_failure =
            engine.calculate_consolidation_boost(false, 0.5, &RetrievalType::CuedRecall);
        assert!(
            boost_failure < 1.0 && boost_failure >= 0.5,
            "Failed retrieval should give 0.5-1.0x boost (partial benefit)"
        );

        // Test very easy retrieval gives reduced benefit
        let boost_too_easy =
            engine.calculate_consolidation_boost(true, 0.1, &RetrievalType::CuedRecall);
        assert!(
            boost_too_easy < boost_success_optimal,
            "Very easy retrieval should give less benefit than optimal difficulty"
        );

        // Test very hard retrieval gives reduced benefit
        let boost_too_hard =
            engine.calculate_consolidation_boost(true, 0.9, &RetrievalType::CuedRecall);
        assert!(
            boost_too_hard < boost_success_optimal,
            "Very hard retrieval should give less benefit than optimal difficulty"
        );
    }

    /// Test retrieval type effects match research findings
    #[test]
    fn test_retrieval_type_effects() {
        let config = create_test_config();
        let repository = create_mock_repository();
        let engine = TestingEffectEngine::new(config.clone(), repository);

        let difficulty = 0.5; // Optimal difficulty
        let success = true;

        // Test free recall gives strongest effect (research finding)
        let boost_free_recall =
            engine.calculate_consolidation_boost(success, difficulty, &RetrievalType::FreeRecall);

        // Test cued recall gives strong effect
        let boost_cued_recall =
            engine.calculate_consolidation_boost(success, difficulty, &RetrievalType::CuedRecall);

        // Test recognition gives weaker effect
        let boost_recognition =
            engine.calculate_consolidation_boost(success, difficulty, &RetrievalType::Recognition);

        // Test similarity search gives minimal effect
        let boost_similarity = engine.calculate_consolidation_boost(
            success,
            difficulty,
            &RetrievalType::SimilaritySearch,
        );

        // Validate hierarchy: Free Recall > Cued Recall > Recognition > Similarity
        assert!(boost_free_recall > boost_cued_recall);
        assert!(boost_cued_recall > boost_recognition);
        assert!(boost_recognition > boost_similarity);
    }

    /// Test spaced repetition interval calculation
    #[test]
    fn test_spaced_repetition_intervals() {
        let config = create_test_config();
        let repository = create_mock_repository();
        let engine = TestingEffectEngine::new(config.clone(), repository);

        let memory = create_test_memory_with_history();

        // Test successful retrieval expands interval
        let next_interval_success = engine.calculate_next_interval(&memory, true, 0.5);
        assert!(
            next_interval_success > memory.current_interval_days.unwrap(),
            "Successful retrieval should expand interval"
        );

        // Test failed retrieval resets interval
        let next_interval_failure = engine.calculate_next_interval(&memory, false, 0.5);
        assert_eq!(
            next_interval_failure, config.min_interval_days,
            "Failed retrieval should reset to minimum interval"
        );

        // Test very easy retrieval expands interval more
        let next_interval_easy = engine.calculate_next_interval(&memory, true, 0.2);
        let next_interval_optimal = engine.calculate_next_interval(&memory, true, 0.5);
        assert!(
            next_interval_easy > next_interval_optimal,
            "Easier retrieval should expand interval more (Pimsleur principle)"
        );

        // Test interval bounds are respected
        assert!(next_interval_success >= config.min_interval_days);
        assert!(next_interval_success <= config.max_interval_days);
    }

    /// Test ease factor adjustments follow SuperMemo2 principles
    #[test]
    fn test_ease_factor_adjustments() {
        let config = create_test_config();
        let repository = create_mock_repository();
        let engine = TestingEffectEngine::new(config.clone(), repository);

        // Test successful retrieval increases ease factor
        let ease_change_success = engine.calculate_ease_factor_change(true, 0.5);
        assert!(
            ease_change_success > 0.0,
            "Successful retrieval should increase ease factor"
        );

        // Test failed retrieval decreases ease factor
        let ease_change_failure = engine.calculate_ease_factor_change(false, 0.5);
        assert!(
            ease_change_failure < 0.0,
            "Failed retrieval should decrease ease factor"
        );

        // Test optimal difficulty gives bonus
        let ease_change_optimal = engine.calculate_ease_factor_change(true, 0.5);
        let ease_change_suboptimal = engine.calculate_ease_factor_change(true, 0.8);
        assert!(
            ease_change_optimal > ease_change_suboptimal,
            "Optimal difficulty should give larger ease factor increase"
        );
    }

    /// Test memory model helper methods for testing effect
    #[test]
    fn test_memory_testing_effect_methods() {
        let memory = create_test_memory_with_history();

        // Test success rate calculation
        let success_rate = memory.testing_effect_success_rate();
        assert_eq!(success_rate, 5.0 / 7.0); // 5 successful out of 7 attempts

        // Test retrieval confidence combines success rate and attempts
        let confidence = memory.retrieval_confidence();
        assert!(confidence > 0.5); // Should be above neutral with good history
        assert!(confidence <= 1.0); // Should not exceed maximum

        // Test spaced interval calculation
        let next_interval_success = memory.calculate_next_spaced_interval(true, 0.5);
        assert!(next_interval_success > memory.current_interval_days.unwrap());

        let next_interval_failure = memory.calculate_next_spaced_interval(false, 0.5);
        assert_eq!(next_interval_failure, 1.0); // Should reset to 1 day

        // Test review due logic
        assert!(memory.is_due_for_review()); // Set to be due in test data

        let mut memory_not_due = memory.clone();
        memory_not_due.next_review_at = Some(Utc::now() + Duration::days(1));
        assert!(!memory_not_due.is_due_for_review());
    }

    /// Test research compliance validation
    #[test]
    fn test_research_compliance_validation() {
        let config = create_test_config();
        let repository = create_mock_repository();
        let engine = TestingEffectEngine::new(config.clone(), repository);

        // Test compliant parameters
        let compliance_good = engine.validate_research_compliance(1.5, 14.0, 0.5);
        assert!(compliance_good.follows_roediger_karpicke);
        assert!(compliance_good.implements_desirable_difficulty);
        assert!(compliance_good.uses_pimsleur_spacing);
        assert!(compliance_good.consolidation_boost_within_research_bounds);
        assert!(compliance_good.interval_progression_optimal);

        // Test non-compliant parameters
        let compliance_bad = engine.validate_research_compliance(3.0, 400.0, 1.5);
        assert!(!compliance_bad.follows_roediger_karpicke); // Boost too high
        assert!(!compliance_bad.implements_desirable_difficulty); // Difficulty out of range
        assert!(!compliance_bad.uses_pimsleur_spacing); // Interval too long
        assert!(!compliance_bad.consolidation_boost_within_research_bounds); // Boost excessive
    }

    /// Test performance bounds and edge cases
    #[test]
    fn test_performance_bounds_and_edge_cases() {
        let config = create_test_config();
        let repository = create_mock_repository();
        let engine = TestingEffectEngine::new(config.clone(), repository);

        // Test extreme latency values
        let difficulty_zero = engine.calculate_difficulty_score(0, 1.0);
        assert!(difficulty_zero >= 0.0);

        let difficulty_extreme = engine.calculate_difficulty_score(u64::MAX, 0.0);
        assert!(difficulty_extreme <= 1.0);

        // Test extreme confidence values
        let difficulty_no_confidence = engine.calculate_difficulty_score(2000, 0.0);
        let difficulty_full_confidence = engine.calculate_difficulty_score(2000, 1.0);
        assert!(difficulty_no_confidence > difficulty_full_confidence);

        // Test consolidation boost bounds
        let boost_extreme_success =
            engine.calculate_consolidation_boost(true, 1.0, &RetrievalType::FreeRecall);
        assert!(boost_extreme_success >= 0.5 && boost_extreme_success <= 2.0);

        let boost_extreme_failure =
            engine.calculate_consolidation_boost(false, 0.0, &RetrievalType::SimilaritySearch);
        assert!(boost_extreme_failure >= 0.5 && boost_extreme_failure <= 2.0);
    }

    /// Test integration with existing memory system
    #[test]
    fn test_memory_system_integration() {
        // Test that new fields are properly initialized in Memory::default()
        let memory = Memory::default();

        assert_eq!(memory.successful_retrievals, 0);
        assert_eq!(memory.failed_retrievals, 0);
        assert_eq!(memory.total_retrieval_attempts, 0);
        assert_eq!(memory.last_retrieval_difficulty, None);
        assert_eq!(memory.last_retrieval_success, None);
        assert_eq!(memory.next_review_at, None);
        assert_eq!(memory.current_interval_days, Some(1.0)); // Pimsleur starting interval
        assert_eq!(memory.ease_factor, 2.5); // SuperMemo2 default

        // Test that testing effect methods work with default memory
        assert_eq!(memory.testing_effect_success_rate(), 0.5); // Neutral starting point
        assert_eq!(memory.retrieval_confidence(), 0.15); // Low confidence with no attempts
        assert!(memory.is_due_for_review()); // Should be due with no review set
    }

    /// Integration test for cognitive consolidation testing effect
    #[test]
    fn test_cognitive_consolidation_integration() {
        use super::super::cognitive_consolidation::{
            CognitiveConsolidationConfig, CognitiveConsolidationEngine,
        };

        let config = CognitiveConsolidationConfig::default();
        let engine = CognitiveConsolidationEngine::new(config);

        // Verify that the testing effect calculation in cognitive consolidation
        // uses research-backed parameters similar to dedicated testing effect
        let context = super::super::cognitive_consolidation::RetrievalContext {
            query_embedding: None,
            environmental_factors: std::collections::HashMap::new(),
            retrieval_latency_ms: 2500, // Optimal difficulty
            confidence_score: 0.8,      // High confidence (successful)
            related_memories: Vec::new(),
        };

        let testing_effect = engine.calculate_testing_effect(&context).unwrap();

        // Should be within research bounds for successful optimal retrieval
        assert!(testing_effect >= 1.0 && testing_effect <= 2.5);
    }
}
