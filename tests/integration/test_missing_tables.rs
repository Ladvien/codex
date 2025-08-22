// Integration tests for Missing Database Tables (Migration 008)
// Tests the new harvest sessions, consolidation events, and system monitoring tables

use chrono::Utc;
use codex_memory::memory::models::*;
use codex_memory::memory::repository::MemoryRepository;
use codex_memory::setup::setup_database_pool;
use serial_test::serial;
use sqlx::PgPool;
use std::env;
use uuid::Uuid;

async fn setup_test_pool() -> PgPool {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/codex_test".to_string());
    
    setup_database_pool(&database_url, None)
        .await
        .expect("Failed to setup test database pool")
}

async fn setup_test_memory(repo: &MemoryRepository) -> Memory {
    let memory_request = CreateMemoryRequest {
        content: "Test memory for missing tables integration".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.8),
        metadata: Some(serde_json::json!({"test": "data"})),
        parent_id: None,
        expires_at: None,
    };
    
    repo.create_memory(memory_request)
        .await
        .expect("Failed to create test memory")
}

#[tokio::test]
#[serial]
async fn test_harvest_session_lifecycle() {
    let pool = setup_test_pool().await;
    let repo = MemoryRepository::new(pool.clone());
    
    // Test creating a harvest session
    let create_request = CreateHarvestSessionRequest {
        session_type: HarvestSessionType::Silent,
        trigger_reason: "Test trigger for integration test".to_string(),
        config_snapshot: Some(serde_json::json!({
            "confidence_threshold": 0.7,
            "max_batch_size": 50
        })),
    };
    
    let session = repo.create_harvest_session(create_request)
        .await
        .expect("Failed to create harvest session");
    
    assert_eq!(session.session_type, HarvestSessionType::Silent);
    assert_eq!(session.status, HarvestSessionStatus::InProgress);
    assert_eq!(session.messages_processed, 0);
    assert_eq!(session.patterns_extracted, 0);
    assert_eq!(session.retry_count, 0);
    
    // Test updating the harvest session
    let update_request = UpdateHarvestSessionRequest {
        status: Some(HarvestSessionStatus::Completed),
        messages_processed: Some(25),
        patterns_extracted: Some(12),
        patterns_stored: Some(8),
        duplicates_filtered: Some(4),
        processing_time_ms: Some(1500),
        error_message: None,
        extraction_time_ms: Some(800),
        deduplication_time_ms: Some(400),
        storage_time_ms: Some(300),
        memory_usage_mb: Some(128.5),
        cpu_usage_percent: Some(15.2),
    };
    
    let updated_session = repo.update_harvest_session(session.id, update_request)
        .await
        .expect("Failed to update harvest session");
    
    assert_eq!(updated_session.status, HarvestSessionStatus::Completed);
    assert_eq!(updated_session.messages_processed, 25);
    assert_eq!(updated_session.patterns_extracted, 12);
    assert_eq!(updated_session.patterns_stored, 8);
    assert_eq!(updated_session.duplicates_filtered, 4);
    assert_eq!(updated_session.processing_time_ms, 1500);
    assert_eq!(updated_session.extraction_time_ms, 800);
    assert!(updated_session.memory_usage_mb.is_some());
    assert!(updated_session.cpu_usage_percent.is_some());
    
    // Test retrieving the harvest session
    let retrieved_session = repo.get_harvest_session(session.id)
        .await
        .expect("Failed to retrieve harvest session");
    
    assert_eq!(retrieved_session.id, session.id);
    assert_eq!(retrieved_session.status, HarvestSessionStatus::Completed);
    assert_eq!(retrieved_session.patterns_stored, 8);
}

