//! Chaos Engineering Tests for Memory System
//!
//! These tests validate system resilience by introducing controlled failures,
//! resource constraints, and adverse conditions to verify graceful degradation
//! and recovery capabilities.

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryTier, RangeFilter, SearchRequest, UpdateMemoryRequest,
};
use codex_memory::MemoryStatus;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use test_helpers::TestEnvironment;
use tokio::sync::Semaphore;
use tokio::time::{sleep, timeout, Instant};
use tracing_test::traced_test;
use uuid::Uuid;

/// Test system behavior under extreme memory pressure
#[tokio::test]
#[traced_test]
async fn test_memory_pressure_resilience() -> Result<()> {
    let env = TestEnvironment::new().await?;

    println!("Starting memory pressure resilience test...");

    // Phase 1: Create baseline load
    let baseline_memories = 50;
    let mut baseline_ids = Vec::new();

    for i in 0..baseline_memories {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("Baseline memory {} - {}", i, "x".repeat(1000)), // 1KB content
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(json!({
                    "baseline": true,
                    "index": i,
                    "size": "1kb"
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;

        baseline_ids.push(memory.id);
    }

    // Phase 2: Apply extreme memory pressure
    let pressure_memories = 200;
    let mut pressure_handles = Vec::new();
    let successful_creates = Arc::new(Mutex::new(0));
    let failed_creates = Arc::new(Mutex::new(0));

    let semaphore = Arc::new(Semaphore::new(20)); // Limit concurrent operations

    for i in 0..pressure_memories {
        let env_clone = env.clone();
        let success_counter = successful_creates.clone();
        let failure_counter = failed_creates.clone();
        let sem = semaphore.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            // Create large memory objects to pressure the system
            let large_content = "x".repeat(10000); // 10KB content
            let large_metadata = json!({
                "pressure_test": true,
                "index": i,
                "large_field_1": "x".repeat(1000),
                "large_field_2": "y".repeat(1000),
                "large_field_3": "z".repeat(1000),
                "timestamp": Utc::now().to_rfc3339()
            });

            match timeout(
                StdDuration::from_secs(10),
                env_clone.repository.create_memory(CreateMemoryRequest {
                    content: format!("Pressure memory {} - {}", i, large_content),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.8),
                    metadata: Some(large_metadata),
                    parent_id: None,
                    expires_at: Some(Utc::now() + Duration::minutes(5)), // Auto-expire
                }),
            )
            .await
            {
                Ok(Ok(memory)) => {
                    let mut counter = success_counter.lock().unwrap();
                    *counter += 1;
                    memory.id
                }
                Ok(Err(e)) => {
                    let mut counter = failure_counter.lock().unwrap();
                    *counter += 1;
                    eprintln!("Pressure memory creation failed: {}", e);
                    Uuid::nil()
                }
                Err(_) => {
                    let mut counter = failure_counter.lock().unwrap();
                    *counter += 1;
                    eprintln!("Pressure memory creation timed out");
                    Uuid::nil()
                }
            }
        });

        pressure_handles.push(handle);

        // Small delay to create gradual pressure
        if i % 10 == 0 {
            sleep(StdDuration::from_millis(100)).await;
        }
    }

    // Wait for pressure phase to complete
    let mut pressure_ids = Vec::new();
    for handle in pressure_handles {
        if let Ok(id) = handle.await {
            if id != Uuid::nil() {
                pressure_ids.push(id);
            }
        }
    }

    let successful = successful_creates.lock().unwrap();
    let failed = failed_creates.lock().unwrap();

    println!(
        "Memory pressure phase completed: {}/{} successful, {} failed",
        *successful, pressure_memories, *failed
    );

    // Phase 3: Verify baseline functionality still works
    println!("Verifying system responsiveness under pressure...");

    let mut responsive_operations = 0;
    let mut failed_operations = 0;

    // Test basic operations under pressure
    for baseline_id in &baseline_ids[..10] {
        // Test first 10
        match timeout(
            StdDuration::from_secs(5),
            env.repository.get_memory(*baseline_id),
        )
        .await
        {
            Ok(Ok(memory)) => {
                assert_eq!(memory.id, *baseline_id);
                responsive_operations += 1;
            }
            _ => {
                failed_operations += 1;
            }
        }
    }

    // Test search under pressure
    let search_request = SearchRequest {
        query_text: Some("Baseline memory".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(5),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    match timeout(
        StdDuration::from_secs(10),
        env.repository.search_memories(search_request),
    )
    .await
    {
        Ok(Ok(results)) => {
            assert!(
                !results.results.is_empty(),
                "Search should return results even under pressure"
            );
            responsive_operations += 1;
            println!(
                "Search under pressure returned {} results",
                results.results.len()
            );
        }
        _ => {
            failed_operations += 1;
            eprintln!("Search failed under memory pressure");
        }
    }

    println!(
        "System responsiveness: {}/{} operations successful",
        responsive_operations,
        responsive_operations + failed_operations
    );

    // Phase 4: Recovery verification
    println!("Testing system recovery...");

    // Clean up pressure memories to reduce load
    let mut cleanup_successful = 0;
    for pressure_id in pressure_ids {
        match env.repository.delete_memory(pressure_id).await {
            Ok(_) => cleanup_successful += 1,
            Err(e) => eprintln!("Cleanup failed for {}: {}", pressure_id, e),
        }
    }

    println!(
        "Cleanup completed: {} pressure memories removed",
        cleanup_successful
    );

    // Allow system to recover
    sleep(StdDuration::from_secs(2)).await;

    // Verify recovery
    let recovery_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Recovery test memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.9),
            metadata: Some(json!({"test": "recovery"})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let retrieved_recovery = env.repository.get_memory(recovery_memory.id).await?;
    assert_eq!(retrieved_recovery.content, "Recovery test memory");

    // Cleanup baseline memories
    for baseline_id in baseline_ids {
        let _ = env.repository.delete_memory(baseline_id).await;
    }
    env.repository.delete_memory(recovery_memory.id).await?;

    env.cleanup_test_data().await?;

    // Assert system maintained minimal functionality
    assert!(
        responsive_operations >= (responsive_operations + failed_operations) / 2,
        "System should maintain at least 50% functionality under pressure"
    );

    println!("✓ Memory pressure resilience test completed successfully");
    Ok(())
}

/// Test system behavior with intermittent failures
#[tokio::test]
#[traced_test]
async fn test_intermittent_failure_resilience() -> Result<()> {
    let env = TestEnvironment::new().await?;

    println!("Starting intermittent failure resilience test...");

    let num_operations = 100;
    let failure_injection_rate = 0.2; // 20% of operations will experience failures

    let mut test_memories = Vec::new();
    let successful_operations = Arc::new(Mutex::new(0));
    let recovered_operations = Arc::new(Mutex::new(0));
    let failed_operations = Arc::new(Mutex::new(0));

    // Create base memories for testing
    for i in 0..20 {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("Base memory {}", i),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(json!({"base": true, "index": i})),
                parent_id: None,
                expires_at: None,
            })
            .await?;

        test_memories.push(memory);
    }

    let mut handles = Vec::new();

    for op_id in 0..num_operations {
        let env_clone = env.clone();
        let memories_clone = test_memories.clone();
        let success_counter = successful_operations.clone();
        let recovery_counter = recovered_operations.clone();
        let failure_counter = failed_operations.clone();

        let handle = tokio::spawn(async move {
            let should_inject_failure =
                (op_id as f32) < (num_operations as f32 * failure_injection_rate);
            let max_retries = 3;
            let base_timeout =
                StdDuration::from_millis(if should_inject_failure { 500 } else { 2000 });

            for retry in 0..=max_retries {
                let operation_timeout = base_timeout + StdDuration::from_millis(retry as u64 * 500);

                let operation_result = timeout(operation_timeout, async {
                    match op_id % 4 {
                        0 => {
                            // Create operation
                            env_clone
                                .repository
                                .create_memory(CreateMemoryRequest {
                                    content: format!("Intermittent test {} retry {}", op_id, retry),
                                    embedding: None,
                                    tier: Some(MemoryTier::Working),
                                    importance_score: Some(0.6),
                                    metadata: Some(json!({
                                        "intermittent_test": true,
                                        "op_id": op_id,
                                        "retry": retry
                                    })),
                                    parent_id: None,
                                    expires_at: None,
                                })
                                .await
                                .map(|m| m.id)
                        }
                        1 => {
                            // Read operation
                            if !memories_clone.is_empty() {
                                let memory_idx = op_id % memories_clone.len();
                                env_clone
                                    .repository
                                    .get_memory(memories_clone[memory_idx].id)
                                    .await
                                    .map(|m| m.id)
                            } else {
                                Ok(Uuid::nil())
                            }
                        }
                        2 => {
                            // Update operation
                            if !memories_clone.is_empty() {
                                let memory_idx = op_id % memories_clone.len();
                                env_clone
                                    .repository
                                    .update_memory(
                                        memories_clone[memory_idx].id,
                                        UpdateMemoryRequest {
                                            content: Some(format!(
                                                "Updated {} retry {}",
                                                op_id, retry
                                            )),
                                            embedding: None,
                                            tier: None,
                                            importance_score: None,
                                            metadata: Some(
                                                json!({"updated_by": op_id, "retry": retry}),
                                            ),
                                            expires_at: None,
                                        },
                                    )
                                    .await
                                    .map(|m| m.id)
                            } else {
                                Ok(Uuid::nil())
                            }
                        }
                        _ => {
                            // Search operation
                            let search_request = SearchRequest {
                                query_text: Some("memory".to_string()),
                                query_embedding: None,
                                search_type: None,
                                hybrid_weights: None,
                                tier: None,
                                date_range: None,
                                importance_range: None,
                                metadata_filters: None,
                                tags: None,
                                limit: Some(5),
                                offset: None,
                                cursor: None,
                                similarity_threshold: None,
                                include_metadata: Some(true),
                                include_facets: None,
                                ranking_boost: None,
                                explain_score: None,
                            };

                            env_clone
                                .repository
                                .search_memories(search_request)
                                .await
                                .map(|_| Uuid::new_v4())
                        }
                    }
                })
                .await;

                match operation_result {
                    Ok(Ok(_)) => {
                        if retry == 0 {
                            let mut counter = success_counter.lock().unwrap();
                            *counter += 1;
                        } else {
                            let mut counter = recovery_counter.lock().unwrap();
                            *counter += 1;
                        }
                        return; // Success, exit retry loop
                    }
                    Ok(Err(e)) => {
                        if retry == max_retries {
                            let mut counter = failure_counter.lock().unwrap();
                            *counter += 1;
                            eprintln!(
                                "Operation {} failed after {} retries: {}",
                                op_id, max_retries, e
                            );
                        }
                    }
                    Err(_) => {
                        if retry == max_retries {
                            let mut counter = failure_counter.lock().unwrap();
                            *counter += 1;
                            eprintln!(
                                "Operation {} timed out after {} retries",
                                op_id, max_retries
                            );
                        }
                    }
                }

                // Exponential backoff between retries
                if retry < max_retries {
                    sleep(StdDuration::from_millis(100 * (2_u64.pow(retry as u32)))).await;
                }
            }
        });

        handles.push(handle);

        // Small delay between operations
        if op_id % 5 == 0 {
            sleep(StdDuration::from_millis(50)).await;
        }
    }

    // Wait for all operations to complete
    for handle in handles {
        let _ = handle.await;
    }

    let successful = *successful_operations.lock().unwrap();
    let recovered = *recovered_operations.lock().unwrap();
    let failed = *failed_operations.lock().unwrap();

    println!("Intermittent failure test results:");
    println!("  Successful on first try: {}", successful);
    println!("  Recovered after retry: {}", recovered);
    println!("  Failed completely: {}", failed);
    println!("  Total operations: {}", num_operations);

    // System should recover from most intermittent failures
    let total_successful = successful + recovered;
    let success_rate = total_successful as f32 / num_operations as f32;

    assert!(
        success_rate >= 0.8,
        "System should recover from intermittent failures with >80% success rate, got {:.1}%",
        success_rate * 100.0
    );

    // Cleanup
    for memory in test_memories {
        let _ = env.repository.delete_memory(memory.id).await;
    }

    env.cleanup_test_data().await?;

    println!("✓ Intermittent failure resilience test completed successfully");
    Ok(())
}

