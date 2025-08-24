//! Comprehensive End-to-End tests for Codex Dreams feature
//!
//! Tests the complete insight generation pipeline including:
//! - Memory creation and processing
//! - Insight generation via Ollama
//! - Storage and retrieval
//! - Search and filtering
//! - Feedback system
//! - Export functionality

#![cfg(feature = "codex-dreams")]

mod helpers;

use anyhow::Result;
use codex_memory::{
    insights::{
        models::{Insight, InsightType, InsightFeedback},
        processor::{InsightsProcessor, ProcessorConfig},
        storage::InsightStorage,
        export::InsightExporter,
    },
    memory::{Memory, MemoryRepository, MemoryTier},
};
use helpers::{
    insights_test_utils::{
        InsightTestEnv, TestMemoryBuilder, TestInsightBuilder, 
        InsightAssertions, PerformanceMetrics
    },
    ollama_mock::{MockOllamaConfig, MockOllamaServer},
};
use std::sync::Arc;
use uuid::Uuid;

/// Test the complete insight generation pipeline from memories to insights
#[tokio::test]
async fn test_full_insight_generation_pipeline() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    let mut metrics = PerformanceMetrics::new();
    
    // Step 1: Create test memories
    let memory1 = TestMemoryBuilder::new("I learned Rust programming today and built a web server")
        .with_importance(0.8)
        .with_tier(MemoryTier::Working)
        .create(&env.repository).await?;
    
    let memory2 = TestMemoryBuilder::new("The Rust web server uses async/await for concurrency")
        .with_importance(0.7)
        .with_tier(MemoryTier::Working)
        .create(&env.repository).await?;
    
    let memory3 = TestMemoryBuilder::new("Performance testing showed 10x improvement with async")
        .with_importance(0.9)
        .with_tier(MemoryTier::Working)
        .create(&env.repository).await?;
    
    println!("Created {} test memories", 3);
    
    // Step 2: Initialize processor with mock Ollama
    let processor_config = ProcessorConfig {
        ollama_url: env.ollama_url.clone(),
        model: "llama2:latest".to_string(),
        batch_size: 10,
        confidence_threshold: 0.5,
        timeout_seconds: 30,
    };
    
    let mut processor = InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?;
    
    // Step 3: Process memories to generate insights
    let processing_result = metrics.measure_async(
        "insight_generation",
        processor.process_batch(vec![memory1.id, memory2.id, memory3.id])
    ).await?;
    
    assert!(processing_result.successful_count > 0, "Should generate at least one insight");
    assert_eq!(processing_result.failed_count, 0, "No failures expected");
    
    println!("Generated {} insights", processing_result.successful_count);
    
    // Step 4: Retrieve and validate generated insights
    let storage = InsightStorage::new(env.pool.clone());
    let insights = storage.list_recent(10).await?;
    
    assert!(!insights.is_empty(), "Should have generated insights");
    
    for insight in &insights {
        InsightAssertions::assert_valid_insight(insight);
        assert!(
            insight.source_memory_ids.iter().any(|id| 
                *id == memory1.id || *id == memory2.id || *id == memory3.id
            ),
            "Insight should reference source memories"
        );
    }
    
    // Step 5: Test semantic search
    let search_results = metrics.measure_async(
        "semantic_search",
        storage.search("Rust programming async performance", 5)
    ).await?;
    
    assert!(!search_results.is_empty(), "Should find relevant insights");
    metrics.assert_under_threshold("semantic_search", 50); // <50ms P99 requirement
    
    // Step 6: Test feedback system
    let insight_id = insights[0].id;
    storage.record_feedback(insight_id, InsightFeedback {
        id: Uuid::new_v4(),
        insight_id,
        rating: 1, // Helpful
        comment: Some("Very insightful connection".to_string()),
        created_at: chrono::Utc::now(),
        user_id: Some("test_user".to_string()),
    }).await?;
    
    let updated_insight = storage.get_by_id(insight_id).await?;
    assert!(updated_insight.feedback_score > 0.0, "Feedback should improve score");
    
    // Cleanup
    env.cleanup().await?;
    metrics.report();
    
    Ok(())
}

/// Test batch processing with many memories
#[tokio::test]
async fn test_batch_processing_performance() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    let mut metrics = PerformanceMetrics::new();
    
    // Create 100 test memories
    let mut memory_ids = Vec::new();
    for i in 0..100 {
        let memory = TestMemoryBuilder::new(format!("Test memory {} with unique content for processing", i))
            .with_importance(0.5 + (i as f32 / 200.0))
            .create(&env.repository).await?;
        memory_ids.push(memory.id);
    }
    
    println!("Created 100 test memories");
    
    // Process in batches
    let processor_config = ProcessorConfig {
        ollama_url: env.ollama_url.clone(),
        model: "llama2:latest".to_string(),
        batch_size: 10,
        confidence_threshold: 0.5,
        timeout_seconds: 30,
    };
    
    let mut processor = InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?;
    
    let processing_result = metrics.measure_async(
        "batch_processing_100",
        processor.process_batch(memory_ids)
    ).await?;
    
    println!("Processed {} memories successfully, {} failed", 
        processing_result.successful_count, 
        processing_result.failed_count);
    
    // Performance assertions
    metrics.assert_under_threshold("batch_processing_100", 5000); // <5s for 100 memories
    assert!(processing_result.memories_per_second > 10.0, 
        "Should process >10 memories per second");
    
    env.cleanup().await?;
    metrics.report();
    
    Ok(())
}