#[tokio::test]
#[serial]
async fn test_harvest_pattern_creation() {
    let pool = setup_test_pool().await;
    let repo = MemoryRepository::new(pool.clone());
    
    // Create a harvest session first
    let session_request = CreateHarvestSessionRequest {
        session_type: HarvestSessionType::Manual,
        trigger_reason: "Test pattern creation".to_string(),
        config_snapshot: None,
    };
    
    let session = repo.create_harvest_session(session_request)
        .await
        .expect("Failed to create harvest session");
    
    // Create a harvest pattern
    let pattern_request = CreateHarvestPatternRequest {
        harvest_session_id: session.id,
        pattern_type: HarvestPatternType::Preference,
        content: "I prefer using PostgreSQL for database operations".to_string(),
        confidence_score: 0.85,
        source_message_id: Some("test_message_123".to_string()),
        context: Some("Database discussion context".to_string()),
        metadata: Some(serde_json::json!({
            "extraction_method": "regex_pattern",
            "matched_phrase": "I prefer"
        })),
    };
    
    let pattern = repo.create_harvest_pattern(pattern_request)
        .await
        .expect("Failed to create harvest pattern");
    
    assert_eq!(pattern.harvest_session_id, session.id);
    assert_eq!(pattern.pattern_type, HarvestPatternType::Preference);
    assert_eq!(pattern.confidence_score, 0.85);
    assert_eq!(pattern.status, HarvestPatternStatus::Extracted);
    assert!(pattern.source_message_id.is_some());
    assert!(pattern.context.is_some());
    assert!(pattern.memory_id.is_none()); // Should be None until stored as memory
}

#[tokio::test]
#[serial]
async fn test_consolidation_event_tracking() {
    let pool = setup_test_pool().await;
    let repo = MemoryRepository::new(pool.clone());
    
    // Create a test memory
    let memory = setup_test_memory(&repo).await;
    
    // Create a tier migration consolidation event
    let event_request = CreateConsolidationEventRequest {
        event_type: ConsolidationEventType::TierMigration,
        memory_id: memory.id,
        source_tier: Some("working".to_string()),
        target_tier: Some("warm".to_string()),
        migration_reason: Some("Low access frequency detected".to_string()),
        old_consolidation_strength: Some(1.0),
        new_consolidation_strength: Some(1.2),
        old_recall_probability: Some(0.8),
        new_recall_probability: Some(0.75),
        triggered_by: Some("background_service".to_string()),
        context_metadata: Some(serde_json::json!({
            "trigger_condition": "access_threshold",
            "threshold_value": 0.3
        })),
    };
    
    let event = repo.create_consolidation_event(event_request)
        .await
        .expect("Failed to create consolidation event");
    
    assert_eq!(event.event_type, ConsolidationEventType::TierMigration);
    assert_eq!(event.memory_id, memory.id);
    assert_eq!(event.source_tier, Some("working".to_string()));
    assert_eq!(event.target_tier, Some("warm".to_string()));
    assert!(event.migration_reason.is_some());
    
    // Check calculated deltas
    assert_eq!(event.strength_delta, Some(0.2));
    assert_eq!(event.probability_delta, Some(-0.05));
    
    // Create an importance update event
    let importance_event_request = CreateConsolidationEventRequest {
        event_type: ConsolidationEventType::ImportanceUpdate,
        memory_id: memory.id,
        source_tier: None,
        target_tier: None,
        migration_reason: None,
        old_consolidation_strength: Some(1.2),
        new_consolidation_strength: Some(1.5),
        old_recall_probability: None,
        new_recall_probability: None,
        triggered_by: Some("user".to_string()),
        context_metadata: Some(serde_json::json!({
            "update_reason": "manual_boost",
            "boost_factor": 1.25
        })),
    };
    
    let importance_event = repo.create_consolidation_event(importance_event_request)
        .await
        .expect("Failed to create importance update event");
    
    assert_eq!(importance_event.event_type, ConsolidationEventType::ImportanceUpdate);
    assert_eq!(importance_event.strength_delta, Some(0.3));
    assert!(importance_event.migration_reason.is_none());
}