/// Test system behavior under resource exhaustion scenarios
#[tokio::test]
#[traced_test]
async fn test_resource_exhaustion_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;

    println!("Starting resource exhaustion handling test...");

    // Phase 1: Connection pool exhaustion simulation
    let connection_pressure_ops = 50;
    let concurrent_connections = Arc::new(Semaphore::new(1)); // Severely limit connections

    let mut connection_handles = Vec::new();
    let connection_successes = Arc::new(Mutex::new(0));
    let connection_failures = Arc::new(Mutex::new(0));

    for i in 0..connection_pressure_ops {
        let env_clone = env.clone();
        let sem = concurrent_connections.clone();
        let success_counter = connection_successes.clone();
        let failure_counter = connection_failures.clone();

        let handle = tokio::spawn(async move {
            match timeout(StdDuration::from_secs(5), async {
                let _permit = sem.acquire().await.unwrap();

                // Hold the connection for a while to create pressure
                sleep(StdDuration::from_millis(100)).await;

                env_clone
                    .repository
                    .create_memory(CreateMemoryRequest {
                        content: format!("Connection pressure test {}", i),
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.4),
                        metadata: Some(json!({"connection_test": true, "index": i})),
                        parent_id: None,
                        expires_at: None,
                    })
                    .await
            })
            .await
            {
                Ok(Ok(memory)) => {
                    let mut counter = success_counter.lock().unwrap();
                    *counter += 1;
                    memory.id
                }
                _ => {
                    let mut counter = failure_counter.lock().unwrap();
                    *counter += 1;
                    Uuid::nil()
                }
            }
        });

        connection_handles.push(handle);
    }

    let mut connection_ids = Vec::new();
    for handle in connection_handles {
        if let Ok(id) = handle.await {
            if id != Uuid::nil() {
                connection_ids.push(id);
            }
        }
    }

    let conn_success = *connection_successes.lock().unwrap();
    let conn_failures = *connection_failures.lock().unwrap();

    println!(
        "Connection pressure test: {}/{} successful",
        conn_success, connection_pressure_ops
    );

    // Phase 2: Disk space exhaustion simulation (large content)
    let large_content_ops = 20;
    let huge_content = "x".repeat(100_000); // 100KB per memory

    let mut large_content_ids = Vec::new();
    let mut disk_successes = 0;
    let mut disk_failures = 0;

    for i in 0..large_content_ops {
        match timeout(
            StdDuration::from_secs(10),
            env.repository.create_memory(CreateMemoryRequest {
                content: format!("Large content test {} - {}", i, huge_content),
                embedding: None,
                tier: Some(MemoryTier::Cold), // Use cold tier for large content
                importance_score: Some(0.2),
                metadata: Some(json!({
                    "large_content_test": true,
                    "index": i,
                    "size": "100kb"
                })),
                parent_id: None,
                expires_at: Some(Utc::now() + Duration::minutes(1)), // Auto-expire quickly
            }),
        )
        .await
        {
            Ok(Ok(memory)) => {
                large_content_ids.push(memory.id);
                disk_successes += 1;
            }
            _ => {
                disk_failures += 1;
            }
        }

        // Brief pause to observe gradual resource exhaustion
        sleep(StdDuration::from_millis(200)).await;
    }

    println!(
        "Disk pressure test: {}/{} successful",
        disk_successes, large_content_ops
    );

    // Phase 3: CPU exhaustion simulation (complex operations)
    let cpu_intensive_ops = 30;
    let cpu_semaphore = Arc::new(Semaphore::new(5)); // Limit CPU-intensive operations

    let mut cpu_handles = Vec::new();
    let cpu_successes = Arc::new(Mutex::new(0));

    for i in 0..cpu_intensive_ops {
        let env_clone = env.clone();
        let sem = cpu_semaphore.clone();
        let success_counter = cpu_successes.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            // Simulate CPU-intensive work with complex search
            let search_request = SearchRequest {
                query_text: Some("connection pressure large content test memory".to_string()),
                query_embedding: None,
                search_type: None,
                hybrid_weights: None,
                tier: None,
                date_range: None,
                importance_range: Some(RangeFilter {
                    min: Some(0.0),
                    max: Some(1.0),
                }),
                metadata_filters: None,
                tags: None,
                limit: Some(100),
                offset: None,
                cursor: None,
                similarity_threshold: Some(0.1),
                include_metadata: Some(true),
                include_facets: Some(true),
                ranking_boost: None,
                explain_score: Some(true),
            };

            match timeout(
                StdDuration::from_secs(15),
                env_clone.repository.search_memories(search_request),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let mut counter = success_counter.lock().unwrap();
                    *counter += 1;
                }
                _ => {
                    eprintln!("CPU-intensive operation {} failed or timed out", i);
                }
            }

            // Simulate additional CPU work
            sleep(StdDuration::from_millis(100)).await;
        });

        cpu_handles.push(handle);
    }

    for handle in cpu_handles {
        let _ = handle.await;
    }

    let cpu_success = *cpu_successes.lock().unwrap();
    println!(
        "CPU pressure test: {}/{} successful",
        cpu_success, cpu_intensive_ops
    );

    // Phase 4: Verify system graceful degradation
    println!("Testing graceful degradation...");

    let degradation_test_start = Instant::now();

    // Simple operations should still work even under resource pressure
    let simple_memory = timeout(
        StdDuration::from_secs(10),
        env.repository.create_memory(CreateMemoryRequest {
            content: "Simple degradation test".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7),
            metadata: Some(json!({"test": "degradation"})),
            parent_id: None,
            expires_at: None,
        }),
    )
    .await;

    match simple_memory {
        Ok(Ok(memory)) => {
            println!("✓ Simple operations still work under resource pressure");

            // Verify we can retrieve it
            let retrieved = timeout(
                StdDuration::from_secs(5),
                env.repository.get_memory(memory.id),
            )
            .await;
            match retrieved {
                Ok(Ok(retrieved_memory)) => {
                    assert_eq!(retrieved_memory.id, memory.id);
                    println!("✓ Retrieval operations work under pressure");
                }
                _ => {
                    eprintln!("⚠ Retrieval failed under resource pressure");
                }
            }

            // Cleanup
            let _ = env.repository.delete_memory(memory.id).await;
        }
        _ => {
            println!("⚠ System may be severely degraded - simple operations failing");
        }
    }

    let degradation_test_duration = degradation_test_start.elapsed();
    println!(
        "Degradation test completed in {:?}",
        degradation_test_duration
    );

    // Phase 5: Recovery testing
    println!("Testing system recovery...");

    // Clean up resource-intensive memories
    for id in connection_ids {
        let _ = env.repository.delete_memory(id).await;
    }
    for id in large_content_ids {
        let _ = env.repository.delete_memory(id).await;
    }

    // Allow system to recover
    sleep(StdDuration::from_secs(3)).await;

    // Test normal operations after cleanup
    let recovery_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Recovery test after resource exhaustion".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.9),
            metadata: Some(json!({"test": "recovery"})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let recovery_search = SearchRequest {
        query_text: Some("recovery test".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let recovery_results = env.repository.search_memories(recovery_search).await?;
    assert!(
        !recovery_results.results.is_empty(),
        "System should recover and be functional"
    );

    env.repository.delete_memory(recovery_memory.id).await?;
    env.cleanup_test_data().await?;

    // Assert system handled resource exhaustion gracefully
    let overall_success_rate = (conn_success + disk_successes + cpu_success) as f32
        / (connection_pressure_ops + large_content_ops + cpu_intensive_ops) as f32;

    println!(
        "Overall success rate under resource pressure: {:.1}%",
        overall_success_rate * 100.0
    );

    // System should maintain some functionality even under severe pressure
    assert!(
        overall_success_rate >= 0.3,
        "System should maintain at least 30% functionality under resource exhaustion"
    );

    println!("✓ Resource exhaustion handling test completed successfully");
    Ok(())
}

/// Test system behavior during partial component failures
#[tokio::test]
#[traced_test]
async fn test_partial_component_failure_resilience() -> Result<()> {
    let env = TestEnvironment::new().await?;

    println!("Starting partial component failure resilience test...");

    // Phase 1: Create baseline functionality test
    let baseline_operations = vec![
        ("create", "Create operation"),
        ("read", "Read operation"),
        ("update", "Update operation"),
        ("search", "Search operation"),
    ];

    let mut baseline_memory_id = None;

    // Establish baseline
    for (op_type, description) in &baseline_operations {
        match *op_type {
            "create" => {
                let memory = env
                    .repository
                    .create_memory(CreateMemoryRequest {
                        content: "Baseline test memory".to_string(),
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.5),
                        metadata: Some(json!({"baseline": true})),
                        parent_id: None,
                        expires_at: None,
                    })
                    .await?;

                baseline_memory_id = Some(memory.id);
                println!("✓ {} baseline established", description);
            }
            "read" => {
                if let Some(id) = baseline_memory_id {
                    let _memory = env.repository.get_memory(id).await?;
                    println!("✓ {} baseline established", description);
                }
            }
            "update" => {
                if let Some(id) = baseline_memory_id {
                    let _updated = env
                        .repository
                        .update_memory(
                            id,
                            UpdateMemoryRequest {
                                content: Some("Updated baseline memory".to_string()),
                                embedding: None,
                                tier: None,
                                importance_score: None,
                                metadata: None,
                                expires_at: None,
                            },
                        )
                        .await?;
                    println!("✓ {} baseline established", description);
                }
            }
            "search" => {
                let search_request = SearchRequest {
                    query_text: Some("baseline".to_string()),
                    query_embedding: None,
                    search_type: None,
                    hybrid_weights: None,
                    tier: None,
                    date_range: None,
                    importance_range: None,
                    metadata_filters: None,
                    tags: None,
                    limit: Some(5),
                    offset: None,
                    cursor: None,
                    similarity_threshold: None,
                    include_metadata: Some(true),
                    include_facets: None,
                    ranking_boost: None,
                    explain_score: None,
                };

                let _results = env.repository.search_memories(search_request).await?;
                println!("✓ {} baseline established", description);
            }
            _ => {}
        }
    }

    // Phase 2: Simulate partial failures with fallback behavior
    let failure_scenarios = vec![
        ("timeout_simulation", "Network timeout simulation"),
        (
            "partial_data_corruption",
            "Partial data corruption simulation",
        ),
        (
            "concurrent_access_conflict",
            "Concurrent access conflict simulation",
        ),
    ];

    let mut scenario_results = Vec::new();

    for (scenario, description) in failure_scenarios {
        println!("Testing scenario: {}", description);

        let scenario_start = Instant::now();
        let mut operations_attempted = 0;
        let mut operations_successful = 0;
        let mut operations_gracefully_failed = 0;

        match scenario {
            "timeout_simulation" => {
                // Simulate operations that might timeout
                for i in 0..10 {
                    operations_attempted += 1;

                    let timeout_duration = if i < 3 {
                        StdDuration::from_secs(5) // Normal timeout
                    } else {
                        StdDuration::from_millis(500) // Short timeout to simulate failure
                    };

                    match timeout(
                        timeout_duration,
                        env.repository.create_memory(CreateMemoryRequest {
                            content: format!("Timeout test memory {}", i),
                            embedding: None,
                            tier: Some(MemoryTier::Working),
                            importance_score: Some(0.4),
                            metadata: Some(json!({"timeout_test": true, "index": i})),
                            parent_id: None,
                            expires_at: None,
                        }),
                    )
                    .await
                    {
                        Ok(Ok(memory)) => {
                            operations_successful += 1;
                            let _ = env.repository.delete_memory(memory.id).await;
                            // Cleanup
                        }
                        Ok(Err(_)) => operations_gracefully_failed += 1,
                        Err(_) => operations_gracefully_failed += 1, // Timeout
                    }
                }
            }

            "partial_data_corruption" => {
                // Simulate handling of partially corrupted data scenarios
                for i in 0..8 {
                    operations_attempted += 1;

                    // Create memory with potentially problematic data
                    let problematic_content = if i % 3 == 0 {
                        format!("Normal content {}", i)
                    } else {
                        format!(
                            "Content with special chars: {} \0 \x01 {}",
                            i,
                            "🚀".repeat(100)
                        )
                    };

                    let problematic_metadata = if i % 4 == 0 {
                        json!({"normal": true, "index": i})
                    } else {
                        json!({
                            "index": i,
                            "large_nested": {
                                "deep": {
                                    "very_deep": {
                                        "extremely_deep": "x".repeat(1000)
                                    }
                                }
                            },
                            "null_value": serde_json::Value::Null,
                            "empty_string": "",
                            "zero_number": 0,
                            "negative": -1
                        })
                    };

                    match env
                        .repository
                        .create_memory(CreateMemoryRequest {
                            content: problematic_content,
                            embedding: None,
                            tier: Some(MemoryTier::Working),
                            importance_score: Some(0.3),
                            metadata: Some(problematic_metadata),
                            parent_id: None,
                            expires_at: None,
                        })
                        .await
                    {
                        Ok(memory) => {
                            operations_successful += 1;
                            // Try to retrieve to verify data integrity
                            match env.repository.get_memory(memory.id).await {
                                Ok(_) => {}
                                Err(_) => {
                                    eprintln!("Data corruption detected in memory {}", memory.id);
                                    operations_gracefully_failed += 1;
                                }
                            }
                            let _ = env.repository.delete_memory(memory.id).await;
                            // Cleanup
                        }
                        Err(_) => operations_gracefully_failed += 1,
                    }
                }
            }

            "concurrent_access_conflict" => {
                // Simulate concurrent access conflicts
                if let Some(test_id) = baseline_memory_id {
                    let mut conflict_handles = Vec::new();
                    let conflict_results = Arc::new(Mutex::new((0, 0))); // (success, failure)

                    for i in 0..6 {
                        operations_attempted += 1;

                        let env_clone = env.clone();
                        let results = conflict_results.clone();

                        let handle = tokio::spawn(async move {
                            // Concurrent update operations
                            match env_clone
                                .repository
                                .update_memory(
                                    test_id,
                                    UpdateMemoryRequest {
                                        content: Some(format!("Concurrent update {}", i)),
                                        embedding: None,
                                        tier: None,
                                        importance_score: Some((0.1 * i as f32) as f64),
                                        metadata: Some(json!({"concurrent": true, "thread": i})),
                                        expires_at: None,
                                    },
                                )
                                .await
                            {
                                Ok(_) => {
                                    let mut results = results.lock().unwrap();
                                    results.0 += 1; // success
                                }
                                Err(_) => {
                                    let mut results = results.lock().unwrap();
                                    results.1 += 1; // failure
                                }
                            }
                        });

                        conflict_handles.push(handle);
                    }

                    for handle in conflict_handles {
                        let _ = handle.await;
                    }

                    let results = conflict_results.lock().unwrap();
                    operations_successful += results.0;
                    operations_gracefully_failed += results.1;
                }
            }

            _ => {}
        }

        let scenario_duration = scenario_start.elapsed();
        let success_rate = operations_successful as f32 / operations_attempted as f32;

        println!(
            "  Scenario '{}' completed in {:?}",
            scenario, scenario_duration
        );
        println!(
            "    Operations: {} attempted, {} successful, {} gracefully failed",
            operations_attempted, operations_successful, operations_gracefully_failed
        );
        println!("    Success rate: {:.1}%", success_rate * 100.0);

        scenario_results.push((scenario, success_rate, scenario_duration));

        // Brief recovery pause between scenarios
        sleep(StdDuration::from_millis(500)).await;
    }

    // Phase 3: Verify system state after all failure scenarios
    println!("Verifying system state after failure scenarios...");

    if let Some(test_id) = baseline_memory_id {
        match env.repository.get_memory(test_id).await {
            Ok(memory) => {
                println!(
                    "✓ Original baseline memory still accessible: {}",
                    memory.content
                );
            }
            Err(e) => {
                println!("⚠ Baseline memory affected by failure scenarios: {}", e);
            }
        }

        let _ = env.repository.delete_memory(test_id).await;
    }

    // Final system health check
    let health_check_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Post-chaos health check".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            metadata: Some(json!({"health_check": true})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let health_retrieved = env.repository.get_memory(health_check_memory.id).await?;
    assert_eq!(health_retrieved.content, "Post-chaos health check");

    env.repository.delete_memory(health_check_memory.id).await?;
    env.cleanup_test_data().await?;

    // Verify overall resilience
    let average_success_rate = scenario_results
        .iter()
        .map(|(_, rate, _)| rate)
        .sum::<f32>()
        / scenario_results.len() as f32;

    println!("\nPartial failure resilience summary:");
    for (scenario, rate, duration) in scenario_results {
        println!(
            "  {}: {:.1}% success in {:?}",
            scenario,
            rate * 100.0,
            duration
        );
    }
    println!(
        "  Average success rate: {:.1}%",
        average_success_rate * 100.0
    );

    // System should handle partial failures gracefully
    assert!(
        average_success_rate >= 0.5,
        "System should maintain at least 50% success rate during partial failures"
    );

    println!("✓ Partial component failure resilience test completed successfully");
    Ok(())
}