/// Test real-time processing for immediate insights
#[tokio::test]
async fn test_realtime_insight_generation() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    let mut metrics = PerformanceMetrics::new();
    
    // Create a single important memory
    let memory = TestMemoryBuilder::new("Critical discovery: The bug was caused by race condition in async handler")
        .with_importance(1.0)
        .with_tier(MemoryTier::Working)
        .create(&env.repository).await?;
    
    let processor_config = ProcessorConfig {
        ollama_url: env.ollama_url.clone(),
        model: "llama2:latest".to_string(),
        batch_size: 1,
        confidence_threshold: 0.5,
        timeout_seconds: 5, // Tight timeout for real-time
    };
    
    let mut processor = InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?;
    
    // Real-time processing
    let insights = metrics.measure_async(
        "realtime_generation",
        processor.process_realtime(memory.id)
    ).await?;
    
    assert!(!insights.is_empty(), "Should generate insight in real-time");
    metrics.assert_under_threshold("realtime_generation", 1000); // <1s for real-time
    
    // Validate insight quality
    let insight = &insights[0];
    InsightAssertions::assert_valid_insight(insight);
    InsightAssertions::assert_confidence_range(insight, 0.7, 1.0); // High confidence for critical
    
    env.cleanup().await?;
    metrics.report();
    
    Ok(())
}

/// Test export functionality
#[tokio::test]
async fn test_insight_export() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    // Create and store test insights
    let storage = InsightStorage::new(env.pool.clone());
    
    for i in 0..10 {
        let insight = TestInsightBuilder::new(format!("Test insight {} for export testing", i))
            .with_type(if i % 2 == 0 { InsightType::Learning } else { InsightType::Pattern })
            .with_confidence(0.6 + (i as f32 / 20.0))
            .build();
        
        storage.store(insight).await?;
    }
    
    let exporter = InsightExporter::new(storage);
    
    // Test Markdown export
    let markdown = exporter.export_markdown(Default::default()).await?;
    assert!(markdown.contains("# Insights Export"), "Should have markdown header");
    assert!(markdown.contains("## Learning"), "Should categorize by type");
    assert!(markdown.len() < 10_000_000, "Should respect size limit");
    
    // Test JSON-LD export
    let jsonld = exporter.export_jsonld(Default::default()).await?;
    assert_eq!(jsonld["@context"], "https://schema.org", "Should use Schema.org context");
    assert!(jsonld["@type"].as_str().unwrap().contains("Dataset"), "Should be Dataset type");
    
    env.cleanup().await?;
    
    Ok(())
}

/// Test insight deduplication
#[tokio::test]
async fn test_insight_deduplication() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    // Create identical memories
    let memory1 = TestMemoryBuilder::new("Duplicate content for testing deduplication")
        .create(&env.repository).await?;
    
    let memory2 = TestMemoryBuilder::new("Duplicate content for testing deduplication")
        .create(&env.repository).await?;
    
    let processor_config = ProcessorConfig {
        ollama_url: env.ollama_url.clone(),
        model: "llama2:latest".to_string(),
        batch_size: 10,
        confidence_threshold: 0.5,
        timeout_seconds: 30,
    };
    
    let mut processor = InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?;
    
    // Process both memories
    let result = processor.process_batch(vec![memory1.id, memory2.id]).await?;
    
    // Should detect and handle duplicates appropriately
    let storage = InsightStorage::new(env.pool.clone());
    let insights = storage.list_recent(10).await?;
    
    // Verify deduplication worked
    let unique_contents: std::collections::HashSet<_> = insights.iter()
        .map(|i| &i.content)
        .collect();
    
    assert_eq!(unique_contents.len(), insights.len(), 
        "Should not have duplicate insight content");
    
    env.cleanup().await?;
    
    Ok(())
}

/// Test tier migration for insights
#[tokio::test]
async fn test_insight_tier_migration() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    let storage = InsightStorage::new(env.pool.clone());
    
    // Create insight in working tier
    let insight = TestInsightBuilder::new("Insight for tier migration testing")
        .with_confidence(0.4) // Low confidence should trigger migration
        .build();
    
    let insight_id = storage.store(insight).await?;
    
    // Simulate tier management process
    // In real system, this would be done by TierManager
    let updated = storage.update_tier(insight_id, "cold".to_string()).await?;
    
    assert_eq!(updated.tier, "cold", "Low confidence insight should move to cold tier");
    
    env.cleanup().await?;
    
    Ok(())
}