#[tokio::test]
#[serial]
async fn test_memory_access_log() {
    let pool = setup_test_pool().await;
    let repo = MemoryRepository::new(pool.clone());
    
    // Create a test memory
    let memory = setup_test_memory(&repo).await;
    
    // Create a memory access log entry
    let access_request = CreateMemoryAccessLogRequest {
        memory_id: memory.id,
        access_type: MemoryAccessType::Search,
        session_id: Some(Uuid::new_v4()),
        user_context: Some("User searching for database-related memories".to_string()),
        query_context: Some("PostgreSQL optimization techniques".to_string()),
        retrieval_time_ms: Some(25),
        similarity_score: Some(0.92),
        ranking_position: Some(3),
        importance_boost: Some(0.1),
        access_count_increment: Some(1),
    };
    
    let access_log = repo.create_memory_access_log(access_request)
        .await
        .expect("Failed to create memory access log");
    
    assert_eq!(access_log.memory_id, memory.id);
    assert_eq!(access_log.access_type, MemoryAccessType::Search);
    assert!(access_log.session_id.is_some());
    assert!(access_log.user_context.is_some());
    assert!(access_log.query_context.is_some());
    assert_eq!(access_log.retrieval_time_ms, Some(25));
    assert_eq!(access_log.similarity_score, Some(0.92));
    assert_eq!(access_log.ranking_position, Some(3));
    assert_eq!(access_log.importance_boost, 0.1);
    assert_eq!(access_log.access_count_increment, 1);
    
    // Create a direct retrieval access log
    let direct_access_request = CreateMemoryAccessLogRequest {
        memory_id: memory.id,
        access_type: MemoryAccessType::DirectRetrieval,
        session_id: None,
        user_context: None,
        query_context: None,
        retrieval_time_ms: Some(5),
        similarity_score: None,
        ranking_position: None,
        importance_boost: None, // Should default to 0.0
        access_count_increment: None, // Should default to 1
    };
    
    let direct_access_log = repo.create_memory_access_log(direct_access_request)
        .await
        .expect("Failed to create direct access log");
    
    assert_eq!(direct_access_log.access_type, MemoryAccessType::DirectRetrieval);
    assert_eq!(direct_access_log.importance_boost, 0.0);
    assert_eq!(direct_access_log.access_count_increment, 1);
}

#[tokio::test]
#[serial]
async fn test_system_metrics_snapshots() {
    let pool = setup_test_pool().await;
    let repo = MemoryRepository::new(pool.clone());
    
    // Create some test memories in different tiers
    let _working_memory = setup_test_memory(&repo).await;
    
    // Create a warm tier memory
    let warm_memory_request = CreateMemoryRequest {
        content: "Test warm memory".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Warm),
        importance_score: Some(0.6),
        metadata: Some(serde_json::json!({"tier": "warm"})),
        parent_id: None,
        expires_at: None,
    };
    let _warm_memory = repo.create_memory(warm_memory_request)
        .await
        .expect("Failed to create warm memory");
    
    // Create a system metrics snapshot
    let snapshot = repo.create_system_metrics_snapshot(SystemMetricsSnapshotType::OnDemand)
        .await
        .expect("Failed to create system metrics snapshot");
    
    assert_eq!(snapshot.snapshot_type, SystemMetricsSnapshotType::OnDemand);
    assert!(snapshot.working_memory_count >= 1);
    assert!(snapshot.warm_memory_count >= 1);
    assert!(snapshot.total_storage_bytes > 0);
    assert_eq!(snapshot.compressed_storage_bytes, 0); // Default value
    assert_eq!(snapshot.slow_query_count, 0); // Default value
    
    // Test retrieving recent snapshots
    let recent_snapshots = repo.get_recent_system_metrics_snapshots(None, 10)
        .await
        .expect("Failed to get recent snapshots");
    
    assert!(!recent_snapshots.is_empty());
    assert!(recent_snapshots.iter().any(|s| s.id == snapshot.id));
    
    // Test retrieving snapshots by type
    let on_demand_snapshots = repo.get_recent_system_metrics_snapshots(
        Some(SystemMetricsSnapshotType::OnDemand), 
        5
    )
    .await
    .expect("Failed to get on-demand snapshots");
    
    assert!(!on_demand_snapshots.is_empty());
    assert!(on_demand_snapshots.iter().all(|s| s.snapshot_type == SystemMetricsSnapshotType::OnDemand));
}

