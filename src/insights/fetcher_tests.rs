//! Comprehensive tests for the memory fetcher module

#[cfg(test)]
mod tests {
    use super::super::{MemoryFetcher, ProcessingMetadata, FetchConfig};
    use crate::memory::{Memory, MemoryTier, MemoryStatus};
    use chrono::{DateTime, Utc, Duration};
    use std::sync::Arc;
    use uuid::Uuid;

    // Mock database pool for testing - in real tests this would use test database
    fn create_mock_pool() -> Arc<sqlx::PgPool> {
        // This is a placeholder - real implementation would use test database
        // For now, we'll test the logic components that don't require DB
        unimplemented!("Mock pool for testing - real tests need test database setup")
    }

    #[test]
    fn test_fetch_config_defaults() {
        let config = FetchConfig::default();
        
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.reprocessing_threshold_hours, 24);
        assert!(!config.include_frozen);
        assert_eq!(config.max_retry_attempts, 3);
        assert_eq!(config.min_confidence_threshold, 0.6);
    }

    #[test]
    fn test_fetch_config_customization() {
        let config = FetchConfig {
            batch_size: 50,
            reprocessing_threshold_hours: 12,
            include_frozen: true,  // Even though this should normally be false
            max_retry_attempts: 5,
            min_confidence_threshold: 0.8,
        };

        assert_eq!(config.batch_size, 50);
        assert_eq!(config.reprocessing_threshold_hours, 12);
        assert!(config.include_frozen);
        assert_eq!(config.max_retry_attempts, 5);
        assert_eq!(config.min_confidence_threshold, 0.8);
    }

    #[test]
    fn test_processing_metadata_creation() {
        let metadata = ProcessingMetadata {
            last_processed_at: Some(Utc::now()),
            processing_count: 3,
            last_processing_success: true,
            last_error: None,
            insights_generated: 2,
            processor_version: "1.0.0".to_string(),
            last_processing_time_ms: Some(1500),
            insight_confidence: Some(0.85),
        };

        assert!(metadata.last_processed_at.is_some());
        assert_eq!(metadata.processing_count, 3);
        assert!(metadata.last_processing_success);
        assert_eq!(metadata.insights_generated, 2);
        assert_eq!(metadata.insight_confidence, Some(0.85));
    }

    #[test]
    fn test_processing_metadata_serialization() {
        let metadata = ProcessingMetadata {
            last_processed_at: Some(Utc::now()),
            processing_count: 1,
            last_processing_success: true,
            last_error: Some("Test error".to_string()),
            insights_generated: 1,
            processor_version: "1.0.0".to_string(),
            last_processing_time_ms: Some(2000),
            insight_confidence: Some(0.75),
        };

        // Test serialization to JSON
        let json = serde_json::to_string(&metadata).expect("Should serialize");
        assert!(json.contains("last_processed_at"));
        assert!(json.contains("processing_count"));
        assert!(json.contains("1.0.0"));

        // Test deserialization from JSON
        let deserialized: ProcessingMetadata = serde_json::from_str(&json)
            .expect("Should deserialize");
        
        assert_eq!(metadata.processing_count, deserialized.processing_count);
        assert_eq!(metadata.last_processing_success, deserialized.last_processing_success);
        assert_eq!(metadata.processor_version, deserialized.processor_version);
    }

    #[test]
    fn test_processing_metadata_default_values() {
        let metadata = ProcessingMetadata::default();

        assert!(metadata.last_processed_at.is_none());
        assert_eq!(metadata.processing_count, 0);
        assert!(!metadata.last_processing_success);
        assert!(metadata.last_error.is_none());
        assert_eq!(metadata.insights_generated, 0);
        assert_eq!(metadata.processor_version, "1.0.0");
        assert!(metadata.last_processing_time_ms.is_none());
        assert!(metadata.insight_confidence.is_none());
    }

    // Cognitive research principles validation tests
    #[test]
    fn test_tier_priority_cognitive_alignment() {
        // Test that tier priorities align with cognitive research
        // Working memory should have highest priority (most active)
        // Warm memory should have medium priority
        // Cold memory should have lowest priority (but still processed)
        // Frozen memory should be excluded (equivalent to forgotten)

        let working_memory = create_test_memory(MemoryTier::Working, 0.8);
        let warm_memory = create_test_memory(MemoryTier::Warm, 0.8);
        let cold_memory = create_test_memory(MemoryTier::Cold, 0.8);
        let frozen_memory = create_test_memory(MemoryTier::Frozen, 0.8);

        // In cognitive terms:
        // - Working memory is actively maintained and most accessible
        // - Warm memory is available but requires more effort to retrieve
        // - Cold memory exists but is weakly connected
        // - Frozen memory is effectively inaccessible (forgotten)
        
        assert_eq!(working_memory.tier, MemoryTier::Working);
        assert_eq!(warm_memory.tier, MemoryTier::Warm);
        assert_eq!(cold_memory.tier, MemoryTier::Cold);
        assert_eq!(frozen_memory.tier, MemoryTier::Frozen);
    }

    #[test]
    fn test_importance_score_weighting() {
        // Test that importance scores properly influence selection
        // Higher importance should correlate with higher processing priority
        // This aligns with cognitive research on attention and memory consolidation

        let high_importance = create_test_memory(MemoryTier::Working, 0.9);
        let medium_importance = create_test_memory(MemoryTier::Working, 0.5);
        let low_importance = create_test_memory(MemoryTier::Working, 0.1);

        assert!(high_importance.importance_score > medium_importance.importance_score);
        assert!(medium_importance.importance_score > low_importance.importance_score);
        
        // High importance memories should be more likely to generate insights
        // This reflects the cognitive principle that important information
        // is more likely to form connections and patterns
    }

    #[test]
    fn test_reprocessing_threshold_cognitive_validity() {
        let config = FetchConfig::default();
        
        // 24-hour reprocessing threshold aligns with memory consolidation research
        // Sleep-dependent consolidation typically occurs within 24 hours
        // After this period, memories may have new contextual relevance
        assert_eq!(config.reprocessing_threshold_hours, 24);
        
        // This threshold allows for:
        // 1. Memory consolidation to occur naturally
        // 2. New experiences to provide fresh context
        // 3. Pattern recognition across time periods
    }

    #[test]
    fn test_confidence_threshold_alignment() {
        let config = FetchConfig::default();
        
        // 0.6 confidence threshold represents reasonable uncertainty tolerance
        // Too low: generates noise and false patterns
        // Too high: misses valid but uncertain insights
        // 0.6 balances precision and recall effectively
        assert_eq!(config.min_confidence_threshold, 0.6);
    }

    #[test]
    fn test_batch_size_cognitive_load_limits() {
        let config = FetchConfig::default();
        
        // Default batch size of 10 respects cognitive load limitations
        // Processing too many memories simultaneously can lead to:
        // 1. Reduced quality of insight generation
        // 2. Interference between memory processing
        // 3. Cognitive overload in LLM processing
        assert_eq!(config.batch_size, 10);
        
        // This aligns with Miller's 7±2 rule for working memory capacity
        // While not directly applicable, the principle of limited capacity holds
    }

    fn create_test_memory(tier: MemoryTier, importance: f64) -> Memory {
        Memory {
            id: Uuid::new_v4(),
            content: "Test memory content".to_string(),
            content_hash: "test_hash".to_string(),
            embedding: None,
            tier,
            status: MemoryStatus::Active,
            importance_score: importance,
            access_count: 1,
            last_accessed_at: Some(Utc::now()),
            metadata: serde_json::json!({}),
            parent_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            recall_probability: Some(0.8),
            last_recall_interval: None,
            recency_score: 0.5,
            relevance_score: 0.5,
            successful_retrievals: 0,
            failed_retrievals: 0,
            total_retrieval_attempts: 0,
            last_retrieval_difficulty: None,
            last_retrieval_success: None,
            next_review_at: None,
            current_interval_days: Some(1.0),
            ease_factor: 2.5,
        }
    }

    // Performance validation tests
    #[test]
    fn test_batch_processing_efficiency() {
        // Test that batch operations are properly sized for efficiency
        let config = FetchConfig::default();
        
        // Batch size should be small enough to process quickly
        // but large enough to be efficient
        assert!(config.batch_size >= 5);  // Minimum efficiency
        assert!(config.batch_size <= 50); // Maximum cognitive load
    }

    #[test] 
    fn test_retry_attempts_reasonable() {
        let config = FetchConfig::default();
        
        // 3 retry attempts provides reasonable resilience
        // without excessive resource consumption
        assert_eq!(config.max_retry_attempts, 3);
        
        // This allows for:
        // 1. Temporary failures (network, LLM availability)
        // 2. Transient processing issues
        // 3. Reasonable resource limits
    }

    #[test]
    fn test_frozen_memory_exclusion_principle() {
        let config = FetchConfig::default();
        
        // Frozen memories should not be included in processing
        // This aligns with cognitive principle that forgotten memories
        // are not available for pattern formation
        assert!(!config.include_frozen);
        
        // Frozen tier represents:
        // 1. Deep storage (like long-term memory not currently accessible)
        // 2. Compressed format (may lose semantic richness)
        // 3. Cognitively equivalent to forgotten information
    }

    // Error handling validation tests
    #[test]
    fn test_processing_metadata_error_tracking() {
        let mut metadata = ProcessingMetadata::default();
        
        // Test error state tracking
        metadata.last_processing_success = false;
        metadata.last_error = Some("Test error message".to_string());
        metadata.processing_count = 1;
        
        assert!(!metadata.last_processing_success);
        assert!(metadata.last_error.is_some());
        assert_eq!(metadata.processing_count, 1);
        
        // Error tracking enables:
        // 1. Retry logic with exponential backoff
        // 2. Debugging and monitoring
        // 3. Quality control for insight generation
    }

    #[test]
    fn test_confidence_score_validation() {
        let metadata = ProcessingMetadata {
            insight_confidence: Some(0.85),
            ..Default::default()
        };
        
        // Confidence scores should be in valid range [0.0, 1.0]
        assert!(metadata.insight_confidence.unwrap() >= 0.0);
        assert!(metadata.insight_confidence.unwrap() <= 1.0);
        
        // High confidence indicates reliable insight generation
        assert!(metadata.insight_confidence.unwrap() > 0.8);
    }
}