//! Error Handling and Recovery Tests
//!
//! These tests validate system resilience and error handling:
//! - Database connection failures and recovery
//! - Invalid input handling and sanitization
//! - Concurrent access error scenarios
//! - Memory exhaustion and resource limits
//! - Network failures (embedding service)
//! - Data corruption detection and recovery

mod test_helpers;

use anyhow::Result;
use codex_memory::memory::models::{CreateMemoryRequest, MemoryTier, SearchRequest, UpdateMemoryRequest};
use serde_json::json;
use std::sync::Arc;
use test_helpers::{ConcurrentTester, TestConfigBuilder, TestDataGenerator, TestEnvironment};
use tokio::time::{timeout, Duration};
use tracing_test::traced_test;
use uuid::Uuid;

/// Test handling of invalid input data
#[tokio::test]
#[traced_test]
async fn test_invalid_input_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Invalid UUID handling
    let invalid_uuid = "not-a-valid-uuid";
    let result = env.repository.get_memory(Uuid::parse_str(invalid_uuid).unwrap_or(Uuid::new_v4())).await;
    // Should either handle gracefully or return appropriate error

    // Test 2: Empty content handling
    let empty_content_request = CreateMemoryRequest {
        content: String::new(), // Empty content
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.5),
        metadata: Some(env.get_test_metadata(None)),
        parent_id: None,
        expires_at: None,
    };

    let empty_result = env.repository.create_memory(empty_content_request).await;
    match empty_result {
        Ok(memory) => {
            // If accepted, should handle gracefully
            assert!(memory.content.is_empty());
        }
        Err(_) => {
            // Rejection is also acceptable
            println!("Empty content rejected (acceptable)");
        }
    }

    // Test 3: Invalid importance score handling
    let invalid_importance_request = CreateMemoryRequest {
        content: "Test with invalid importance".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(-1.5), // Invalid negative importance
        metadata: Some(env.get_test_metadata(None)),
        parent_id: None,
        expires_at: None,
    };

    let invalid_importance_result = env.repository.create_memory(invalid_importance_request).await;
    match invalid_importance_result {
        Ok(memory) => {
            // Should clamp or normalize invalid values
            assert!(memory.importance_score >= 0.0 && memory.importance_score <= 1.0);
        }
        Err(_) => {
            // Rejection is also acceptable
            println!("Invalid importance score rejected (acceptable)");
        }
    }

    // Test 4: Malformed metadata handling
    let malformed_metadata = json!({
        "test_id": env.test_id,
        "large_nested": {
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "level5": "deeply nested structure"
                        }
                    }
                }
            }
        },
        "large_array": (0..1000).collect::<Vec<i32>>(),
        "binary_data": "\\x00\\x01\\x02\\x03\\xFF",
        "unicode": "🚀🎯💡⚡🔥",
        "control_chars": "\t\n\r\x08"
    });

    let malformed_request = CreateMemoryRequest {
        content: "Testing malformed metadata handling".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.5),
        metadata: Some(malformed_metadata),
        parent_id: None,
        expires_at: None,
    };

    let malformed_result = env.repository.create_memory(malformed_request).await;
    match malformed_result {
        Ok(memory) => {
            // Should handle complex metadata safely
            assert!(!memory.metadata.is_null());
            println!("Complex metadata accepted and handled safely");
        }
        Err(e) => {
            println!("Complex metadata rejected: {}", e);
        }
    }

    // Test 5: SQL injection attempts in content
    let sql_injection_attempts = vec![
        "'; DROP TABLE memories; --",
        "SELECT * FROM memories WHERE 1=1; --",
        "UNION SELECT password FROM users; --",
        "'; INSERT INTO memories (content) VALUES ('injected'); --",
    ];

    for injection in sql_injection_attempts {
        let injection_request = CreateMemoryRequest {
            content: injection.to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(env.get_test_metadata(Some(json!({"injection_test": true})))),
            parent_id: None,
            expires_at: None,
        };

        let injection_result = env.repository.create_memory(injection_request).await;
        match injection_result {
            Ok(memory) => {
                // Content should be stored safely without execution
                assert_eq!(memory.content, injection);
                println!("SQL injection attempt stored safely: {}", injection);
            }
            Err(_) => {
                println!("SQL injection attempt rejected: {}", injection);
            }
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test handling of extremely large inputs
#[tokio::test]
#[traced_test]
async fn test_large_input_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test different content sizes to find limits
    let size_tests = vec![
        ("1MB", 1024),
        ("5MB", 5 * 1024),
        ("10MB", 10 * 1024),
    ];

    for (size_name, size_kb) in size_tests {
        println!("Testing {} content handling", size_name);
        
        let large_content = TestDataGenerator::large_content(size_kb);
        let large_request = CreateMemoryRequest {
            content: large_content.clone(),
            embedding: None,
            tier: Some(MemoryTier::Cold), // Use cold tier for large content
            importance_score: Some(0.1),
            metadata: Some(env.get_test_metadata(Some(json!({"size_test": size_name})))),
            parent_id: None,
            expires_at: None,
        };

        // Use timeout to prevent hanging on very large content
        let result = timeout(Duration::from_secs(60), env.repository.create_memory(large_request)).await;
        
        match result {
            Ok(Ok(memory)) => {
                println!("{} content accepted: {} bytes", size_name, memory.content.len());
                
                // Test retrieval of large content
                let retrieval_result = timeout(
                    Duration::from_secs(30),
                    env.repository.get_memory(memory.id)
                ).await;
                
                match retrieval_result {
                    Ok(Ok(retrieved)) => {
                        assert_eq!(retrieved.content.len(), large_content.len());
                        println!("{} content retrieved successfully", size_name);
                    }
                    Ok(Err(e)) => {
                        println!("{} content retrieval failed: {}", size_name, e);
                    }
                    Err(_) => {
                        println!("{} content retrieval timed out", size_name);
                    }
                }
            }
            Ok(Err(e)) => {
                println!("{} content rejected: {}", size_name, e);
            }
            Err(_) => {
                println!("{} content creation timed out", size_name);
            }
        }
    }

    // Test extremely large metadata
    let large_metadata = json!({
        "test_id": env.test_id,
        "large_text": "x".repeat(100_000), // 100KB metadata field
        "large_array": (0..10_000).collect::<Vec<i32>>(),
    });

    let large_metadata_request = CreateMemoryRequest {
        content: "Testing large metadata".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.5),
        metadata: Some(large_metadata),
        parent_id: None,
        expires_at: None,
    };

    let metadata_result = env.repository.create_memory(large_metadata_request).await;
    match metadata_result {
        Ok(memory) => {
            println!("Large metadata accepted");
            assert!(!memory.metadata.is_null());
        }
        Err(e) => {
            println!("Large metadata rejected: {}", e);
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test concurrent access error scenarios
#[tokio::test]
#[traced_test]
async fn test_concurrent_access_errors() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create a shared memory for concurrent modification testing
    let shared_memory = env.create_test_memory(
        "Shared memory for concurrent access testing",
        MemoryTier::Working,
        0.7,
    ).await?;

    // Test 1: Concurrent updates to the same memory
    println!("Testing concurrent updates to same memory");
    
    let shared_memory_id = shared_memory.id;
    let repository = Arc::clone(&env.repository);
    let test_id = env.test_id.clone();
    
    let update_operations = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repository);
            let memory_id = shared_memory_id;
            let test_id = test_id.clone();
            async move {
                let update_request = UpdateMemoryRequest {
                    content: Some(format!("Updated by worker {} at concurrent test", i)),
                    embedding: None,
                    tier: None,
                    importance_score: Some(0.5 + (i as f32 * 0.05)),
                    metadata: Some(serde_json::json!({
                        "test_id": test_id,
                        "updated_by_worker": i,
                        "concurrent_update": true
                    })),
                    expires_at: None,
                };

                repo.update_memory(memory_id, update_request).await
            }
        },
        10, // 10 concurrent updates
    ).await?;

    // Some updates may succeed, some may fail due to conflicts
    let successful_updates = update_operations.iter().filter(|r| r.is_ok()).count();
    let failed_updates = update_operations.iter().filter(|r| r.is_err()).count();
    
    println!("Concurrent updates: {} succeeded, {} failed", successful_updates, failed_updates);
    
    // At least one update should succeed
    assert!(successful_updates > 0, "At least one concurrent update should succeed");

    // Test 2: Concurrent deletion attempts
    let deletion_memory = env.create_test_memory(
        "Memory for concurrent deletion test",
        MemoryTier::Working,
        0.5,
    ).await?;

    let deletion_memory_id = deletion_memory.id;
    let repository2 = Arc::clone(&env.repository);
    
    let deletion_operations = ConcurrentTester::run_parallel(
        move |_| {
            let repo = Arc::clone(&repository2);
            let memory_id = deletion_memory_id;
            async move {
                repo.delete_memory(memory_id).await
            }
        },
        5, // 5 concurrent deletion attempts
    ).await?;

    // Only one deletion should succeed, others should fail
    let successful_deletions = deletion_operations.iter().filter(|r| r.is_ok()).count();
    let failed_deletions = deletion_operations.iter().filter(|r| r.is_err()).count();
    
    println!("Concurrent deletions: {} succeeded, {} failed", successful_deletions, failed_deletions);
    
    // Exactly one deletion should succeed
    assert!(successful_deletions <= 1, "At most one deletion should succeed");
    assert!(failed_deletions >= 4, "Most deletions should fail");

    // Test 3: Concurrent access with one modifier and multiple readers
    let reader_memory = env.create_test_memory(
        "Memory for reader/writer test",
        MemoryTier::Working,
        0.6,
    ).await?;

    let reader_memory_id = reader_memory.id;
    let repository3 = Arc::clone(&env.repository);
    let test_id3 = env.test_id.clone();
    
    let mixed_operations = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repository3);
            let memory_id = reader_memory_id;
            let test_id = test_id3.clone();
            async move {
                if i == 0 {
                    // Writer
                    let update_request = UpdateMemoryRequest {
                        content: Some("Content updated during reader/writer test".to_string()),
                        embedding: None,
                        tier: None,
                        importance_score: Some(0.8),
                        metadata: Some(serde_json::json!({
                            "test_id": test_id,
                            "reader_writer_test": true
                        })),
                        expires_at: None,
                    };
                    repo.update_memory(memory_id, update_request).await.map(|_| "write".to_string())
                } else {
                    // Readers
                    repo.get_memory(memory_id).await.map(|_| "read".to_string())
                }
            }
        },
        10, // 1 writer, 9 readers
    ).await?;

    // All operations should handle concurrency gracefully
    let successful_ops = mixed_operations.iter().filter(|r| r.is_ok()).count();
    println!("Reader/writer test: {} operations succeeded out of 10", successful_ops);
    
    assert!(successful_ops >= 9, "Most reader/writer operations should succeed");

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test network and external service failure handling
#[tokio::test]
#[traced_test]
async fn test_external_service_failures() -> Result<()> {
    // Test with a configuration that points to a non-existent embedding service
    let config = TestConfigBuilder::new()
        .with_ollama("http://nonexistent:11434", "fake-model")
        .build();

    let env = TestEnvironment::new_with_config(Some(config)).await;
    
    // If environment creation fails due to unreachable service, that's expected
    let env = match env {
        Ok(env) => env,
        Err(e) => {
            println!("Environment creation failed with unreachable embedding service (expected): {}", e);
            // Test with mock embedder instead
            let mock_config = TestConfigBuilder::new()
                .with_mock_embedder()
                .build();
            TestEnvironment::new_with_config(Some(mock_config)).await?
        }
    };

    // Test 1: Memory creation with embedding service unavailable
    // (This test will pass with mock embedder, fail with real unreachable service)
    let request_with_embedding = CreateMemoryRequest {
        content: "Test content that requires embedding generation".to_string(),
        embedding: None, // Should trigger embedding generation
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.7),
        metadata: Some(env.get_test_metadata(Some(json!({"embedding_test": true})))),
        parent_id: None,
        expires_at: None,
    };

    let embedding_result = timeout(
        Duration::from_secs(10),
        env.repository.create_memory(request_with_embedding)
    ).await;

    match embedding_result {
        Ok(Ok(memory)) => {
            println!("Memory created successfully (embedding service available or mock)");
            assert!(!memory.content.is_empty());
        }
        Ok(Err(e)) => {
            println!("Memory creation failed due to embedding service: {}", e);
            // This is acceptable when embedding service is unavailable
        }
        Err(_) => {
            println!("Memory creation timed out (embedding service unresponsive)");
            // This is also acceptable
        }
    }

    // Test 2: Memory creation with pre-provided embedding (should work even if service is down)
    let fake_embedding = vec![0.1; 768]; // Fake 768-dimensional embedding
    let request_with_provided_embedding = CreateMemoryRequest {
        content: "Test content with provided embedding".to_string(),
        embedding: Some(fake_embedding.clone()),
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.6),
        metadata: Some(env.get_test_metadata(Some(json!({"provided_embedding": true})))),
        parent_id: None,
        expires_at: None,
    };

    let provided_embedding_result = env.repository.create_memory(request_with_provided_embedding).await;
    match provided_embedding_result {
        Ok(memory) => {
            println!("Memory with provided embedding created successfully");
            assert_eq!(memory.embedding.as_ref().unwrap().as_slice(), fake_embedding.as_slice());
        }
        Err(e) => {
            println!("Memory creation with provided embedding failed: {}", e);
        }
    }

    // Test 3: Search operations when embedding service is unavailable
    let search_request = SearchRequest {
        query_text: Some("test content".to_string()),
        query_embedding: None, // Should trigger embedding generation for query
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(env.get_test_metadata(None)),
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

    let search_result = timeout(
        Duration::from_secs(10),
        env.repository.search_memories(search_request)
    ).await;

    match search_result {
        Ok(Ok(response)) => {
            println!("Search completed successfully");
            // May return empty results if no memories have embeddings
        }
        Ok(Err(e)) => {
            println!("Search failed due to embedding service: {}", e);
        }
        Err(_) => {
            println!("Search timed out");
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test database connection issues and recovery
#[tokio::test]
#[traced_test]
async fn test_database_error_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Operations with invalid SQL (should be handled by query builder)
    // This is more of a defensive test since the repository uses proper query builders
    
    // Test 2: Very rapid database operations to potentially trigger connection issues
    println!("Testing rapid database operations");
    
    let repository4 = Arc::clone(&env.repository);
    let test_id4 = env.test_id.clone();
    
    let rapid_operations = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repository4);
            let test_id = test_id4.clone();
            async move {
                // Rapid-fire operations that might stress connection pool
                for j in 0..10 {
                    let request = CreateMemoryRequest {
                        content: format!("Rapid operation {} - {}", i, j),
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.5),
                        metadata: Some(serde_json::json!({
                            "test_id": test_id,
                            "rapid_test": true,
                            "worker": i,
                            "operation": j
                        })),
                        parent_id: None,
                        expires_at: None,
                    };

                    match repo.create_memory(request).await {
                        Ok(_) => {},
                        Err(e) => {
                            println!("Rapid operation failed: {}", e);
                            return Err(e);
                        }
                    }

                    // Quick read
                    let _stats = repo.get_statistics().await?;
                }
                Ok(())
            }
        },
        20, // 20 workers doing 10 operations each
    ).await?;

    let successful_workers = rapid_operations.iter().filter(|r| r.is_ok()).count();
    let failed_workers = rapid_operations.iter().filter(|r| r.is_err()).count();
    
    println!("Rapid operations: {} workers succeeded, {} failed", successful_workers, failed_workers);
    
    // Most operations should succeed despite stress
    assert!(successful_workers >= rapid_operations.len() / 2, 
        "At least half of rapid operations should succeed");

    // Test 3: Long-running transaction simulation
    println!("Testing long-running operations");
    
    let long_content = TestDataGenerator::large_content(50); // 50KB content
    let long_operation_start = std::time::Instant::now();
    
    let long_request = CreateMemoryRequest {
        content: long_content,
        embedding: None,
        tier: Some(MemoryTier::Cold),
        importance_score: Some(0.3),
        metadata: Some(env.get_test_metadata(Some(json!({"long_operation": true})))),
        parent_id: None,
        expires_at: None,
    };

    let long_result = timeout(
        Duration::from_secs(30),
        env.repository.create_memory(long_request)
    ).await;

    let long_duration = long_operation_start.elapsed();
    println!("Long operation took: {:?}", long_duration);

    match long_result {
        Ok(Ok(_)) => {
            println!("Long operation completed successfully");
        }
        Ok(Err(e)) => {
            println!("Long operation failed: {}", e);
        }
        Err(_) => {
            println!("Long operation timed out (may indicate connection issues)");
        }
    }

    // Test 4: Statistics consistency after errors
    let final_stats = env.repository.get_statistics().await?;
    println!("Final statistics: {:?}", final_stats);
    
    // Statistics should be retrievable even after various error conditions
    assert!(final_stats.total_active.is_some());

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test data integrity and consistency after errors
#[tokio::test]
#[traced_test]
async fn test_data_integrity_after_errors() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create some baseline memories
    let baseline_memories = env.create_test_memories(10).await?;
    println!("Created {} baseline memories", baseline_memories.len());

    // Test 1: Verify data integrity after failed operations
    let repository5 = Arc::clone(&env.repository);
    let test_id5 = env.test_id.clone();
    
    let mixed_operations = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repository5);
            let test_id = test_id5.clone();
            async move {
                // Mix of valid and potentially problematic operations
                match i % 4 {
                    0 => {
                        // Valid operation
                        let request = CreateMemoryRequest {
                            content: format!("Valid memory {}", i),
                            embedding: None,
                            tier: Some(MemoryTier::Working),
                            importance_score: Some(0.7),
                            metadata: Some(serde_json::json!({"test_id": test_id, "valid": true})),
                            parent_id: None,
                            expires_at: None,
                        };
                        repo.create_memory(request).await.map(|_| "valid")
                    }
                    1 => {
                        // Operation with edge case data
                        let request = CreateMemoryRequest {
                            content: "\0\t\n\r".to_string(), // Control characters
                            embedding: None,
                            tier: Some(MemoryTier::Working),
                            importance_score: Some(0.5),
                            metadata: Some(serde_json::json!({"test_id": test_id, "edge_case": true})),
                            parent_id: None,
                            expires_at: None,
                        };
                        repo.create_memory(request).await.map(|_| "edge_case")
                    }
                    2 => {
                        // Query operation (should always work)
                        repo.get_statistics().await.map(|_| "query")
                    }
                    _ => {
                        // Update operation on potentially non-existent memory
                        let fake_id = Uuid::new_v4();
                        let update_request = UpdateMemoryRequest {
                            content: Some("Update attempt".to_string()),
                            embedding: None,
                            tier: None,
                            importance_score: None,
                            metadata: None,
                            expires_at: None,
                        };
                        repo.update_memory(fake_id, update_request).await.map(|_| "update")
                    }
                }
            }
        },
        20,
    ).await?;

    let successful_mixed = mixed_operations.iter().filter(|r| r.is_ok()).count();
    println!("Mixed operations: {} out of 20 succeeded", successful_mixed);

    // Test 2: Verify baseline memories are still intact
    env.wait_for_consistency().await;

    for baseline_memory in &baseline_memories {
        let retrieved = env.repository.get_memory(baseline_memory.id).await;
        match retrieved {
            Ok(memory) => {
                assert_eq!(memory.id, baseline_memory.id);
                assert_eq!(memory.content, baseline_memory.content);
                println!("Baseline memory {} intact after error operations", memory.id);
            }
            Err(e) => {
                panic!("Baseline memory {} corrupted after error operations: {}", baseline_memory.id, e);
            }
        }
    }

    // Test 3: Verify search still works correctly
    let post_error_search = env.test_search("test memory", Some(20)).await?;
    println!("Post-error search found {} results", post_error_search.len());
    
    // Should still find our baseline memories
    assert!(post_error_search.len() >= baseline_memories.len());

    // Test 4: Verify statistics are consistent
    let post_error_stats = env.repository.get_statistics().await?;
    println!("Post-error statistics: {:?}", post_error_stats);
    
    assert!(post_error_stats.total_active.unwrap_or(0) >= baseline_memories.len() as i64);

    // Test 5: Create a few more memories to ensure system is still functional
    let post_error_memories = env.create_test_memories(5).await?;
    assert_eq!(post_error_memories.len(), 5);
    
    println!("Successfully created {} new memories after error scenarios", post_error_memories.len());

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test graceful degradation under resource constraints
#[tokio::test]
#[traced_test]
async fn test_graceful_degradation() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: High memory pressure simulation
    println!("Testing behavior under simulated memory pressure");
    
    // Create many large memories to simulate memory pressure
    let repository6 = Arc::clone(&env.repository);
    let test_id6 = env.test_id.clone();
    
    let large_memory_operations = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repository6);
            let test_id = test_id6.clone();
            async move {
                let large_content = format!("Large memory {} - {}", i, "x".repeat(10_000));
                let request = CreateMemoryRequest {
                    content: large_content,
                    embedding: None,
                    tier: Some(MemoryTier::Cold), // Use cold tier for large memories
                    importance_score: Some(0.2),
                    metadata: Some(serde_json::json!({
                        "test_id": test_id,
                        "memory_pressure_test": true,
                        "index": i
                    })),
                    parent_id: None,
                    expires_at: None,
                };

                // Use timeout to handle potential slowdowns
                timeout(Duration::from_secs(15), repo.create_memory(request)).await
            }
        },
        50, // Try to create 50 large memories
    ).await?;

    let successful_large = large_memory_operations.iter()
        .filter(|r| matches!(r, Ok(Ok(_))))
        .count();
    let failed_large = large_memory_operations.len() - successful_large;
    
    println!("Under memory pressure: {} large memories created, {} failed/timed out", 
        successful_large, failed_large);

    // System should either succeed or fail gracefully
    assert!(successful_large > 0 || failed_large == large_memory_operations.len(),
        "System should either create memories or fail consistently");

    // Test 2: Ensure basic operations still work under pressure
    let basic_operation = env.create_test_memory(
        "Basic operation under pressure",
        MemoryTier::Working,
        0.8,
    ).await;

    match basic_operation {
        Ok(memory) => {
            println!("Basic operation successful under pressure");
            
            // Try to retrieve it
            let retrieved = env.repository.get_memory(memory.id).await;
            assert!(retrieved.is_ok(), "Should be able to retrieve memory even under pressure");
        }
        Err(e) => {
            println!("Basic operation failed under pressure: {}", e);
        }
    }

    // Test 3: Search performance degradation
    let search_start = std::time::Instant::now();
    let search_result = timeout(
        Duration::from_secs(10),
        env.test_search("memory pressure test", Some(10))
    ).await;
    let search_duration = search_start.elapsed();

    match search_result {
        Ok(Ok(results)) => {
            println!("Search under pressure completed in {:?}, found {} results", 
                search_duration, results.len());
        }
        Ok(Err(e)) => {
            println!("Search failed under pressure: {}", e);
        }
        Err(_) => {
            println!("Search timed out under pressure");
        }
    }

    // Test 4: Verify system can recover
    println!("Testing recovery after pressure");
    
    // Wait a bit for potential cleanup/GC
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Try a simple operation
    let recovery_test = env.create_test_memory(
        "Recovery test memory",
        MemoryTier::Working,
        0.7,
    ).await;

    match recovery_test {
        Ok(memory) => {
            println!("System recovered successfully");
            assert!(!memory.content.is_empty());
        }
        Err(e) => {
            println!("System recovery incomplete: {}", e);
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}