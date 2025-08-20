//! Comprehensive End-to-End tests for the Agentic Memory System
//!
//! These tests cover the complete memory lifecycle including:
//! - CRUD operations with real embeddings
//! - Search functionality with semantic similarity
//! - Memory tier management and migration
//! - Concurrent access patterns
//! - Performance under load
//! - Error handling and recovery

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryStatus, MemoryTier, RangeFilter, SearchRequest, SearchType,
    UpdateMemoryRequest,
};
use std::sync::Arc;
use test_helpers::{ConcurrentTester, PerformanceMeter, TestDataGenerator, TestEnvironment};
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

    let memory = env.repository.create_memory(create_request).await?;
    assert!(!memory.content.is_empty());
    assert!(memory.embedding.is_some());
    assert_eq!(memory.tier, MemoryTier::Working);
    assert_eq!(memory.importance_score, 0.8);
    assert_eq!(memory.status, MemoryStatus::Active);

    // Test 2: Retrieve memory (should increment access count)
    let retrieved = env.repository.get_memory(memory.id).await?;
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
        .await?;
    assert_eq!(updated.content, "Updated content with new embedding");
    assert_eq!(updated.tier, MemoryTier::Warm);
    assert_eq!(updated.importance_score, 0.9);
    assert!(updated.expires_at.is_some());
    assert!(updated.updated_at > memory.updated_at);

    // Test 4: Delete memory
    env.repository.delete_memory(memory.id).await?;
    let result = env.repository.get_memory(memory.id).await;
    assert!(
        result.is_err(),
        "Memory should be deleted and not retrievable"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test semantic search functionality with different query types
#[tokio::test]
#[traced_test]
async fn test_semantic_search_functionality() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create diverse test memories with different content types
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

    // Verify results are relevant (should contain Rust or coding related content)
    let has_rust_content = programming_results.iter().any(|r| {
        r.memory.content.to_lowercase().contains("rust")
            || r.memory.content.to_lowercase().contains("coding")
    });
    assert!(
        has_rust_content,
        "Search results should contain Rust/coding content"
    );

    // Test 2: Search for error-related content
    let error_results = env
        .test_search("error messages and debugging", Some(3))
        .await?;
    let has_error_content = error_results
        .iter()
        .any(|r| r.memory.content.to_lowercase().contains("error"));
    assert!(has_error_content, "Should find error-related content");

    // Test 3: Search with importance filtering
    let search_request = SearchRequest {
        query_text: Some("database and performance".to_string()),
        query_embedding: None,
        search_type: Some(SearchType::Semantic),
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

    let filtered_results = env.repository.search_memories(search_request).await?;
    assert!(!filtered_results.results.is_empty());

    // All results should have importance >= 0.7
    for result in &filtered_results.results {
        assert!(
            result.memory.importance_score >= 0.7,
            "All results should meet importance threshold"
        );
    }

    // Test 4: Search with tier filtering
    let tier_search = SearchRequest {
        query_text: Some("meeting notes".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: Some(MemoryTier::Working),
        date_range: None,
        importance_range: None,
        metadata_filters: Some(env.get_test_metadata(None)),
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: None,
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let tier_results = env.repository.search_memories(tier_search).await?;
    for result in &tier_results.results {
        assert_eq!(result.memory.tier, MemoryTier::Working);
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test memory tier management and migration logic
#[tokio::test]
#[traced_test]
async fn test_memory_tier_management() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Create memories in different tiers
    let working_memory = env
        .create_test_memory("High importance working memory", MemoryTier::Working, 0.9)
        .await?;

    let warm_memory = env
        .create_test_memory("Medium importance warm memory", MemoryTier::Warm, 0.6)
        .await?;

    let cold_memory = env
        .create_test_memory("Low importance cold memory", MemoryTier::Cold, 0.3)
        .await?;

    // Test 2: Verify tier-based retrieval
    assert_eq!(working_memory.tier, MemoryTier::Working);
    assert_eq!(warm_memory.tier, MemoryTier::Warm);
    assert_eq!(cold_memory.tier, MemoryTier::Cold);

    // Test 3: Test tier migration (simulate based on access patterns)
    // Update working memory to have low importance (should be candidate for migration)
    let migration_request = UpdateMemoryRequest {
        content: None,
        embedding: None,
        tier: None,                  // Keep current tier for now
        importance_score: Some(0.1), // Very low importance
        metadata: Some(env.get_test_metadata(Some(serde_json::json!({"migration_test": true})))),
        expires_at: None,
    };

    let updated_memory = env
        .repository
        .update_memory(working_memory.id, migration_request)
        .await?;
    assert_eq!(updated_memory.importance_score, 0.1);

    // Check migration logic
    assert!(
        updated_memory.should_migrate(),
        "Low importance memory should be migration candidate"
    );

    if let Some(next_tier) = updated_memory.next_tier() {
        assert_eq!(next_tier, MemoryTier::Warm);
    }

    // Test 4: Verify statistics by tier
    let stats = env.get_test_statistics().await?;
    assert!(stats.total_count >= 3);
    assert!(stats.working_count >= 1);
    assert!(stats.warm_count >= 1);
    assert!(stats.cold_count >= 1);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test concurrent access patterns and data integrity
#[tokio::test]
#[traced_test]
async fn test_concurrent_memory_operations() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Concurrent memory creation
    let creation_ops = (0..10)
        .map(|i| {
            let repo = Arc::clone(&env.repository);
            let test_id = env.test_id.clone();
            move || async move {
                let request = CreateMemoryRequest {
                    content: format!("Concurrent memory {}", i),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.5 + (i as f64 * 0.05)),
                    metadata: Some(serde_json::json!({
                        "test_id": test_id,
                        "concurrent_test": true,
                        "worker_id": i
                    })),
                    parent_id: None,
                    expires_at: None,
                };
                repo.create_memory(request).await
            }
        })
        .collect();

    let created_memories_results = ConcurrentTester::run_concurrent(creation_ops).await?;
    assert_eq!(created_memories_results.len(), 10);

    // Unwrap successful results and collect memories
    let mut created_memories = Vec::new();
    for result in created_memories_results {
        created_memories.push(result.map_err(|e| anyhow::anyhow!("{}", e))?);
    }

    // Verify all memories were created with unique IDs
    let mut ids = std::collections::HashSet::new();
    for memory in &created_memories {
        assert!(ids.insert(memory.id), "Memory IDs should be unique");
    }

    // Test 2: Concurrent reads of the same memory
    let shared_memory = &created_memories[0];
    let read_ops = (0..5)
        .map(|_| {
            let repo = Arc::clone(&env.repository);
            let memory_id = shared_memory.id;
            move || async move { repo.get_memory(memory_id).await }
        })
        .collect();

    let read_results_raw = ConcurrentTester::run_concurrent(read_ops).await?;
    assert_eq!(read_results_raw.len(), 5);

    // Unwrap successful results
    let mut read_results = Vec::new();
    for result in read_results_raw {
        read_results.push(result.map_err(|e| anyhow::anyhow!("{}", e))?);
    }

    // All reads should return the same content but may have different access counts
    for result in &read_results {
        assert_eq!(result.content, shared_memory.content);
        assert!(result.access_count > 0);
    }

    // Test 3: Concurrent searches
    let search_ops = (0..5)
        .map(|i| {
            let repo = Arc::clone(&env.repository);
            let test_id = env.test_id.clone();
            move || async move {
                let request = SearchRequest {
                    query_text: Some(format!("Concurrent memory {}", i)),
                    query_embedding: None,
                    search_type: None,
                    hybrid_weights: None,
                    tier: None,
                    date_range: None,
                    importance_range: None,
                    metadata_filters: Some(serde_json::json!({"test_id": test_id})),
                    tags: None,
                    limit: Some(5),
                    offset: None,
                    cursor: None,
                    similarity_threshold: None,
                    include_metadata: None,
                    include_facets: None,
                    ranking_boost: None,
                    explain_score: None,
                };
                repo.search_memories(request).await
            }
        })
        .collect();

    let search_results_raw = ConcurrentTester::run_concurrent(search_ops).await?;
    assert_eq!(search_results_raw.len(), 5);

    // Unwrap successful results
    let mut search_results = Vec::new();
    for result in search_results_raw {
        search_results.push(result.map_err(|e| anyhow::anyhow!("{}", e))?);
    }

    // All searches should return results
    for result in &search_results {
        assert!(!result.results.is_empty());
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test system performance under load
#[tokio::test]
#[traced_test]
async fn test_performance_under_load() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Extract values we need to avoid lifetime issues with move closures
    let repository = Arc::clone(&env.repository);
    let test_id = env.test_id.clone();

    // Test 1: Batch memory creation performance
    let create_meter = PerformanceMeter::new("batch_memory_creation");
    let num_memories = 50;

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
                importance_score: Some(0.5 + ((i % 10) as f64) * 0.05),
                metadata: Some(serde_json::json!({
                    "test_id": test_id_clone,
                    "performance_test": true,
                    "batch_id": i
                })),
                parent_id: None,
                expires_at: None,
            };
            repo_clone.create_memory(request).await
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
        create_ops_per_sec >= 1.0,
        "Should achieve at least 1 creation/sec"
    );

    // Test 2: Search performance
    let search_meter = PerformanceMeter::new("concurrent_searches");
    let num_searches = 20;

    let repo_for_search = Arc::clone(&repository);
    let test_id_for_search = test_id.clone();
    let search_operations = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repo_for_search);
            let test_id = test_id_for_search.clone();
            async move {
                let request = SearchRequest {
                    query_text: Some(format!("Performance test memory {}", i % 10)),
                    query_embedding: None,
                    search_type: None,
                    hybrid_weights: None,
                    tier: None,
                    date_range: None,
                    importance_range: None,
                    metadata_filters: Some(serde_json::json!({"test_id": test_id})),
                    tags: None,
                    limit: Some(10),
                    offset: None,
                    cursor: None,
                    similarity_threshold: Some(0.1),
                    include_metadata: None,
                    include_facets: None,
                    ranking_boost: None,
                    explain_score: None,
                };
                repo.search_memories(request).await
            }
        },
        num_searches,
    )
    .await?;

    let search_result = search_meter.finish();
    search_result.assert_under(TokioDuration::from_secs(15)); // Should complete in under 15 seconds

    let search_ops_per_sec = search_result.operations_per_second(num_searches);
    println!("Search operations: {:.1} ops/sec", search_ops_per_sec);
    assert!(
        search_ops_per_sec >= 2.0,
        "Should achieve at least 2 searches/sec"
    );

    // Test 3: Mixed workload performance
    let mixed_meter = PerformanceMeter::new("mixed_workload");

    // Mixed operations test
    let repo_for_mixed = Arc::clone(&repository);
    let test_id_for_mixed = test_id.clone();
    let mixed_operations = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repo_for_mixed);
            let test_id = test_id_for_mixed.clone();
            async move {
                match i % 3 {
                    0 => {
                        // Create operation
                        let request = CreateMemoryRequest {
                            content: format!("Mixed workload memory {}", i),
                            embedding: None,
                            tier: Some(MemoryTier::Working),
                            importance_score: Some(0.6),
                            metadata: Some(serde_json::json!({"test_id": test_id, "mixed": true})),
                            parent_id: None,
                            expires_at: None,
                        };
                        repo.create_memory(request).await.map(|_| ())
                    }
                    1 => {
                        // Search operation
                        let request = SearchRequest {
                            query_text: Some("mixed workload".to_string()),
                            query_embedding: None,
                            search_type: None,
                            hybrid_weights: None,
                            tier: None,
                            date_range: None,
                            importance_range: None,
                            metadata_filters: Some(serde_json::json!({"test_id": test_id})),
                            tags: None,
                            limit: Some(5),
                            offset: None,
                            cursor: None,
                            similarity_threshold: None,
                            include_metadata: None,
                            include_facets: None,
                            ranking_boost: None,
                            explain_score: None,
                        };
                        repo.search_memories(request).await.map(|_| ())
                    }
                    _ => {
                        // Read operation (get statistics)
                        repo.get_statistics().await.map(|_| ())
                    }
                }
            }
        },
        30, // 30 mixed operations
    )
    .await?;

    let mixed_result = mixed_meter.finish();
    mixed_result.assert_under(TokioDuration::from_secs(20));

    let mixed_ops_per_sec = mixed_result.operations_per_second(30);
    println!("Mixed workload: {:.1} ops/sec", mixed_ops_per_sec);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test error handling and system resilience
