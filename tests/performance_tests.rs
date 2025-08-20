//! Performance and Load Testing for the Agentic Memory System
//!
//! These tests validate system performance under various load conditions:
//! - High-volume memory operations
//! - Concurrent user simulation
//! - Memory tier stress testing
//! - Embedding generation performance
//! - Database performance under load

mod test_helpers;

use anyhow::Result;
use codex_memory::memory::models::{CreateMemoryRequest, MemoryTier, SearchRequest};
use criterion::Criterion;
use std::sync::Arc;
use test_helpers::{ConcurrentTester, PerformanceMeter, TestDataGenerator, TestEnvironment};
use tokio::time::{Duration, Instant};
use tracing_test::traced_test;

/// Performance baseline tests
#[tokio::test]
#[traced_test]
async fn test_baseline_operation_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Single memory creation baseline
    let create_meter = PerformanceMeter::new("single_memory_creation");

    let memory = env
        .create_test_memory(
            "Baseline performance test memory with typical content length",
            MemoryTier::Working,
            0.7,
        )
        .await?;

    let create_result = create_meter.finish();
    create_result.assert_under(Duration::from_millis(2000)); // Should be under 2 seconds
    println!("Single memory creation: {:?}", create_result.duration);

    // Test 2: Single memory retrieval baseline
    let get_meter = PerformanceMeter::new("single_memory_retrieval");

    let retrieved = env.repository.get_memory(memory.id).await?;
    assert_eq!(retrieved.id, memory.id);

    let get_result = get_meter.finish();
    get_result.assert_under(Duration::from_millis(100)); // Should be under 100ms
    println!("Single memory retrieval: {:?}", get_result.duration);

    // Test 3: Single search baseline
    let search_meter = PerformanceMeter::new("single_search");

    let search_results = env
        .test_search("baseline performance test", Some(10))
        .await?;
    assert!(!search_results.is_empty());

    let search_result = search_meter.finish();
    search_result.assert_under(Duration::from_millis(1000)); // Should be under 1 second
    println!("Single search: {:?}", search_result.duration);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test high-volume memory creation performance