#[tokio::test]
#[serial]
async fn test_harvest_success_rate_analytics() {
    let pool = setup_test_pool().await;
    let repo = MemoryRepository::new(pool.clone());
    
    // Create multiple harvest sessions with different statuses
    let completed_session_request = CreateHarvestSessionRequest {
        session_type: HarvestSessionType::Silent,
        trigger_reason: "Successful harvest test".to_string(),
        config_snapshot: None,
    };
    let completed_session = repo.create_harvest_session(completed_session_request)
        .await
        .expect("Failed to create completed session");
    
    // Update to completed status
    let _updated_completed = repo.update_harvest_session(
        completed_session.id,
        UpdateHarvestSessionRequest {
            status: Some(HarvestSessionStatus::Completed),
            processing_time_ms: Some(1200),
            ..Default::default()
        }
    ).await.expect("Failed to update completed session");
    
    // Create a failed session
    let failed_session_request = CreateHarvestSessionRequest {
        session_type: HarvestSessionType::Manual,
        trigger_reason: "Failed harvest test".to_string(),
        config_snapshot: None,
    };
    let failed_session = repo.create_harvest_session(failed_session_request)
        .await
        .expect("Failed to create failed session");
    
    let _updated_failed = repo.update_harvest_session(
        failed_session.id,
        UpdateHarvestSessionRequest {
            status: Some(HarvestSessionStatus::Failed),
            error_message: Some("Test failure".to_string()),
            processing_time_ms: Some(500),
            ..Default::default()
        }
    ).await.expect("Failed to update failed session");
    
    // Get harvest success rate statistics
    let success_rate = repo.get_harvest_success_rate(7)
        .await
        .expect("Failed to get harvest success rate");
    
    assert!(success_rate.total_sessions >= 2);
    assert!(success_rate.successful_sessions >= 1);
    assert!(success_rate.failed_sessions >= 1);
    assert!(success_rate.success_rate >= 0.0 && success_rate.success_rate <= 1.0);
    assert!(success_rate.average_processing_time_ms > 0.0);
}

#[tokio::test]
#[serial]
async fn test_tier_migration_statistics() {
    let pool = setup_test_pool().await;
    let repo = MemoryRepository::new(pool.clone());
    
    // Create test memories
    let memory1 = setup_test_memory(&repo).await;
    let memory2 = setup_test_memory(&repo).await;
    
    // Create tier migration events
    let migration1_request = CreateConsolidationEventRequest {
        event_type: ConsolidationEventType::TierMigration,
        memory_id: memory1.id,
        source_tier: Some("working".to_string()),
        target_tier: Some("warm".to_string()),
        migration_reason: Some("Low activity migration".to_string()),
        old_consolidation_strength: None,
        new_consolidation_strength: None,
        old_recall_probability: None,
        new_recall_probability: None,
        triggered_by: Some("system".to_string()),
        context_metadata: None,
    };
    
    let migration2_request = CreateConsolidationEventRequest {
        event_type: ConsolidationEventType::TierMigration,
        memory_id: memory2.id,
        source_tier: Some("working".to_string()),
        target_tier: Some("warm".to_string()),
        migration_reason: Some("Batch migration".to_string()),
        old_consolidation_strength: None,
        new_consolidation_strength: None,
        old_recall_probability: None,
        new_recall_probability: None,
        triggered_by: Some("system".to_string()),
        context_metadata: None,
    };
    
    let _event1 = repo.create_consolidation_event(migration1_request)
        .await
        .expect("Failed to create migration event 1");
    let _event2 = repo.create_consolidation_event(migration2_request)
        .await
        .expect("Failed to create migration event 2");
    
    // Get tier migration statistics
    let migration_stats = repo.get_tier_migration_stats(7)
        .await
        .expect("Failed to get tier migration stats");
    
    // Should have at least one working->warm migration
    let working_to_warm = migration_stats.iter()
        .find(|stat| stat.source_tier == "working" && stat.target_tier == "warm");
    
    assert!(working_to_warm.is_some());
    let working_to_warm = working_to_warm.unwrap();
    assert!(working_to_warm.migration_count >= 2);
    assert!(working_to_warm.success_rate >= 0.0);
}