#[tokio::test]
#[traced_test]
async fn test_error_handling_and_resilience() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Extract values we need to avoid lifetime issues
    let repository = Arc::clone(&env.repository);
    let test_id = env.test_id.clone();

    // Test 1: Invalid memory ID handling
    let invalid_id = Uuid::new_v4();
    let result = repository.get_memory(invalid_id).await;
    assert!(
        result.is_err(),
        "Should return error for non-existent memory"
    );

    // Test 2: Invalid search parameters
    let invalid_search = SearchRequest {
        query_text: None,
        query_embedding: None, // Both text and embedding are None
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(-1), // Invalid limit
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: None,
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    // Should handle gracefully
    let search_result = repository.search_memories(invalid_search).await;
    match search_result {
        Ok(response) => assert!(
            response.results.is_empty(),
            "Invalid search should return empty results"
        ),
        Err(_) => {} // Proper error is also acceptable
    }

    // Test 3: Extremely large content handling
    let large_content = TestDataGenerator::large_content(100); // 100KB content
    let large_request = CreateMemoryRequest {
        content: large_content.clone(),
        embedding: None,
        tier: Some(MemoryTier::Cold),
        importance_score: Some(0.1),
        metadata: Some(serde_json::json!({"test_id": test_id, "size_test": true})),
        parent_id: None,
        expires_at: None,
    };

    // Should either handle gracefully or reject appropriately
    match repository.create_memory(large_request).await {
        Ok(memory) => {
            assert!(!memory.content.is_empty());
            println!("Large content accepted: {} bytes", memory.content.len());
        }
        Err(e) => {
            println!("Large content rejected (acceptable): {}", e);
        }
    }

    // Test 4: Malformed metadata handling
    let malicious_metadata = serde_json::json!({
        "script": "<script>alert('xss')</script>",
        "sql_injection": "'; DROP TABLE memories; --",
        "test_id": test_id
    });

    let metadata_request = CreateMemoryRequest {
        content: "Testing metadata security".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.5),
        metadata: Some(malicious_metadata.clone()),
        parent_id: None,
        expires_at: None,
    };

    let metadata_memory = repository.create_memory(metadata_request).await?;
    // Metadata should be stored safely without execution
    assert_eq!(
        metadata_memory.metadata["script"],
        "<script>alert('xss')</script>"
    );

    // Test 5: Concurrent stress test (should not cause deadlocks or corruption)
    // Stress operations test
    let repo_for_stress = Arc::clone(&repository);
    let test_id_for_stress = test_id.clone();
    let stress_operations: Vec<Result<(), anyhow::Error>> = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repo_for_stress);
            let test_id = test_id_for_stress.clone();
            async move {
                // Rapid creation and deletion
                let request = CreateMemoryRequest {
                    content: format!("Stress test memory {}", i),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.5),
                    metadata: Some(serde_json::json!({"test_id": test_id, "stress": true})),
                    parent_id: None,
                    expires_at: None,
                };

                if let Ok(memory) = repo.create_memory(request).await {
                    // Immediately try to delete
                    let _ = repo.delete_memory(memory.id).await;
                }
                Ok::<(), anyhow::Error>(())
            }
        },
        20,
    )
    .await?;

    // Should complete without panics or deadlocks
    assert_eq!(stress_operations.len(), 20);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test data persistence and consistency across database operations
