//! Simplified End-to-End tests that work with the current implementation
//!
//! These tests focus on the core functionality that is available in the current codebase

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryStatus, MemoryTier, RangeFilter, SearchRequest, UpdateMemoryRequest,
};
use std::sync::Arc;
use test_helpers::{PerformanceMeter, TestDataGenerator, TestEnvironment};
use tokio::time::Duration as TokioDuration;
use tracing_test::traced_test;
use uuid::Uuid;

/// Test basic CRUD operations with embeddings
#[tokio::test]
#[traced_test]
async fn test_basic_memory_crud_with_embeddings() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Create memory with automatic embedding generation
    let create_request = CreateMemoryRequest {
        content: "This is a test memory for CRUD operations".to_string(),
        embedding: None, // Should be generated automatically
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.8),
        metadata: Some(env.get_test_metadata(None)),
        parent_id: None,
        expires_at: None,
    };

    let memory = env
        .repository
        .create_memory(create_request)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(!memory.content.is_empty());
    assert!(memory.embedding.is_some());
    assert_eq!(memory.tier, MemoryTier::Working);
    assert_eq!(memory.importance_score, 0.8);
    assert_eq!(memory.status, MemoryStatus::Active);

    // Test 2: Retrieve memory (should increment access count)
    let retrieved = env
        .repository
        .get_memory(memory.id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert_eq!(retrieved.content, memory.content);
    assert_eq!(retrieved.access_count, memory.access_count + 1);
    assert!(retrieved.last_accessed_at.is_some());

    // Test 3: Update memory
    let update_request = UpdateMemoryRequest {
        content: Some("Updated content with new embedding".to_string()),
        embedding: None, // Should be regenerated for new content
        tier: Some(MemoryTier::Warm),
        importance_score: Some(0.9),
        metadata: Some(env.get_test_metadata(Some(serde_json::json!({"updated": true})))),
        expires_at: Some(Utc::now() + Duration::hours(24)),
    };

    let updated = env
        .repository
        .update_memory(memory.id, update_request)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert_eq!(updated.content, "Updated content with new embedding");
    assert_eq!(updated.tier, MemoryTier::Warm);
    assert_eq!(updated.importance_score, 0.9);
    assert!(updated.expires_at.is_some());
    assert!(updated.updated_at > memory.updated_at);

    // Test 4: Delete memory
    env.repository
        .delete_memory(memory.id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let result = env.repository.get_memory(memory.id).await;
    assert!(
        result.is_err(),
        "Memory should be deleted and not retrievable"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test semantic search functionality
#[tokio::test]
#[traced_test]
async fn test_semantic_search_functionality() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create diverse test memories
    let test_memories = vec![
        (
            "How to implement a binary search tree in Rust",
            "coding",
            0.9,
        ),
        ("Recipe for chocolate chip cookies", "cooking", 0.7),
        ("Meeting notes from standup on Monday", "meeting", 0.6),
        ("Error: cannot borrow x as mutable", "error", 0.8),
        ("Database optimization strategies", "technical", 0.85),
        ("Rust programming language features", "coding", 0.9),
        ("Team lunch at Italian restaurant", "social", 0.4),
        ("Performance benchmarks for new API", "metrics", 0.75),
    ];

    let mut created_memories = Vec::new();
    for (content, category, importance) in test_memories {
        let memory = env
            .create_test_memory(content, MemoryTier::Working, importance)
            .await?;
        created_memories.push((memory, category));
    }

    // Wait for embeddings to be generated and indexed
    env.wait_for_consistency().await;

    // Test 1: Semantic search for programming content
    let programming_results = env
        .test_search("Rust programming and coding", Some(5))
        .await?;
    assert!(
        !programming_results.is_empty(),
        "Should find programming-related memories"
    );

    // Test 2: Search with importance filtering
    let search_request = SearchRequest {
        query_text: Some("database and performance".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: Some(RangeFilter {
            min: Some(0.7),
            max: None,
        }),
        metadata_filters: Some(env.get_test_metadata(None)),
        tags: None,
        limit: Some(5),
        offset: None,
        cursor: None,
        similarity_threshold: Some(0.1),
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: Some(true),
    };

    let filtered_results = env
        .repository
        .search_memories(search_request)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(!filtered_results.results.is_empty());

    // All results should have importance >= 0.7
    for result in &filtered_results.results {
        assert!(
            result.memory.importance_score >= 0.7,
            "All results should meet importance threshold"
        );
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test concurrent memory operations
#[tokio::test]
#[traced_test]
async fn test_concurrent_memory_operations() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test concurrent memory creation
    let mut handles = Vec::new();
    let repo = Arc::clone(&env.repository);
    let test_id = env.test_id.clone();

    for i in 0..10 {
        let repo_clone = Arc::clone(&repo);
        let test_id_clone = test_id.clone();
        let handle = tokio::spawn(async move {
            let request = CreateMemoryRequest {
                content: format!("Concurrent memory {}", i),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5 + (i as f32 * 0.05)),
                metadata: Some(serde_json::json!({
                    "test_id": test_id_clone,
                    "concurrent_test": true,
                    "worker_id": i
                })),
                parent_id: None,
                expires_at: None,
            };
            repo_clone
                .create_memory(request)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        });
        handles.push(handle);
    }

    let mut created_memories = Vec::new();
    for handle in handles {
        created_memories.push(handle.await??);
    }
    assert_eq!(created_memories.len(), 10);

    // Verify all memories were created with unique IDs
    let mut ids = std::collections::HashSet::new();
    for memory in &created_memories {
        assert!(ids.insert(memory.id), "Memory IDs should be unique");
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test performance under load
#[tokio::test]
#[traced_test]
async fn test_performance_under_load() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test batch memory creation performance
    let create_meter = PerformanceMeter::new("batch_memory_creation");
    let num_memories = 20; // Reduced for faster testing

    // Create handles manually to avoid lifetime issues
    let mut handles = Vec::new();
    let repo = Arc::clone(&env.repository);
    let test_id = env.test_id.clone();

    for i in 0..num_memories {
        let repo_clone = Arc::clone(&repo);
        let test_id_clone = test_id.clone();
        let handle = tokio::spawn(async move {
            let request = CreateMemoryRequest {
                content: format!(
                    "Performance test memory {} with some additional content to make it realistic",
                    i
                ),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5 + ((i % 10) as f32) * 0.05),
                metadata: Some(serde_json::json!({
                    "test_id": test_id_clone,
                    "performance_test": true,
                    "batch_id": i
                })),
                parent_id: None,
                expires_at: None,
            };
            repo_clone
                .create_memory(request)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        });
        handles.push(handle);
    }

    let mut creation_operations = Vec::new();
    for handle in handles {
        creation_operations.push(handle.await??);
    }

    let create_result = create_meter.finish();
    create_result.assert_under(TokioDuration::from_secs(30)); // Should complete in under 30 seconds

    let create_ops_per_sec = create_result.operations_per_second(num_memories);
    println!("Memory creation: {:.1} ops/sec", create_ops_per_sec);
    assert!(
        create_ops_per_sec >= 0.5,
        "Should achieve at least 0.5 creation/sec"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test error handling
#[tokio::test]
#[traced_test]
async fn test_error_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Invalid memory ID
    let invalid_id = Uuid::new_v4();
    let result = env.repository.get_memory(invalid_id).await;
    assert!(
        result.is_err(),
        "Should return error for non-existent memory"
    );

    // Test 2: Large content handling
    let large_content = TestDataGenerator::large_content(10); // 10KB content
    let large_request = CreateMemoryRequest {
        content: large_content.clone(),
        embedding: None,
        tier: Some(MemoryTier::Cold),
        importance_score: Some(0.1),
        metadata: Some(env.get_test_metadata(Some(serde_json::json!({"size_test": true})))),
        parent_id: None,
        expires_at: None,
    };

    // Should either handle gracefully or reject appropriately
    match env.repository.create_memory(large_request).await {
        Ok(memory) => {
            assert!(!memory.content.is_empty());
            println!("Large content accepted: {} bytes", memory.content.len());
        }
        Err(e) => {
            println!("Large content rejected (acceptable): {}", e);
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test data persistence and consistency
#[tokio::test]
#[traced_test]
async fn test_data_persistence_and_consistency() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create memory with all fields populated
    let comprehensive_request = CreateMemoryRequest {
        content: "Comprehensive test memory with all fields populated for persistence testing"
            .to_string(),
        embedding: None, // Will be generated
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.85),
        metadata: Some(env.get_test_metadata(Some(serde_json::json!({
            "comprehensive": true,
            "timestamp": Utc::now().to_rfc3339(),
            "source": "e2e_test",
            "tags": ["test", "comprehensive", "persistence"]
        })))),
        parent_id: None,
        expires_at: Some(Utc::now() + Duration::hours(24)),
    };

    let created = env
        .repository
        .create_memory(comprehensive_request)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Verify all fields are correctly persisted
    assert_eq!(
        created.content,
        "Comprehensive test memory with all fields populated for persistence testing"
    );
    assert_eq!(created.tier, MemoryTier::Working);
    assert_eq!(created.importance_score, 0.85);
    assert!(!created.metadata.is_null());
    assert!(created.expires_at.is_some());
    assert!(created.embedding.is_some());
    assert_eq!(created.access_count, 0);
    assert_eq!(created.status, MemoryStatus::Active);

    // Verify access tracking
    let retrieved = env
        .repository
        .get_memory(created.id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert_eq!(retrieved.access_count, created.access_count + 1);
    assert!(retrieved.last_accessed_at.is_some());
    assert!(retrieved.last_accessed_at.unwrap() > created.created_at);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test diverse content handling
#[tokio::test]
#[traced_test]
async fn test_diverse_content_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Code content
    let code_samples = TestDataGenerator::code_samples();
    let mut code_memories = Vec::new();

    for (code, description) in code_samples.iter() {
        let memory = env
            .create_test_memory(
                &format!("Code sample: {} - {}", code, description),
                MemoryTier::Working,
                0.8,
            )
            .await?;
        code_memories.push(memory);
    }

    // Test 2: Conversation content
    let conversation_samples = TestDataGenerator::conversation_samples();
    let mut conversation_memories = Vec::new();

    for content in conversation_samples.iter() {
        let memory = env
            .create_test_memory(content, MemoryTier::Working, 0.6)
            .await?;
        conversation_memories.push(memory);
    }

    // Test 3: Verify semantic search works across content types
    env.wait_for_consistency().await;

    // Search for coding content
    let code_results = env
        .test_search("programming and software development", Some(10))
        .await?;
    assert!(!code_results.is_empty(), "Should find code-related content");

    // Test 4: Content with special characters and unicode
    let special_content = vec![
        "Unicode test: こんにちは 世界",
        "Emoji test: 🚀 🎯 💡 ⚡",
        "Special chars: @#$%^&*()_+-=[]{}|;':\",./<>?",
        "Mixed: Code snippet `let x = 42;` with 中文 and émojis 🔥",
    ];

    for content in special_content {
        let memory = env
            .create_test_memory(content, MemoryTier::Working, 0.5)
            .await?;
        assert_eq!(
            memory.content, content,
            "Special characters should be preserved"
        );
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test statistics and monitoring functionality
#[tokio::test]
#[traced_test]
async fn test_statistics_and_monitoring() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create a known set of memories for statistics testing
    let memories = env.create_test_memories(10).await?;
    assert_eq!(memories.len(), 10);

    // Wait for all operations to complete
    env.wait_for_consistency().await;

    // Test 1: Basic repository statistics
    let repo_stats = env
        .repository
        .get_statistics()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(repo_stats.total_active.unwrap_or(0) >= 10);

    // Test 2: Test environment statistics
    let test_stats = env.get_test_statistics().await?;
    assert_eq!(test_stats.total_count, 10);
    assert!(test_stats.working_count > 0);
    assert!(test_stats.warm_count > 0);
    assert!(test_stats.cold_count > 0);
    assert!(test_stats.avg_importance > 0.0);

    // Test 3: Access pattern tracking
    // Access some memories multiple times
    for memory in &memories[0..5] {
        for _ in 0..3 {
            let _ = env
                .repository
                .get_memory(memory.id)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
    }

    env.wait_for_consistency().await;

    // Verify access counts increased
    for memory in &memories[0..5] {
        let updated = env
            .repository
            .get_memory(memory.id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        assert!(
            updated.access_count >= 3,
            "Access count should reflect multiple reads"
        );
    }

    env.cleanup_test_data().await?;
    Ok(())
}