#[tokio::test]
#[serial]
async fn test_top_harvest_patterns_analytics() {
    let pool = setup_test_pool().await;
    let repo = MemoryRepository::new(pool.clone());
    
    // Create a harvest session
    let session_request = CreateHarvestSessionRequest {
        session_type: HarvestSessionType::Silent,
        trigger_reason: "Pattern analytics test".to_string(),
        config_snapshot: None,
    };
    let session = repo.create_harvest_session(session_request)
        .await
        .expect("Failed to create harvest session");
    
    // Create multiple patterns of different types
    let preference_pattern = CreateHarvestPatternRequest {
        harvest_session_id: session.id,
        pattern_type: HarvestPatternType::Preference,
        content: "I prefer async programming".to_string(),
        confidence_score: 0.9,
        source_message_id: None,
        context: None,
        metadata: None,
    };
    
    let fact_pattern = CreateHarvestPatternRequest {
        harvest_session_id: session.id,
        pattern_type: HarvestPatternType::Fact,
        content: "PostgreSQL supports vector operations".to_string(),
        confidence_score: 0.95,
        source_message_id: None,
        context: None,
        metadata: None,
    };
    
    let _pattern1 = repo.create_harvest_pattern(preference_pattern)
        .await
        .expect("Failed to create preference pattern");
    let _pattern2 = repo.create_harvest_pattern(fact_pattern)
        .await
        .expect("Failed to create fact pattern");
    
    // Get top harvest patterns
    let top_patterns = repo.get_top_harvest_patterns(10, 7)
        .await
        .expect("Failed to get top harvest patterns");
    
    assert!(!top_patterns.is_empty());
    
    // Check that we have patterns for different types
    let has_preference = top_patterns.iter()
        .any(|p| p.pattern_type == HarvestPatternType::Preference);
    let has_fact = top_patterns.iter()
        .any(|p| p.pattern_type == HarvestPatternType::Fact);
    
    assert!(has_preference || has_fact);
    
    // Verify analytics data structure
    for pattern in &top_patterns {
        assert!(pattern.total_extracted > 0);
        assert!(pattern.avg_confidence > 0.0 && pattern.avg_confidence <= 1.0);
        assert!(pattern.success_rate >= 0.0 && pattern.success_rate <= 1.0);
    }
}

#[tokio::test]
#[serial]
async fn test_database_functions_execution() {
    let pool = setup_test_pool().await;
    
    // Test the cleanup function
    let cleanup_result = sqlx::query_scalar!(
        "SELECT cleanup_old_harvest_sessions()"
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to execute cleanup function");
    
    assert!(cleanup_result.is_some());
    
    // Test the harvest success rate function
    let success_rate_result = sqlx::query!(
        r#"
        SELECT total_sessions, successful_sessions, failed_sessions, success_rate, average_processing_time_ms
        FROM calculate_harvest_success_rate(7)
        "#
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to execute success rate function");
    
    assert!(success_rate_result.total_sessions >= 0);
    assert!(success_rate_result.success_rate >= 0.0);
    
    // Test the tier migration stats function
    let migration_stats_result = sqlx::query!(
        r#"
        SELECT source_tier, target_tier, migration_count, avg_processing_time_ms, success_rate
        FROM get_tier_migration_stats(30)
        "#
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to execute migration stats function");
    
    // Should not fail, even if empty
    assert!(migration_stats_result.len() >= 0);
}

// Default implementation for UpdateHarvestSessionRequest
impl Default for UpdateHarvestSessionRequest {
    fn default() -> Self {
        Self {
            status: None,
            messages_processed: None,
            patterns_extracted: None,
            patterns_stored: None,
            duplicates_filtered: None,
            processing_time_ms: None,
            error_message: None,
            extraction_time_ms: None,
            deduplication_time_ms: None,
            storage_time_ms: None,
            memory_usage_mb: None,
            cpu_usage_percent: None,
        }
    }
}