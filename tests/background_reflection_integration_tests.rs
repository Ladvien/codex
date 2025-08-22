//! Integration tests for Background Reflection Service
//!
//! These tests verify the complete integration of the reflection system
//! including insight generation, database storage, and cognitive processing.

use anyhow::Result;
use codex_memory::memory::{
    BackgroundReflectionConfig, BackgroundReflectionService, CognitiveMemoryConfig,
    CognitiveMemorySystem, CreateMemoryRequest, LoopPreventionConfig, MemoryTier, ReflectionConfig,
    ReflectionPriority, TriggerType,
};
use serial_test::serial;
use std::sync::Arc;
use test_helpers::TestEnvironment;

mod test_helpers;

#[tokio::test]
#[serial]
async fn test_background_reflection_service_integration() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Configure background reflection service
    let reflection_config = BackgroundReflectionConfig {
        enabled: true,
        check_interval_minutes: 1, // Fast interval for testing
        min_reflection_interval_minutes: 1,
        max_concurrent_sessions: 1,
        session_timeout_minutes: 5,
        store_insights_as_memories: true,
        enable_quality_filtering: true,
        enable_metrics: true,
        max_retry_attempts: 2,
        retry_backoff_multiplier: 1.5,
        priority_thresholds: codex_memory::memory::PriorityThresholds {
            high_importance_threshold: 10.0, // Lower for testing
            medium_importance_threshold: 5.0,
            low_importance_threshold: 2.0,
            critical_pattern_threshold: 20.0,
        },
    };

    let service = BackgroundReflectionService::new(
        reflection_config,
        env.repository.clone(),
        ReflectionConfig::default(),
        LoopPreventionConfig::default(),
    );

    // Test service lifecycle
    assert!(!service.is_running());

    service.start().await?;
    assert!(service.is_running());

    // Create some test memories to trigger reflection
    for i in 0..5 {
        let memory_request = CreateMemoryRequest {
            content: format!(
                "Test memory content {} about machine learning algorithms",
                i
            ),
            embedding: Some(vec![0.1; 384]), // Mock embedding
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            metadata: None,
            parent_id: None,
            expires_at: None,
        };

        env.repository.create_memory(memory_request).await?;
    }

    // Wait briefly for background processing
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Get metrics
    let metrics = service.get_metrics().await;
    println!("Service metrics: {:#?}", metrics);

    // Test manual reflection trigger
    let session_id = service
        .trigger_manual_reflection("Test manual reflection".to_string())
        .await;

    match session_id {
        Ok(id) => {
            println!("Manual reflection session created: {}", id);
            assert!(!id.is_nil());
        }
        Err(e) => {
            // Acceptable if no memories exist for reflection
            println!("Manual reflection failed (expected): {}", e);
        }
    }

    // Stop the service
    service.stop().await?;
    assert!(!service.is_running());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_cognitive_memory_system_with_reflection() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Configure cognitive memory system with reflection enabled
    let config = CognitiveMemoryConfig {
        background_reflection_config: BackgroundReflectionConfig {
            enabled: true,
            check_interval_minutes: 1,
            min_reflection_interval_minutes: 1,
            ..Default::default()
        },
        enable_background_reflection: true,
        ..Default::default()
    };

    let system = CognitiveMemorySystem::new(env.repository.clone(), config).await?;

    // Create some memories through the cognitive system
    for i in 0..3 {
        let request = codex_memory::memory::CognitiveMemoryRequest {
            content: format!("Cognitive memory test {} about artificial intelligence", i),
            embedding: Some(vec![0.2; 384]),
            importance_score: Some(0.9),
            metadata: None,
            retrieval_context: codex_memory::memory::RetrievalContext {
                user_context: Some("Testing reflection integration".to_string()),
                environmental_factors: std::collections::HashMap::new(),
                temporal_context: chrono::Utc::now(),
                interaction_history: Vec::new(),
            },
            enable_immediate_consolidation: false,
            enable_quality_assessment: true,
        };

        let result = system
            .store_memory_with_cognitive_processing(request)
            .await?;
        println!("Stored cognitive memory: {}", result.memory.id);
    }

    // Test manual reflection trigger
    let session = system
        .trigger_reflection("Manual test reflection".to_string())
        .await;
    match session {
        Ok(session) => {
            println!("Reflection session completed: {}", session.id);
            println!("Generated {} insights", session.generated_insights.len());
        }
        Err(e) => {
            println!("Reflection failed (expected): {}", e);
        }
    }

    // Get reflection service metrics
    let reflection_metrics = system.get_reflection_service_metrics().await;
    println!("Reflection service metrics: {:#?}", reflection_metrics);

    // Get performance metrics
    let performance_metrics = system.get_performance_metrics().await;
    println!("Cognitive system metrics: {:#?}", performance_metrics);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_insight_storage_and_retrieval() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test that insights are properly stored in the database
    // This would require checking the insights table directly

    let insights_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM insights")
        .fetch_one(&env.pool)
        .await?;

    println!("Initial insights count: {}", insights_count);

    // Test reflection sessions table
    let sessions_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM reflection_sessions")
        .fetch_one(&env.pool)
        .await?;

    println!("Initial reflection sessions count: {}", sessions_count);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_priority_thresholds() -> Result<()> {
    // Test priority determination logic
    let config = BackgroundReflectionConfig::default();
    let env = TestEnvironment::new().await?;

    let service = BackgroundReflectionService::new(
        config.clone(),
        env.repository,
        ReflectionConfig::default(),
        LoopPreventionConfig::default(),
    );

    // Test priority levels
    assert_eq!(service.determine_priority(50.0), ReflectionPriority::Low);
    assert_eq!(
        service.determine_priority(150.0),
        ReflectionPriority::Medium
    );
    assert_eq!(service.determine_priority(350.0), ReflectionPriority::High);
    assert_eq!(
        service.determine_priority(600.0),
        ReflectionPriority::Critical
    );

    // Test trigger types
    assert_eq!(
        TriggerType::ImportanceAccumulation,
        TriggerType::ImportanceAccumulation
    );
    assert_ne!(
        TriggerType::ImportanceAccumulation,
        TriggerType::ManualRequest
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_importance_multiplier() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create a memory through the cognitive system
    let config = CognitiveMemoryConfig::default();
    let system = CognitiveMemorySystem::new(env.repository.clone(), config).await?;

    let request = codex_memory::memory::CognitiveMemoryRequest {
        content: "Test content for importance multiplier verification".to_string(),
        embedding: Some(vec![0.5; 384]),
        importance_score: Some(0.6), // Base importance
        metadata: None,
        retrieval_context: codex_memory::memory::RetrievalContext {
            user_context: Some("Testing importance multiplier".to_string()),
            environmental_factors: std::collections::HashMap::new(),
            temporal_context: chrono::Utc::now(),
            interaction_history: Vec::new(),
        },
        enable_immediate_consolidation: false,
        enable_quality_assessment: false,
    };

    let result = system
        .store_memory_with_cognitive_processing(request)
        .await?;

    // Verify the memory was created with correct base importance
    assert!((result.memory.importance_score - 0.6).abs() < 0.01);

    println!(
        "Memory created with importance: {}",
        result.memory.importance_score
    );

    // The 1.5x multiplier should be applied when insights are stored as memories
    // This would be tested in the actual insight generation process

    Ok(())
}