#[tokio::test]
#[traced_test]
async fn test_high_volume_memory_creation() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test various batch sizes
    let batch_sizes = vec![10, 50, 100, 250];

    for batch_size in batch_sizes {
        println!("Testing batch size: {}", batch_size);

        let batch_meter = PerformanceMeter::new(&format!("batch_creation_{}", batch_size));

        // Create handles manually to avoid lifetime issues
        let mut handles = Vec::new();
        let repo = Arc::clone(&env.repository);
        let test_id = env.test_id.clone();

        for i in 0..batch_size {
            let repo_clone = Arc::clone(&repo);
            let test_id_clone = test_id.clone();
            let handle = tokio::spawn(async move {
                let request = CreateMemoryRequest {
                    content: format!("High volume test memory {} with sufficient content to be realistic for testing purposes", i),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.5 + ((i % 100) as f64) * 0.005),
                    metadata: Some(serde_json::json!({
                        "test_id": test_id_clone,
                        "high_volume_test": true,
                        "batch_size": batch_size,
                        "index": i
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

        let batch_result = batch_meter.finish();
        let ops_per_second = batch_result.operations_per_second(batch_size);

        println!(
            "Batch size {}: {:.1} ops/sec, total time: {:?}",
            batch_size, ops_per_second, batch_result.duration
        );

        // Performance assertions based on batch size
        match batch_size {
            10 => {
                batch_result.assert_under(Duration::from_secs(10));
                assert!(
                    ops_per_second >= 1.0,
                    "Should achieve at least 1 op/sec for small batches"
                );
            }
            50 => {
                batch_result.assert_under(Duration::from_secs(30));
                assert!(
                    ops_per_second >= 1.5,
                    "Should achieve at least 1.5 ops/sec for medium batches"
                );
            }
            100 => {
                batch_result.assert_under(Duration::from_secs(60));
                assert!(
                    ops_per_second >= 1.5,
                    "Should maintain performance for larger batches"
                );
            }
            250 => {
                batch_result.assert_under(Duration::from_secs(180)); // 3 minutes for large batch
                assert!(
                    ops_per_second >= 1.0,
                    "Should maintain minimum performance for very large batches"
                );
            }
            _ => {}
        }

        assert_eq!(creation_operations.len(), batch_size);
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test concurrent user simulation
#[tokio::test]
#[traced_test]
async fn test_concurrent_user_simulation() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Simulate different numbers of concurrent users
    let user_counts = vec![5, 10, 20];
    let operations_per_user = 10;

    for user_count in user_counts {
        println!("Testing {} concurrent users", user_count);

        let concurrent_meter = PerformanceMeter::new(&format!("concurrent_users_{}", user_count));

        // Create handles manually for concurrent users
        let mut handles = Vec::new();
        let repo = Arc::clone(&env.repository);

        for user_id in 0..user_count {
            let repo_clone = Arc::clone(&repo);
            let test_id_clone = env.test_id.clone();
            let handle = tokio::spawn(async move {
                let mut user_memories = Vec::new();
                // Each user performs a series of operations
                for op_id in 0..operations_per_user {
                    // Create memory
                    let create_request = CreateMemoryRequest {
                        content: format!("User {} operation {} - content with realistic length for testing concurrent access patterns", user_id, op_id),
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.5 + (op_id as f64 * 0.05)),
                        metadata: Some(serde_json::json!({
                            "test_id": test_id_clone,
                            "user_id": user_id,
                            "operation_id": op_id,
                            "concurrent_test": true
                        })),
                        parent_id: None,
                        expires_at: None,
                    };

                    let memory = repo_clone.create_memory(create_request).await?;
                    user_memories.push(memory.id);

                    // Immediately read it back
                    let _retrieved = repo_clone.get_memory(memory.id).await?;

                    // Every 3rd operation, do a search
                    if op_id % 3 == 0 {
                        let search_request = SearchRequest {
                            query_text: Some(format!("User {} operation", user_id)),
                            query_embedding: None,
                            search_type: None,
                            hybrid_weights: None,
                            tier: None,
                            date_range: None,
                            importance_range: None,
                            metadata_filters: Some(serde_json::json!({
                                "test_id": test_id_clone,
                                "user_id": user_id
                            })),
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

                        let _search_results = repo_clone.search_memories(search_request).await?;
                    }
                }

                Ok::<Vec<uuid::Uuid>, anyhow::Error>(user_memories)
            });
            handles.push(handle);
        }

        let mut user_operations = Vec::new();
        for handle in handles {
            user_operations.push(handle.await??);
        }

        let concurrent_result = concurrent_meter.finish();
        let total_operations = user_count * operations_per_user;
        let ops_per_second = concurrent_result.operations_per_second(total_operations);

        println!(
            "Concurrent users {}: {:.1} ops/sec, total time: {:?}",
            user_count, ops_per_second, concurrent_result.duration
        );

        // Performance assertions
        match user_count {
            5 => {
                concurrent_result.assert_under(Duration::from_secs(45));
                assert!(
                    ops_per_second >= 1.0,
                    "Should maintain performance with 5 users"
                );
            }
            10 => {
                concurrent_result.assert_under(Duration::from_secs(90));
                assert!(
                    ops_per_second >= 0.8,
                    "Should maintain reasonable performance with 10 users"
                );
            }
            20 => {
                concurrent_result.assert_under(Duration::from_secs(180)); // 3 minutes
                assert!(
                    ops_per_second >= 0.5,
                    "Should maintain minimum performance with 20 users"
                );
            }
            _ => {}
        }

        // Verify all users completed their operations
        assert_eq!(user_operations.len(), user_count);
        for user_memories in &user_operations {
            assert_eq!(user_memories.len(), operations_per_user);
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test search performance with different result set sizes
#[tokio::test]
#[traced_test]
async fn test_search_performance_scaling() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let test_id = env.test_id.clone();

    // Create a large dataset for search testing
    let dataset_size = 500;
    println!(
        "Creating dataset of {} memories for search testing",
        dataset_size
    );

    let setup_meter = PerformanceMeter::new("dataset_creation");

    // Create diverse content for better search testing
    let content_types = vec![
        ("rust programming", "coding"),
        ("database optimization", "technical"),
        ("user interface design", "design"),
        ("api documentation", "docs"),
        ("performance testing", "testing"),
        ("security best practices", "security"),
        ("deployment strategies", "devops"),
        ("error handling patterns", "debugging"),
    ];

    let repo_arc = Arc::clone(&env.repository);
    let test_id_string = env.test_id.clone();
    let content_types_arc = Arc::new(content_types.clone());
    let _creation_operations = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repo_arc);
            let test_id = test_id_string.clone();
            let content_types = Arc::clone(&content_types_arc);
            async move {
                let content_type = content_types[i % content_types.len()];
                let request = CreateMemoryRequest {
                    content: format!("{} - Memory {} with detailed content about {} for comprehensive search testing",
                        content_type.0, i, content_type.1),
                    embedding: None,
                    tier: Some(match i % 3 {
                        0 => MemoryTier::Working,
                        1 => MemoryTier::Warm,
                        _ => MemoryTier::Cold,
                    }),
                    importance_score: Some(0.1 + ((i % 100) as f64) * 0.009),
                    metadata: Some(serde_json::json!({
                        "test_id": test_id,
                        "search_test": true,
                        "category": content_type.1,
                        "index": i
                    })),
                    parent_id: None,
                    expires_at: None,
                };
                repo.create_memory(request).await
            }
        },
        dataset_size,
    ).await?;

    let setup_result = setup_meter.finish();
    println!("Dataset creation took: {:?}", setup_result.duration);

    // Wait for embeddings and indexing
    env.wait_for_consistency().await;

    // Test search performance with different query types and limits
    let search_scenarios = vec![
        ("programming", 10),
        ("database optimization", 25),
        ("performance testing", 50),
        ("comprehensive search", 100),
    ];

    for (query, limit) in search_scenarios {
        let search_meter =
            PerformanceMeter::new(&format!("search_{}_{}", query.replace(" ", "_"), limit));

        let search_request = SearchRequest {
            query_text: Some(query.to_string()),
            query_embedding: None,
            search_type: None,
            hybrid_weights: None,
            tier: None,
            date_range: None,
            importance_range: None,
            metadata_filters: Some(serde_json::json!({"test_id": test_id})),
            tags: None,
            limit: Some(limit),
            offset: None,
            cursor: None,
            similarity_threshold: None,
            include_metadata: Some(true),
            include_facets: None,
            ranking_boost: None,
            explain_score: None,
        };

        let search_response = env.repository.search_memories(search_request).await?;
        let search_result = search_meter.finish();

        println!(
            "Search '{}' (limit {}): {:?}, found {} results",
            query,
            limit,
            search_result.duration,
            search_response.results.len()
        );

        // Performance assertions
        match limit {
            10 => search_result.assert_under(Duration::from_millis(500)),
            25 => search_result.assert_under(Duration::from_millis(800)),
            50 => search_result.assert_under(Duration::from_secs(1)),
            100 => search_result.assert_under(Duration::from_secs(2)),
            _ => {}
        }

        assert!(search_response.results.len() <= limit as usize);
    }

    // Test concurrent search performance
    println!("Testing concurrent search performance");
    let concurrent_search_meter = PerformanceMeter::new("concurrent_searches");

    let repo_search = Arc::clone(&env.repository);
    let test_id_search = env.test_id.clone();
    let content_types_search = Arc::new(content_types.clone());
    let _concurrent_searches = ConcurrentTester::run_parallel(
        move |i| {
            let repo = Arc::clone(&repo_search);
            let test_id = test_id_search.clone();
            let content_types = Arc::clone(&content_types_search);
            async move {
                let query = content_types[i % content_types.len()].0;
                let request = SearchRequest {
                    query_text: Some(query.to_string()),
                    query_embedding: None,
                    search_type: None,
                    hybrid_weights: None,
                    tier: None,
                    date_range: None,
                    importance_range: None,
                    metadata_filters: Some(serde_json::json!({"test_id": test_id})),
                    tags: None,
                    limit: Some(20),
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
        },
        15, // 15 concurrent searches
    )
    .await?;

    let concurrent_search_result = concurrent_search_meter.finish();
    let search_ops_per_sec = concurrent_search_result.operations_per_second(15);

    println!(
        "Concurrent searches: {:.1} searches/sec, total time: {:?}",
        search_ops_per_sec, concurrent_search_result.duration
    );

    concurrent_search_result.assert_under(Duration::from_secs(10));
    assert!(
        search_ops_per_sec >= 1.5,
        "Should achieve good concurrent search performance"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test memory tier migration performance
#[tokio::test]
#[traced_test]
async fn test_memory_tier_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create references we'll need throughout the function
    let repository = Arc::clone(&env.repository);
    let test_id = env.test_id.clone();

    // Create memories in different tiers
    let memories_per_tier = 100;
    println!(
        "Creating {} memories per tier for tier performance testing",
        memories_per_tier
    );

    let tiers = vec![MemoryTier::Working, MemoryTier::Warm, MemoryTier::Cold];
    let mut all_memories = Vec::new();

    for tier in &tiers {
        let tier_meter = PerformanceMeter::new(&format!("create_{:?}_tier", tier));

        let current_tier = *tier;
        let repo_for_tier = Arc::clone(&repository);
        let test_id_for_tier = test_id.clone();
        let tier_operations = ConcurrentTester::run_parallel(
            move |i| {
                let repo = Arc::clone(&repo_for_tier);
                let test_id = test_id_for_tier.clone();
                async move {
                    let request = CreateMemoryRequest {
                        content: format!(
                            "Tier {:?} memory {} for performance testing with sufficient content",
                            current_tier, i
                        ),
                        embedding: None,
                        tier: Some(current_tier),
                        importance_score: Some(match current_tier {
                            MemoryTier::Working => 0.8 + (i as f64 * 0.002),
                            MemoryTier::Warm => 0.5 + (i as f64 * 0.003),
                            MemoryTier::Cold => 0.2 + (i as f64 * 0.001),
                        }),
                        metadata: Some(serde_json::json!({
                            "test_id": test_id,
                            "tier_test": true,
                            "target_tier": format!("{:?}", current_tier),
                            "index": i
                        })),
                        parent_id: None,
                        expires_at: None,
                    };
                    repo.create_memory(request).await
                }
            },
            memories_per_tier,
        )
        .await?;

        let tier_result = tier_meter.finish();
        println!(
            "Created {} {:?} tier memories in {:?}",
            memories_per_tier, tier, tier_result.duration
        );

        // Unwrap the Results and collect only successful memories
        for result in tier_operations {
            match result {
                Ok(memory) => all_memories.push(memory),
                Err(e) => eprintln!("Failed to create memory: {:?}", e),
            }
        }
    }

    env.wait_for_consistency().await;

    // Test cross-tier search performance
    let cross_tier_meter = PerformanceMeter::new("cross_tier_search");

    for tier in &tiers {
        let tier_search = SearchRequest {
            query_text: Some("performance testing".to_string()),
            query_embedding: None,
            search_type: None,
            hybrid_weights: None,
            tier: Some(*tier),
            date_range: None,
            importance_range: None,
            metadata_filters: Some(serde_json::json!({"test_id": test_id})),
            tags: None,
            limit: Some(50),
            offset: None,
            cursor: None,
            similarity_threshold: None,
            include_metadata: None,
            include_facets: None,
            ranking_boost: None,
            explain_score: None,
        };

        let tier_results = repository.search_memories(tier_search).await?;
        println!(
            "Found {} results in {:?} tier",
            tier_results.results.len(),
            tier
        );

        // Verify tier filtering worked
        for result in tier_results.results {
            assert_eq!(result.memory.tier, *tier);
        }
    }

    let cross_tier_result = cross_tier_meter.finish();
    cross_tier_result.assert_under(Duration::from_secs(5));

    // Test tier-based access patterns (simulate working memory being accessed more frequently)
    let access_pattern_meter = PerformanceMeter::new("tier_access_patterns");

    // Access working tier memories more frequently
    for memory in &all_memories[0..memories_per_tier] {
        // Working tier memories
        for _ in 0..5 {
            let _ = repository.get_memory(memory.id).await?;
        }
    }

    // Access warm tier memories occasionally
    for memory in &all_memories[memories_per_tier..memories_per_tier * 2] {
        // Warm tier memories
        for _ in 0..2 {
            let _ = repository.get_memory(memory.id).await?;
        }
    }

    // Access cold tier memories rarely
    for memory in &all_memories[memories_per_tier * 2..] {
        // Cold tier memories
        let _ = repository.get_memory(memory.id).await?;
    }

    let access_pattern_result = access_pattern_meter.finish();
    println!(
        "Tier access pattern simulation took: {:?}",
        access_pattern_result.duration
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test embedding generation performance
#[tokio::test]
#[traced_test]
async fn test_embedding_generation_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test different content sizes
    let content_sizes = vec![
        ("small", 50),    // 50 characters
        ("medium", 500),  // 500 characters
        ("large", 2000),  // 2KB
        ("xlarge", 5000), // 5KB
    ];

    for (size_name, char_count) in content_sizes {
        println!(
            "Testing embedding generation for {} content ({} chars)",
            size_name, char_count
        );

        let content =
            TestDataGenerator::large_content(char_count / 1024 + 1)[..char_count].to_string();

        let embedding_meter = PerformanceMeter::new(&format!("embedding_{}", size_name));

        let embedding = env.embedder.generate_embedding(&content).await?;

        let embedding_result = embedding_meter.finish();
        println!(
            "Embedding generation for {} content: {:?} (dimension: {})",
            size_name,
            embedding_result.duration,
            embedding.len()
        );

        // Performance assertions based on content size
        match size_name {
            "small" => embedding_result.assert_under(Duration::from_millis(500)),
            "medium" => embedding_result.assert_under(Duration::from_secs(1)),
            "large" => embedding_result.assert_under(Duration::from_secs(3)),
            "xlarge" => embedding_result.assert_under(Duration::from_secs(5)),
            _ => {}
        }

        assert!(!embedding.is_empty());
        assert_eq!(embedding.len(), env.embedder.embedding_dimension());
    }

    // Test batch embedding generation performance
    let batch_meter = PerformanceMeter::new("batch_embeddings");

    let batch_content: Vec<String> = (0..20)
        .map(|i| {
            format!(
                "Batch embedding test content {} with sufficient length for realistic testing",
                i
            )
        })
        .collect();

    let batch_embeddings = env
        .embedder
        .generate_embeddings_batch(&batch_content)
        .await?;

    let batch_result = batch_meter.finish();
    println!(
        "Batch embedding generation (20 items): {:?}",
        batch_result.duration
    );

    batch_result.assert_under(Duration::from_secs(30));
    assert_eq!(batch_embeddings.len(), 20);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test system resource usage and limits
#[tokio::test]
#[traced_test]
async fn test_resource_usage_and_limits() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Extract values we need to avoid lifetime issues
    let repository = Arc::clone(&env.repository);
    let test_id = env.test_id.clone();

    // Test memory usage growth with large datasets
    let memory_usage_meter = PerformanceMeter::new("memory_usage_test");

    // Create progressively larger batches and monitor
    let batch_sizes = vec![100, 500, 1000];

    for batch_size in batch_sizes {
        println!("Testing resource usage with {} memories", batch_size);

        let batch_start = Instant::now();

        let current_batch_size = batch_size;
        let repo_for_batch = Arc::clone(&repository);
        let test_id_for_batch = test_id.clone();
        let operations = ConcurrentTester::run_parallel(
            move |i| {
                let repo = Arc::clone(&repo_for_batch);
                let test_id = test_id_for_batch.clone();
                async move {
                    let content = format!("Resource usage test memory {} with substantial content to simulate realistic memory consumption patterns during high-volume operations", i);
                    let request = CreateMemoryRequest {
                        content,
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.5),
                        metadata: Some(serde_json::json!({
                            "test_id": test_id,
                            "resource_test": true,
                            "batch_size": current_batch_size,
                            "index": i
                        })),
                        parent_id: None,
                        expires_at: None,
                    };
                    repo.create_memory(request).await
                }
            },
            batch_size,
        ).await?;

        let batch_duration = batch_start.elapsed();
        let ops_per_sec = batch_size as f64 / batch_duration.as_secs_f64();

        println!(
            "Batch size {}: {:.1} ops/sec, duration: {:?}",
            batch_size, ops_per_sec, batch_duration
        );

        // Verify all operations completed
        assert_eq!(operations.len(), batch_size);

        // Test search performance with large dataset
        let search_start = Instant::now();
        let search_results = env.test_search("resource usage test", Some(50)).await?;
        let search_duration = search_start.elapsed();

        println!(
            "Search with {} memories: {:?}, found {} results",
            batch_size,
            search_duration,
            search_results.len()
        );

        // Performance should not degrade significantly with larger datasets
        assert!(
            search_duration < Duration::from_secs(2),
            "Search performance should remain good even with {} memories",
            batch_size
        );
    }

    let memory_usage_result = memory_usage_meter.finish();
    println!(
        "Total resource usage test duration: {:?}",
        memory_usage_result.duration
    );

    // Test database connection efficiency
    let connection_meter = PerformanceMeter::new("connection_efficiency");

    // Rapid-fire operations to test connection pool efficiency
    // Rapid-fire operations to test connection pool efficiency
    let repo_for_rapid = Arc::clone(&repository);
    let rapid_operations = ConcurrentTester::run_parallel(
        move |_i| {
            let repo = Arc::clone(&repo_for_rapid);
            async move {
                // Quick read operation
                let _stats = repo.get_statistics().await?;
                Ok::<(), anyhow::Error>(())
            }
        },
        50, // 50 concurrent connection requests
    )
    .await?;

    let connection_result = connection_meter.finish();
    println!(
        "Connection efficiency test (50 operations): {:?}",
        connection_result.duration
    );

    connection_result.assert_under(Duration::from_secs(5));
    assert_eq!(rapid_operations.len(), 50);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Benchmark functions for criterion (if we want detailed benchmarking)
fn benchmark_memory_operations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Note: These benchmarks would need the test environment to be set up
    // For now, this is a placeholder showing how to structure criterion benchmarks

    c.bench_function("memory_creation", |b| {
        b.iter(|| {
            // Benchmark memory creation
            // This would need async handling in criterion
        });
    });

    c.bench_function("memory_search", |b| {
        b.iter(|| {
            // Benchmark search operations
        });
    });
}

// Uncomment these lines if you want to run criterion benchmarks
// criterion_group!(benches, benchmark_memory_operations);
// criterion_main!(benches);