#[tokio::test]
#[traced_test]
async fn test_data_persistence_and_consistency() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Create memory with all fields populated
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

    let created = env.repository.create_memory(comprehensive_request).await?;

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

    // Test 2: Verify access tracking
    let retrieved = env.repository.get_memory(created.id).await?;
    assert_eq!(retrieved.access_count, created.access_count + 1);
    assert!(retrieved.last_accessed_at.is_some());
    assert!(retrieved.last_accessed_at.unwrap() > created.created_at);

    // Test 3: Update and verify changes are persisted
    let update_request = UpdateMemoryRequest {
        content: Some("Updated content for persistence test".to_string()),
        embedding: None, // Should be regenerated
        tier: Some(MemoryTier::Warm),
        importance_score: Some(0.95),
        metadata: Some(env.get_test_metadata(Some(serde_json::json!({
            "updated": true,
            "update_timestamp": Utc::now().to_rfc3339()
        })))),
        expires_at: None, // Remove expiration
    };

    let updated = env
        .repository
        .update_memory(created.id, update_request)
        .await?;
    assert_eq!(updated.content, "Updated content for persistence test");
    assert_eq!(updated.tier, MemoryTier::Warm);
    assert_eq!(updated.importance_score, 0.95);
    assert!(updated.expires_at.is_none());
    assert!(updated.updated_at > created.updated_at);

    // Test 4: Verify consistency after multiple operations
    env.wait_for_consistency().await;

    let final_retrieval = env.repository.get_memory(created.id).await?;
    assert_eq!(final_retrieval.content, updated.content);
    assert_eq!(final_retrieval.tier, updated.tier);
    assert_eq!(final_retrieval.importance_score, updated.importance_score);

    // Test 5: Verify search can find the updated content
    let search_results = env
        .test_search("Updated content persistence", Some(5))
        .await?;
    let found_memory = search_results.iter().find(|r| r.memory.id == created.id);
    assert!(
        found_memory.is_some(),
        "Updated memory should be findable via search"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test memory operations with different content types and sizes
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

    // Search for conversational content
    let conv_results = env.test_search("user questions and help", Some(10)).await?;
    assert!(!conv_results.is_empty(), "Should find conversation content");

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

    // Test 5: Large content
    let large_content = TestDataGenerator::large_content(10); // 10KB
    let large_memory = env
        .create_test_memory(&large_content, MemoryTier::Cold, 0.3)
        .await?;
    assert!(
        large_memory.content.len() >= 10000,
        "Large content should be preserved"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test statistics and monitoring functionality
#[tokio::test]
#[traced_test]
async fn test_statistics_and_monitoring() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create a known set of memories for statistics testing
    let memories = env.create_test_memories(15).await?;
    assert_eq!(memories.len(), 15);

    // Wait for all operations to complete
    env.wait_for_consistency().await;

    // Test 1: Basic repository statistics
    let repo_stats = env.repository.get_statistics().await?;
    assert!(repo_stats.total_active.unwrap_or(0) >= 15);

    // Test 2: Test environment statistics
    let test_stats = env.get_test_statistics().await?;
    assert_eq!(test_stats.total_count, 15);
    assert!(test_stats.working_count > 0);
    assert!(test_stats.warm_count > 0);
    assert!(test_stats.cold_count > 0);
    assert!(test_stats.avg_importance > 0.0);

    // Test 3: Access pattern tracking
    // Access some memories multiple times
    for memory in &memories[0..5] {
        for _ in 0..3 {
            let _ = env.repository.get_memory(memory.id).await?;
        }
    }

    env.wait_for_consistency().await;

    // Verify access counts increased
    for memory in &memories[0..5] {
        let updated = env.repository.get_memory(memory.id).await?;
        assert!(
            updated.access_count >= 3,
            "Access count should reflect multiple reads"
        );
    }

    // Test 4: Search result statistics
    let search_request = SearchRequest {
        query_text: Some("test memory".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(env.get_test_metadata(None)),
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: Some(true),
        ranking_boost: None,
        explain_score: Some(true),
    };

    let search_response = env.repository.search_memories(search_request).await?;
    assert!(!search_response.results.is_empty());
    assert!(search_response.execution_time_ms > 0);

    if let Some(facets) = search_response.facets {
        assert!(!facets.tiers.is_empty());
    }

    for result in search_response.results {
        assert!(result.similarity_score >= 0.0);
        assert!(result.combined_score >= 0.0);

        if let Some(explanation) = result.score_explanation {
            assert!(explanation.total_score >= 0.0);
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}
